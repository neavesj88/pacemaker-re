# A one-button "Favourite" for the Pacemaker: research and patch plan

Static analysis only, nothing run on a device, nothing built. Tags: **[V]** verified in code
(decompiled, disassembled or read from the ELF), **[I]** inferred from verified facts, **[H]**
hypothesis, untested.

Target: firmware 180096069, `bin/pmplayer` (sha256 `4c5d502c...797ab`, 11,286,972 bytes). Ghidra
project `work/ghidra/fw-180096069-pmplayer`, addresses are the ELF virtual addresses (file offset =
address - 0x8000 in the text segment). Scratch output is in `work/fav/` (gitignored). One new helper,
`scripts/ghidra/DisasmAt.java` (prints the disassembly with raw bytes of the function containing an
address, or of `range:<start>-<end>`); no existing script was changed.

## Short answer

Add the favourite with a **headphone-modifier + CHANNEL1 (or CHANNEL2)** chord, delivered by a small
`LD_PRELOAD` library that patches two 8-byte hook sites in `pmplayer` at run time. The library calls
the player's own `tl::Repo` / `CaseOperations` code (every needed symbol is exported by name), so it
creates the case "Favourites" if missing, refuses duplicates, adds the highlighted track and shows the
stock "Added to case" banner. The `pmplayer` file in the image stays byte-identical, and deleting one
line in the wrapper script turns the feature off.

## 1. Input path and key bindings

### 1.1 Hardware to UI [V]

- `pmplayer` is **ARM mode, not Thumb**: 22,435 defined functions in `.dynsym`, none with an odd
  address; all disassembly seen is 4-byte ARM (`ldrls pc,[pc,r3,lsl #2]` jump tables, `bx lr`). ELF is
  `ET_EXEC` (non-PIE), EABI4, GCC 4.1.1 (`.comment`), dynamically linked (`/lib/ld-linux.so.3`;
  needs libasound, librt, libdirectfb-1.0, libfusion, libdirect, libpthread, libdl, libstdc++, libm,
  libgcc_s, libc).
- DirectFB thread: `CRTEventCatcher` (a `CDFBEventCatcher`, i.e. a `juce::Thread`) gets raw DFB
  events in `CDFBEventCatcher::DispatchInputEvent` (0x352ce4). Key symbols map to `CEventCatcher::KEY`
  (`CDFBEventCatcher::DFBKeyToEventKey` 0x352bec). `CEventCatcher::KeyToDesc` (0x353380) gives the
  button names, so the physical set is:

| Physical key (KeyToDesc) | DFB keysym | KEY | `pecomms::DeviceButton` |
|---|---|---|---|
| BTN_CHANNEL1 | 0xf629 | 1 | 1 |
| BTN_CHANNEL2 | 0xf62a | 2 | 2 |
| BTN_PLAYPAUSE | 0xf646 | 3 | 3 |
| BTN_CUE | 0xf647 | 4 | 4 |
| BTN_SEARCH_FW | 0xf645 | 5 | 9 |
| BTN_SEARCH_BW | 0xf640 | 6 | 10 (0xa) |
| BTN_PITCH_MOD (modifier, juce "shift") | 0xf631/0xf632 | 7 | 5 ("Alt" in the player) |
| BTN_HEADPHONE_MOD (modifier, juce "ctrl") | 0xf633/0xf634 | 8 | 6 |
| BTN_FX | 0xf625 | 9 | 0xb |
| BTN_POWER | 0xf626 | 10 | 0xe (menu on release) |

  KEY to DeviceButton is the switch in `CRTEventCatcher::OnKeyPress/OnKeyRelease` (0x3548a0 /
  0x354b50), which sends a `pecomms::LowLevelEvent` to the real-time side
  (`pecomms::rtec2rt::SendLowLevelEvents` 0x3e36d0). DeviceButtons 7, 8, 0xc, 0xd (loop in/out, sync,
  digital in `bob::Player::IRtec2Rt_lowLevelEvents` 0x3ee140) are produced by **no** DFB key; where
  they come from was not traced.
- **Two consumers get every key.** (a) The real-time player, above. (b) The UI: the same DFB keysyms
  arrive as juce key events on the focused `RawInputComponent`. `RawInputComponent::keyPressed`
  (0x34f5f8) maps the juce key code to a DeviceButton through the table at 0xacf1c0 (0xf629->1,
  0xf62a->2, 0xf646->3, 0xf647->4, 0xf645->9, 0xf640->0xa, 0xf625->0xb, 0xf626->0xe, read from the
  ELF) and calls `IRawInput::keyDown(DeviceButton)` (vtable +0x18). Auto-repeat is filtered by the
  down-state array at 0xad1c68, so each press gives one keyDown. `RawInputComponent::modifierKeysChanged`
  (0x34f7f4) turns the shift/ctrl modifier bits into DeviceButton 5 / 6 and keeps the modifier state
  in that static block (+0x14 shift, +0x18 ctrl). `pwrApplication` also sees the keys (its own
  `CEventCatcher`); `FunctionalStateMachine::OnKeyPress` (pwrApplication 0x6bde8) treats KEY 10
  (POWER) specially (HDD flush, timer) and is also where the boot combos of ssd-swap.md section 3
  live. Do not use POWER.
- `IRawInput` implementors: `EventMediator` (the global fallback sink), `PmListComponent`,
  `PwrMgr` (posts a juce message, activity wake), `ParameterComponent` and `SettingString` (empty),
  `RawInputEater` (empty). `EventMediator` is at `PMRunningApp+0x3140`, its `IRawInput` base at
  +0x3148 (entered through thunk `_ZThn8_...` so the real method sees the `EventMediator*`).

### 1.2 Bindings per screen [V unless tagged]

- **Browser / track list** (`PmTrackList::IList_keyDown` 0x394590, `IList_keyUp` 0x39182c; `this` =
  `PMRunningApp+0x18e0`). Jump table over DeviceButton 3..10:
  PLAY(3) and CUE(4) emit a track-list request (`emitTrackList` 0x394208, probably the load-to-deck
  action [I]; variant chosen by the headphone-held flag at `this+0x13f8` and the current channel); ALT(5) down opens the filter
  navigation (`FilterMediator::show`), up leaves it; HEADPHONE_MOD(6) sets `this+0x13f8 = 1` and calls
  `pecomms::ui2rt::SendHeadphoneBtn` (up clears both); SEARCH_FW/BW(9/10) skip to the next first
  letter only if the list is ordered by title (`isCurrentListOrderedByTitle` 0x3935f4), otherwise
  nothing; POWER(0xe) **key up** opens `SettingMediator::show(..., this)` = the "Track list menu"
  (Add to case etc.). DeviceButtons 1, 2, 7, 8, 0xb, 0xc, 0xd fall into the jump-table default
  (address 0x394678, reached by `b` at 0x3945c4 and by table slots 7 and 8), which forwards them to
  `EventMediator::IRawInput_keyDown` through the `IRawInput*` stored at `this+0x2b4` [I: set by
  `PmTrackList::setInterface`, same object as `PMRunningApp+0x3148`].
- **Playing view** (no list on top, `TrackArea` focused, keys go straight to
  `EventMediator::IRawInput_keyDown` 0x349b34, 64 bytes): ALT(5) down sets `EventMediator+0x44`,
  up clears it; CH1/CH2 with that flag set toggle the time-left readout of deck 0/1
  (`TrackArea::toggleTimeLeft`); everything else is ignored by the UI. `TrackArea+0x919` = track list
  shown, `+0x91a` = menu shown (`EventMediator::IPmTrackList_isShown` 0x349dc8 sets both).
- **Real-time side** (`bob::Player::IRtec2Rt_lowLevelEvents` 0x3ee140 and `SliderHandler`
  0x3f785c): 3/4/5/6/9/10/0xb/0xc/0xd/loop buttons go to `UserInputs::button*EventHandler`; 0xb is
  `UserInputs::toggleKillFX`; 0xe release is `UserInputs::enterMenu` (sets the status bit that makes
  `EventMediator::rtStatusHandlerCore` call `SettingMediator::show`). **CH1/CH2 select the active
  channel only when neither extended (ALT) nor headphone functionality is active**
  (`bob::Player::handleDirectEvent` 0x3ee38c: `inExtendedFunctionality()` and
  `inHeadphoneFunctionality()` both must be 0). Mix mode uses `Mix*Functionality` state classes on
  the RT side; the UI key code has no mix-specific branch [I].
- Settings menus (`SettingMediator::IList_keyDown/Up` 0x39558c/0x395c28) and filter screens
  (`FilterMediator::IList_keyDown/Up` 0x3827b0/0x3838a4) forward unhandled keys to an inner
  `IRawInput`. Boot-time combos (debug menu, diag) are `PwrApplication` constructor checks, see
  ssd-swap.md section 3. DFU is bootloader level.

### 1.3 Free key or chord

- Plain single keys are all taken. FX(0xb) is ignored by the UI but RT toggles kill-FX
  (`toggleKillFX`, audible); FW/BW are fast-forward/back on the RT side; POWER is shutdown/menu.
- **HEADPHONE_MOD held + CHANNEL1 or CHANNEL2 is unused everywhere** [V]: RT returns early in
  `handleDirectEvent` while headphone functionality is active; the browser forwards DB 1/2 to
  `EventMediator::IRawInput_keyDown`, which ignores them unless ALT is held. Held headphone state is
  readable in two places: `PmTrackList+0x13f8` (== 1) and the juce modifier block at 0xad1c68+0x18.
  Whether the chord is comfortable on the physical device is not something code can tell [H].
  ALT + CH1/CH2 is taken (time-left toggle), and ALT in the browser opens filters.
- Unused chords: HP + CH1, HP + CH2. Suggested mapping: HP+CH1 = favourite the highlighted track.
  HP+CH2 could be reserved for "favourite the track loaded on the selected deck" (section 2.4).
  `FX` + anything is a bad idea because RT also acts on FX.

## 2. The existing "Add to case" code path

### 2.1 Classes [V]

`CaseOperations` (an `ISetting` listener, also an `ITimer` via a second vtable) is constructed in
`PMRunningApp::PMRunningApp` (0x342e90) as `CaseOperations(SettingMediator&, PmTrackList&, const
juce::String& deviceInfo)` at **`PMRunningApp+0x31f4`**; `PmTrackList` is at `PMRunningApp+0x18e0`, so
`CaseOperations* = PmTrackList* + 0x1914`. Fields: +8 `SettingMediator&`, +0xc `PmTrackList&`, +0x10
device-info String, +0x14 `ErrorComponent*`, +0x18 `BannerWindow*`, +0x1c `ImpTimer`.

The context-menu flow: Track list menu -> "Add to case" (language 207) -> "My cases" list ->
`CaseOperations::ISetting_ValueChanged(int)` (0x3485ac): ids below 2000 are case indices
(`SettingMediator::setCurrentCaseIdx` then `addToCurrentCase(action)`), special ids call
`removeFromCase` / `createCase`. `action` 0 = current track, non-zero = all tracks in the list.

All of these are exported with mangled names (`.dynsym` is the full C++ symbol table), so the exact
parameter types are known:

| Function | Address | Mangled |
|---|---|---|
| `CaseOperations::addToCurrentCase(CaseAction)` | 0x348294 | `_ZN14CaseOperations16addToCurrentCaseE10CaseAction` |
| `CaseOperations::actionOnSpecifiedCase(CaseAction, unsigned)` | 0x34857c | `_ZN14CaseOperations21actionOnSpecifiedCaseE10CaseActionj` |
| `CaseOperations::createCase(CaseAction)` | 0x347dd0 | `_ZN14CaseOperations10createCaseE10CaseAction` |
| `CaseOperations::currentCaseId()` | 0x348204 | `_ZN14CaseOperations13currentCaseIdEv` |
| `PmTrackList::getCurrentTrack() const` | 0x3916b4 | `_ZNK11PmTrackList15getCurrentTrackEv` |
| `tl::GetRepo()` | 0x3a6784 | `_ZN2tl7GetRepoEv` |
| `tl::Repo::getCaseIdFromName(RepoLock&, const String&)` | 0x3b6e04 | `_ZN2tl4Repo17getCaseIdFromNameERNS_8RepoLockERKN4juce6StringE` |
| `tl::Repo::getCaseList(const TrackFilter*, const CaseMd::FieldId*, unsigned long long, unsigned)` | 0x3b797c | `_ZN2tl4Repo11getCaseListEPKNS_11TrackFilterEPKNS_6CaseMd7FieldIdEyj` |
| `tl::Repo::createCase(const CaseMd&)` | 0x3b308c | `_ZN2tl4Repo10createCaseERKNS_6CaseMdE` |
| `tl::Repo::isTrackInCase(RepoLock&, long long, long long)` | 0x3b8708 | `_ZN2tl4Repo13isTrackInCaseERNS_8RepoLockExx` |
| `tl::Repo::insertCasetracks(RepoWrLock&, DbTrans&, long long, const vector<long long>&)` | 0x3b8abc | `_ZN2tl4Repo16insertCasetracksERNS_10RepoWrLockERNS_7DbTransExRKSt6vectorIxSaIxEE` |
| `tl::Repo::checkWritable() const` | 0x3b2f54 | `_ZNK2tl4Repo13checkWritableEv` |
| `RepoRdLock/RepoWrLock(Repo&)`, `~` | 0x3a6d68 / 0x3a73b0 | `_ZN2tl10RepoRdLockC1ERNS_4RepoE`, `_ZN2tl10RepoWrLockC1ERNS_4RepoE` |
| `DbTrans(Repo&)`, `commit()`, `~DbTrans` | 0x3aac5c / 0x3aa9d8 / 0x3aaa8c | `_ZN2tl7DbTransC1ERNS_4RepoE`, `_ZN2tl7DbTrans6commitEv`, `_ZN2tl7DbTransD1Ev` |
| `juce::String(const char*)`, `String()`, `~String` | 0x6c8744 / 0x6c6b80 / 0x6c7450 | `_ZN4juce6StringC1EPKc`, `...C1Ev`, `...D1Ev` |

Sizes: `RepoRdLock` and `RepoWrLock` are 4 bytes (a `Repo*`), `DbTrans` 8 bytes (DirectAccessRepo*,
two flag bytes), `juce::String` one pointer. Calling convention is ARM EABI: `this` in r0; a 64-bit
argument takes an even register pair or the stack, so `insertCasetracks(r0=repo, r1=&wrlock,
r2=&dbtrans, [sp]=caseId, [sp+8]=&vector)` (seen in the disassembly of `addToCurrentCase` at
0x348320..0x348338); `getCaseIdFromName` and `getCaseList` return a `std::vector<long long>` through a
hidden r0 pointer with `this` in r1 (`getCaseIdFromName(r0=ret, r1=Repo, r2=&lock, r3=&name)`).

### 2.2 What `addToCurrentCase` does (0x348294) [V]

1. `currentCaseId()` = `Repo::getCaseList(null,null,0,0)[SettingMediator::getCurrentCaseIdx()]`
   (8-byte case ids; -1 if the index is out of range).
2. Track: action 0 -> vector of one id from `PmTrackList::getCurrentTrack` (the highlighted row:
   `trackVector[scrollPos]`, vector at `PmTrackList+0x2b8..0x2bc`, 8-byte ids; it does **not** check
   for an empty list); else `getCurrentTracks()`.
3. `RepoWrLock(repo)`, `DbTrans(repo)`, `Repo::insertCasetracks(...)`, `DbTrans::commit()`, then
   the destructors. `insertCasetracks` runs one `INSERT INTO casetracks VALUES (case, track);` per id,
   then `updateCaseDateModified` and `updateCasePlayTimeSecs`. There is **no uniqueness check** and the
   `casetracks` table has no unique index, so repeating the action duplicates rows. A hook must test
   `isTrackInCase` first.
4. Confirmation: `new ErrorComponent(0,0,0)` at `CaseOperations+0x14`, `new BannerWindow(component)`
   at +0x18, `ErrorComponent::unholySetStrings(getString(0xd5) = "Added to case", "", "")`,
   `setTextColor`, `juce::Timer::startTimer(co+0x1c, 700)`, `BannerWindow::setVisible(true)`.

`actionOnSpecifiedCase(action, idx)` is just `SettingMediator::setCurrentCaseIdx(idx)` followed by
`addToCurrentCase(action)` (0x34857c, 48 bytes). `setCurrentCaseIdx` stores the index before it
touches any setting objects and returns early if the settings tree is not built yet [V], so calling it
before the menu was ever opened is safe; the visible side effect is that the chosen case becomes the
"current case" in the menus, which is what picking it from "My cases" does anyway.

### 2.3 Creating a case [V]

`CaseOperations::createCase` (0x347dd0) builds a `tl::CaseMd` on the stack with name = language
string 0x104 ("New Case") + two-digit index (`getLastCaseNameIndex`, format `%.2u`) + string 0x105,
zeroes the numeric fields, sets `creator_id` = `"Device;"` + the device-info string (`co+0x10`),
calls `Repo::createCase` and `SettingMediator::setCurrentCaseIdx(listSize-1)`, then shows the banner
"Created" (string 0xd4). The name is therefore **not** configurable, so the favourite needs its own
`CaseMd`. Layout (from `DirectAccessRepo::createCase` 0x3ad5e8, SQL `INSERT INTO cases VALUES (NULL, ...)`):
0x00 name (String), 0x08 date_created (i64), 0x10 times_played, 0x14 image_id, 0x18 genre (String),
0x1c year, 0x20 rating, 0x24 producer (String), 0x28 remixer (String), 0x2c play_time_secs, 0x30
last_played (i64), 0x38 date_modified (i64), 0x40 comments (String), 0x44 creator_id (String); total
0x48. Each String member must be initialised with `juce::String::String()` (an empty juce string is
not a null pointer). `Repo::createCase` takes the write lock itself (`ReadWriteLock::enterWrite`),
calls `checkWritable()` (throws a C++ exception if `Repo+0x54 != 0`), and clears a cache flag.
Lookup by name: `SELECT case_id FROM cases WHERE name = '<escaped>';` via `getCaseIdFromName`
(needs the caller to hold a `RepoRdLock`; returns all matching ids, names are not unique).

### 2.4 Which track [V]/[I]

- Highlighted track: `PmTrackList::getCurrentTrack()`; valid only if `*(pt+0x2b8) != *(pt+0x2bc)`.
- Track loaded on a deck: `PmTrackList::setCurrentTrack(ch, id, pos)` (0x3913b4, called from
  `EventMediator::rtStatusHandlerCore` with the RT status) stores `id` at `pt + ch*8 + 0x13e8` and the
  list position at `pt + ch*4 + 0x13e0`; the selected channel is `*(uint*)(pt+0x2e4)`. Id 0 means
  nothing loaded [I]. Adding it needs a private insert path (lock, transaction, `insertCasetracks`)
  plus a copy of the banner code, because the stock add uses the highlight.

## 3. Patch mechanics

### 3.1 Hook sites [V]

- **H1, browser:** address 0x394678, 8 bytes `b4 22 95 e5 02 00 a0 e1` (`ldr r2,[r5,#0x2b4]` ;
  `cpy r0,r2`), the start of the default branch of `PmTrackList::IList_keyDown`. At that point
  r5 = `PmTrackList*`, r1 = DeviceButton, r6 = list component, lr = scratch (it holds the row-index
  argument and is overwritten before use), sp is 8-aligned (entry sp minus 56). Only DeviceButtons
  outside 3..10 and 7/8 reach it, so the stock behaviour of every bound key is untouched. Hook =
  replace those 8 bytes with `04 f0 1f e5` (`ldr pc,[pc,#-4]`) + `.word trampoline`; the trampoline
  saves r0-r3,r12,lr (plus 8 bytes of padding to keep 8-byte stack alignment), calls
  `fav_browser(r0 = r5, r1 = db)`, restores, re-executes the two displaced instructions and jumps to
  0x394680.
- **H2, playing view (optional):** address 0x349b34, 8 bytes `00 00 51 e3 1e ff 2f d1` (`cmp r1,#0` ;
  `bxle lr`), entry of `EventMediator::IRawInput_keyDown`. r0 = `EventMediator*` (`+0x1c` = TrackArea,
  `+0x20` = PmTrackList), r1 = DeviceButton. Same trampoline pattern, resume at 0x349b3c. Caveat:
  settings, filter and (via `PmTrackList` default) the browser also end up here, so it needs a gate:
  fire only when `TrackArea+0x91a == 0` (no menu) and the filter screen is not active (flag inside
  `FilterMediator`, at about `PmTrackList+0x115c`, **not verified**, [H]); the browser is covered
  by H1 (`TrackArea+0x91a == 1` there). The held-headphone state must come from the modifier block
  at 0xad1c68+0x18, since `PmTrackList+0x13f8` is only maintained while the list has the key focus.
  Plan H1 first, H2 as a second stage after on-device testing.
- Both sites can be found without hard-coded addresses: `dlsym(RTLD_DEFAULT,
  "_ZN11PmTrackList13IList_keyDownER15PmListComponentjN7pecomms12DeviceButtonE") + 0xe8` and
  `dlsym(... "_ZN13EventMediator17IRawInput_keyDownEN7pecomms12DeviceButtonE") + 0`; verify the
  8 expected bytes before writing and do nothing on mismatch (other firmware versions).

### 3.2 Where the new code can live

- **Slack in the ELF:** none of the usable kind. Program headers: PT_LOAD R+X file 0..0xabe6e4 at
  0x8000, PT_LOAD RW file 0xabe6e4 at 0xace6e4 (filesz 0x2e40, memsz 0xc238), PT_PHDR is directly
  followed by PT_INTERP so there is no room for another header, `.dynamic` has 6 spare slots but
  `.dynstr` has no slack (one byte) for a new DT_NEEDED name. There are zero runs of up to 3.5 KB
  (for example 0x908ffb, 0x8e8598) but they are inside `.rodata` data tables, not padding, so they are
  a gamble. A real cave exists in **dead code**: `pecomms::testRealtimeCommsCore` (0x3e4c68, 6552
  bytes) and `testRealtimeComms` (0x3e6600, 52 bytes) have no callers [I, direct callers only].
  Another static option is turning PT_NOTE (0x128, 32 bytes, not needed by this loader) into a new
  PT_LOAD for an appended segment; the kernel would also move the brk start above it.
  A static blob would have to be hand-linked at fixed addresses with a hand-written ABI, which is
  the most error-prone way to write this routine. Not recommended.
- **Preload library (recommended):** the loader is glibc 2.4 (`lib/libc-2.4.so`, `ld-2.4.so`), whose
  `ld.so` contains `LD_PRELOAD` and the message "object '%s' from %s cannot be preloaded: ignored."
  [V], so a missing or unloadable library degrades to stock behaviour instead of a crash. The player is
  dynamic and the exe exports its whole C++ symbol table (31,454 `.dynsym` entries, all functions above
  are `STB_GLOBAL` and defined in `.text`), so a library can reference `tl::Repo::createCase` etc. by
  mangled name and the dynamic linker resolves them against the executable (main program is first in
  the lookup scope) [I]. Imports of `pmplayer` include `mprotect`, `syscall`, `dlsym`, `dlopen`. The
  image is mounted read-only (`mount -r ... loop`), which does not matter: the patch is a private
  copy-on-write change in memory.
- Install sequence in the library constructor: resolve both sites, check the bytes, `mprotect` the
  page(s) to RWX, write the 8 bytes, `mprotect` back to R+X, flush the instruction cache with
  the ARM `cacheflush` syscall (`swi 0x9f0002`, ARM11 has split caches), `unsetenv("LD_PRELOAD")` so
  child processes are clean. The constructor runs before `main`, so no thread executes the patched
  code yet.
- Wrapper: the existing `--diag` wrapper in `scripts/build-pfw.sh` already shows the mechanism (rename
  `/bin/pmplayer` to `pmplayer.real`, shell script that ends with `exec`, same PID, which is how
  `pwrApplication` tracks it). Add `LD_PRELOAD=/pmds/bin/libfavourite.so` only if the file exists and
  a kill-switch file (for example `/mnt/public/.Pacemaker/favourite.off`) does not.
- Toolchain: none installed here (no ARM gcc, no capstone/keystone). Use the WSL Debian box used for
  `build-pfw.sh` with an ARM EABI cross compiler (package name not verified; check `apt-cache`), ARM mode,
  `-march=armv6 -mfloat-abi=soft` (no floats are involved; the player's float ABI was not determined),
  no stack protector, `-fexceptions`. Link against the device's own `libc-2.4.so`, `libdl-2.4.so`,
  `libstdc++.so.6.0.8` (GLIBCXX up to 3.4.8, from GCC 4.1.1) and `libgcc_s.so.1` from
  `work/rootfs-180096069/lib`, then check the `.so` with readelf that it needs no symbol version newer
  than GLIBC_2.4 / GLIBCXX_3.4.8. Trampolines go in a `.S` file (GCC 4.x-era ARM has no `naked`).

## 4. Risks and fail-safe

- **DB integrity.** Writes use the same sequence as the stock code: `RepoWrLock` (a
  `juce::ReadWriteLock` write lock; it also sets `Repo+0xa8`), `DbTrans` (reference-counted
  `BEGIN`; the destructor issues `ROLLBACK` when `commit()` was not reached [V from `~DbTrans`
  0x3aaa8c]), then `commit`. Any early return or exception leaves nothing half-written. Do not add
  raw SQL or open a second SQLite connection: the `Repo` object caches (`Repo::createCase` clears a
  cache flag at `Repo+0xe8`) and `user_version` must remain 677 (FINDINGS section 3).
- **Locking/threads.** The hook runs on the juce message thread, as the stock menu action does. Release
  the `RepoRdLock` taken for lookups before `createCase` (it takes the write lock itself, and
  `createCase` is also called by stock code with no lock held). The write lock may block while the
  track loader or analyser holds a read lock, same as the menu path. Add a re-entrancy flag so a held
  key cannot recurse (auto-repeat is already filtered, see 1.1).
- **Read-only repo.** `Repo::checkWritable` throws when `Repo+0x54 != 0`; wrap everything in
  `try { ... } catch (...) {}` (the library must be built with exceptions) or test `Repo+0x54` first,
  and never let an exception unwind into the player's C frames. Behaviour while USB mass storage is
  active is not analysed (the state machine in `pwrApplication` owns it and `pmplayer` is not meant to
  run then [I]).
- **Duplicates and bad ids.** Check `isTrackInCase` before inserting. Ignore empty lists (the stock
  `getCurrentTrack` indexes the vector blindly). Negative ids are the placeholders for deleted tracks
  (FINDINGS section 3) and cannot appear in a freshly shown list.
- **Crash path.** A fault inside the hook kills `pmplayer`; `pwrApplication` prints "pmplayer abnormal
  exit" (what it then does was not analysed [H]). Mitigations: dry-run build first (hook only logs),
  exact byte checks, the kill-switch file, library never writes to the DB until the key chord, state
  and symbol checks pass.
- **Other firmware versions.** Hook offsets and the struct offsets used (`PmTrackList` +0x2b8/+0x2bc/
  +0x2e8/+0x13f8, `CaseOperations = PmTrackList+0x1914`) are specific to 180096069; the byte check
  and a firmware-version check (`system.xml` `SWVERSION`) keep it inert elsewhere.
- **Side effect.** Using `actionOnSpecifiedCase` makes "Favourites" the current case of the "Add to
  case" menus. If that is unwanted, save and restore `SettingMediator::getCurrentCaseIdx()` around
  the call.

## 5. Recommended implementation plan

1. Settle the chord on the real device (HP + CH1 proposed). No code needed: confirm that holding the
   headphone modifier and pressing CH1/CH2 does nothing today in the browser and while playing.
2. Write `libfavourite.so` (C++ with the mangled-name declarations from section 2.1, `-fexceptions`):
   constructor = resolve both sites with `dlsym`, byte-check, patch, cache flush, `unsetenv`;
   `fav_browser(PmTrackList*, int db)` = filter (`db` is 1 or 2, `*(int*)(pt+0x13f8) == 1`,
   `*(char*)(pt+0x2e8) != 0`, list non-empty, not re-entrant), then `favourite(pt)`.
3. `favourite(pt)`: `Repo* r = tl::GetRepo()`; `RepoRdLock` -> `getCaseIdFromName(...,
   String("Favourites"))`, take the first id; if none, build a zeroed 0x48-byte `CaseMd` (empty
   Strings, name "Favourites", `creator_id` copied from `"Device;" + *(String*)(co+0x10)`), call
   `r->createCase(md)` outside any lock, destroy the Strings, look the id up again; compute its index in
   `r->getCaseList(0,0,0,0)`; `track = pt->getCurrentTrack()`; if `isTrackInCase(lock, id, track)`
   return (optionally show a different banner); else
   `CaseOperations::actionOnSpecifiedCase(co = (char*)pt + 0x1914, 0, idx)`, which inserts in a
   locked transaction and shows "Added to case".
4. Trampoline `.S` for H1 (stmfd {r0-r3,r12,lr} + 8 bytes pad, mov r0,r5, bl `fav_browser`,
   restore, displaced `ldr r2,[r5,#0x2b4]` / `mov r0,r2`, `ldr pc,=0x394680`).
5. Extend `scripts/build-pfw.sh` (new flag next to `--diag`, a combined wrapper because both rename
   `/bin/pmplayer`) to add `libfavourite.so` and the conditional `LD_PRELOAD`; rebuild, run
   `e2fsck -fn` and the trailer check it already does.
6. Offline checks before any device: `readelf` symbol-version audit; run the constructor against a
   copy of `pmplayer` mapped in `qemu-arm` or with a stub that exports the same symbols to exercise the
   byte check and the patch/rollback path (the player itself cannot run off-device: DirectFB, ALSA);
   re-read the diff.
7. Device test 1 (log only): library built with the DB calls compiled out, writes one line per
   chord to `/mnt/public/.Pacemaker/favourite.log`. Confirms the hook fires only for the chord,
   in the browser, with the playlist highlight, and that stock keys still behave.
8. Device test 2 (live): back up `music.db`, then favourite one track; check the banner, that
   "Favourites" appears in "My cases", that pressing again adds no duplicate, and that the Editor
   (or `scripts/pmdb.py`) reads the case. Test with a list of zero items, with a mix loaded, and with a
   filtered list.
9. Stage 2 (optional): H2 for the playing view and/or HP+CH2 for the track loaded on the selected deck
   (section 2.4), gated as in 3.1.
10. Document how to remove it: restore the stock `.pfw` (the rebuilt image only adds files and a
    wrapper), or create `favourite.off` on the volume.
