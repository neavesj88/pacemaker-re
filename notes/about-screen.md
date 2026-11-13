# About screen and Legal notice in pmplayer (firmware 180096069)

Reverse engineering for firmware 180096074 (built by `scripts/build-180096074.sh`): make every user-visible About / Status / Legal string say
Pacemaker Revival instead of Tonium or the original product. Nothing has been run on hardware; this is the patch
plan and the evidence behind it (the "As built" section at the end records what 180096074 actually applies). Stock binary: `work/fw-180096069/bin/pmplayer`, 11 286 972 bytes,
sha256 `4c5d502c6d52e2bafab894cef43e7213e35aca73f497b68482dd7867c13797ab`. No serial number or library data
appears in this note (the serial is only ever read through `/system.xml` at run time).

## Addresses

The ELF is not position independent and its single text/rodata LOAD segment is at file offset 0 / VA
0x8000, so **Ghidra VA = file offset + 0x8000** everywhere. The brief's "file offset/VA 0x844b90" is the
file offset; the Ghidra address is 0x84cb90.

| string | file offset | Ghidra VA | size |
|---|---|---|---|
| Legal notice (char*, ASCII) | 0x844b90 | 0x84cb90 | 13152 + NUL; the Ghidra item is 13156 because 3 pad bytes follow |
| `www.pacemaker.net` (char*) | 0x847ef4 | 0x84fef4 | 17 + NUL, then padding |
| `Pacemaker Player` (wchar_t*, 4 byte) | 0x83e51c | 0x84651c | 16 chars + NUL = 68 |
| `Tonium;Device;` (wchar_t*) | 0x851280 | 0x859280 | 14 chars + NUL = 60 |
| `P211` (char*) | 0x8407c8 | 0x8487c8 | model default |
| `666` (char*) | 0x84085c | 0x84885c | model for serial prefix "01" |

The 13156 bytes 0x844b90..0x847ef4 are one contiguous block ending exactly where `www.pacemaker.net` starts.

## How the screens are built

About is a Settings menu (id/title/parent table near VA 0x84c9e8, read by `SettingMediator::SettingMediator`, 0x396278):
About (id 2, lang 39) has children Status (3, lang 40), Version (4, lang 46) and Legal notice (0x4b0,
lang 53). Status holds Battery, Available, Used, Mixes, Tracks. Version holds these `StringSetting`s
(created by `PMSettingsAlways::PMSettingsAlways`, 0x338ac0 and its twin 0x33fe7c, and by the
`SettingMediator` constructor), all with parent 4:

| id | title lang id | label | value comes from |
|---|---|---|---|
| 0x44c | 47 | Serial no | `PwrSettings+0x24`, read from system.xml; row shows ">", the pane shows the value |
| 0x44d | 48 | Model | `PwrSettings+0x28` (see below) |
| 0x44e | 50 | Firmware | `PwrSettings+0x2c` = `FWVERSION` of `/mnt/public/.Pacemaker/system.xml` |
| 0x44f | 51 | Software | `PwrSettings+0x30` = `SWVERSION` of the same file |
| 0x450 | 52 | Support | row shows ">"; the pane draws a literal string (below) |
| 0x4b0 | 53 | Legal notice (child of About) | row shows ">"; the pane draws the Legal literal |

Row labels come from `IList_getRowStrings` (0x395ce8): `is::getString(setting->getTitle())`. Values are
pushed by `PMRunningApp::ISetting_ValueRequested` (0x337a00) from `PwrSettings::getSerialNumber / getModel /
getFwVersion / getSwVersion`.

**Model.** `PwrSettings::PwrSettings` (0x3587c8 and twin 0x358878) initialises `+0x28` with `"P211"`. `PwrMgr::PwrMgr`
(0x3590ac and twin 0x35965c) then parses `/mnt/public/.Pacemaker/system.xml` (PRODUCTINFORMATION element) and
sets `+0x28` to `"P211"` if the first two characters of the serial are `00`, or to `"666"` if they are `01`
(otherwise it stays at the default). `getModelName` (0x5cd62c) has the same two literals but nothing calls it
(searched every `bl`/`b` and every pointer word). The hardware checks `getFlashSize` (0x5cd500) and
`isAudioChannelSwapped` (0x5cd560) use `getModel()`, a *substring of the serial* compared with `"00"` and `"0001"`,
never `"P211"`. So the displayed model string is not used for any hardware decision.

The model string is also part of `PwrSettings::getDeviceInfoString` (0x358d00) = `model;sw;fw;serial`. It goes
into `CaseOperations` and `pe::AudioPlayer` and ends up as the second half of `cases.creator_id`
(`"Tonium;Device;"` + that string, `CaseOperations::createCase` 0x347dd0). FINDINGS.md already records that nothing
parses `creator_id`. A new model string only changes that provenance text for newly created cases.

**Support pane.** `SettingMediator::IList_action` (0x39a140) calls `setSettingStringShowState(id)` for ids
0x44c, 0x450 and 0x4b0. `SettingString::paint` (0x39bf40) then chooses by that id:

- 0x450: `juce::String::String(char const*)` of the literal at pool word 0x39c088 (`www.pacemaker.net`),
  drawn with `drawFittedText(x0, y0x47, w0xba, h0x0d, centred, maxLines 1, minHorizontalScale 0.7)`.
- 0x4b0: `String(char const*)` of the literal at pool word 0x39c08c (the Legal text),
  `drawMultiLineText(text, 0, -scroll, 0xba)`.

So the pane is 186 px wide. The one-line URL is squeezed horizontally to at most 0.7 of its width and
ellipsised beyond that, so there is room for about 265 px of unscaled text.

**Legal pane.** `StringWindowShow` sets the scroll position to -0x14. `SettingString::IScrollTarget_increment`
(0x39bf08) clamps the top at -0x10 and has **no clamp at the bottom**, so a short text can be scrolled into an empty
pane (cosmetic, scrolling back returns). `drawMultiLineText` -> `GlyphArrangement::addJustifiedText` (0x600d68)
breaks lines on character 10 or 13 and wraps words at 186 px. The stock text itself relies on that (119 LF, no CR).
There is no length cap: the string is copied whole (`strlen`) and drawn.

## Encoding

`juce::String::String(char const*)` (0x6c8744) is `strlen` + `mbstowcs` into a 4-byte `wchar_t` buffer
(`CharacterFunctions::copy`, 0x6c5e18, is literally a jump to `mbstowcs`). The program never calls
`setlocale` to set a locale (the only call, 0x7f54b8, is the query `setlocale(0, NULL)`), so it runs in the
"C" locale and **bytes >= 0x80 are not decodable**: the Legal text, the model string and the URL must be
**pure ASCII**. (`legal.txt` is: 2909 bytes, 46 LF, no CR, no byte above 0x7e.) The `wchar_t*` strings (`Pacemaker Player`,
`Tonium;Device;`) are 4-byte-per-char and need 4-byte alignment.

Lang files are different: `is::loadLanguageFileInternal` (0x388330) reads lines with `strtol` for the id and
`juce::String::fromUTF8` for the text (1024-byte line buffer), so the lang files are UTF-8 and `(c)` can stay the real
sign. U+00A9 has a glyph in all six shipped fonts (checked in the cmap of Ar_WGL__/Ar_WGL_B/DejaVuSans/DejaVuSans-Bold/UnDotum/
ipagui), and the stock lang files already contain it as `C2 A9`.

## Where each Tonium / pacemaker string is used

| string | users | verdict |
|---|---|---|
| Legal text 0x84cb90 | one pool word, 0x39c08c, read by `SettingString::paint` (0x39bfa8). No other pointer or interior pointer exists (aligned scan of the whole file; the hits at 0x8d2540/0x8d259c/0x8d25a8 are ICU data bytes `65 DD 84 00` that merely look like 0x0084dd65). | rewrite in place, same address |
| `www.pacemaker.net` 0x84fef4 | one pool word, 0x39c088, same function (0x39bf94) | repoint |
| `Pacemaker Player` 0x84651c | pool word 0x337708 in `PMPlayerApplication::getApplicationName` (0x3376f0), a virtual on the JUCE application | not shown anywhere. Its only consumer found is `juce::JUCEApplication::main` (0x5d3818): lock name `"juceAppLock_" + name` and a broadcast message. Leave alone, or optionally patch in place (see plan). |
| `Tonium;Device;` 0x859280 (+ tail `Device;` at 0x85929c) | pool words 0x40b1d4 (`saveCurrentMixDocCore`, 0x40ad24, prefix of the creator/blob name) and 0x3481f4 (`CaseOperations::createCase`, creator of new cases) | **do not change**: it is the creator id written into the user's music database. Not shown in About or Status. |
| `toniumId` (narrow 0x87909c, wide 0x881bc0) | boost XML tag of `GlobalContext` in mix documents (`iserializer/oserializer<..GlobalContext>`, `MixDocument::loadDocumentLocalized`) | **do not change**: it is a serialization key; mixes written by the Editor and the device use it. |
| `com.tonium.peer`, `com.tonium.app_pid` 0x89225c/0x89226c | `juce::LinuxComponentPeer::createWindow/destroyWindow/getPeerFor` (X11 window property names) | **do not change** (and pwrApplication has the same pair). |
| `.Pacemaker` (wide, `E.Pacemaker` at 0x891ae8 is its neighbour) and `/mnt/public/.Pacemaker/system.xml` 0x848838 | data folder name on the FAT volume, used by the Editor, `PwrMgr` and the revival flag files | **do not change**. |
| `/pmds/bin/resources/...` | path prefix of fonts and symbols | **not a brand**, do not change. |
| `P211`, `666` | see Model; 4 + 2 pool words plus 2 dead ones | repoint the 6 live words |
| lang string 49 `(c) Tonium` | no `getString(49)` immediate, no Setting with title 49, no table entry (checked: `FindImm 49`, every `StringSetting` title, a scan of rodata/data for `[x,49,y]` triples) | **dead in this firmware** (no row shows it). Patch it anyway so that nothing in the image says Tonium; it cannot break anything. |

Also found, outside About and not touched by this plan: `Tonium AB` in the `author=` strings of `evrout.ko` and `pmic_input.ko`
(module metadata), `Tonium` as the USB manufacturer string in `boot/redboot.bin`, the DFU name `DFU Gadget`, and a second
`english.lang` in the cramfs at `/usr/local/share/fonts/english.lang` (read by pwrApplication, not by the About screen). Those
are not About-screen text; changing the first two means rebuilding kernel modules or the bootloader and is out of scope.

## Patch plan

All patches are against stock `pmplayer`. Every one asserts the stock bytes first and refuses on mismatch, the file
size stays 11 286 972, and the result is deterministic. File offsets below are file offsets in `pmplayer`; "VA" = file offset + 0x8000.

New block layout inside the 13156-byte region 0x844b90..0x847ef4 (zero everything first, then write):

| what | file offset | VA | bytes |
|---|---|---|---|
| Legal text = `device/about/legal.txt` verbatim + NUL | 0x844b90 | 0x84cb90 | 2909 + 1 (ends 0x8456ef) |
| `Pacemaker Revival` + NUL | 0x8456f0 | 0x84d6f0 | 18 |
| `github.com/neavesj88/pacemaker-re` + NUL | 0x845704 | 0x84d704 | 34 (ends 0x845726) |
| zero fill to 0x847ef4 | 0x845726 | | 10 190 free |

| # | file | file offset | old | new | why safe |
|---|---|---|---|---|---|
| 1 | pmplayer | 0x844b90..0x847ef4 | the 13152-byte stock Legal text (sha256 `d2a84f37...c639211`, starts `Pacemaker Legal Notices:`) + 4 NULs | zeros, then the block layout above | The only reference is pool word 0x39c08c = 0x84cb90, which stays valid because the new text starts at the same address. 2910 bytes < 13156. ASCII only, as the C-locale `mbstowcs` needs. Lines are LF-separated and wrapped at 186 px by the existing code. |
| 2 | pmplayer | 0x394088 (VA 0x39c088) | `f4 fe 84 00` (0x84fef4) | `04 d7 84 00` (0x84d704) | Single user (`SettingString::paint`), consumed as `char*` -> `mbstowcs`, 33 chars fit the 186 px / 0.7 squeeze budget. |
| 3 | pmplayer | 0x847ef4..0x847f05 | `www.pacemaker.net` + NUL (18 bytes) | 18 zero bytes | Optional hygiene; only after #2, since #2 removes its single reference. The following padding and the next item at 0x847f0c are untouched. |
| 4 | pmplayer | 0x350874 and 0x350924 (VA 0x358874, 0x358924) | `c8 87 84 00` (0x8487c8, `P211`) | `f0 d6 84 00` (0x84d6f0) | `PwrSettings` constructors, default model. `char*` -> `juce::String(const char*)`, copied by value. |
| 5 | pmplayer | 0x351650 and 0x351c00 (VA 0x359650, 0x359c00) | `c8 87 84 00` | `f0 d6 84 00` | `PwrMgr` constructors, serial prefix `00`. |
| 6 | pmplayer | 0x35164c and 0x351bfc (VA 0x35964c, 0x359bfc) | `5c 88 84 00` (0x84885c, `666`) | `f0 d6 84 00` | `PwrMgr` constructors, serial prefix `01`. Without it a unit with that prefix would still show `666`. |
| 7 | english.lang, japanese.lang, korean.lang (`/bin/resources/fonts/`) | string 49 | `49 "\xc2\xa9 Tonium"` | `49 "\xc2\xa9 Revival contributors"` | Text format, no offsets or checksums; UTF-8 via `fromUTF8`; the glyph exists in every font. The english and japanese files to start from are the 073 ones (`device/vocalfx/lang/*.lang`, stock except string 236 `Vocal`); korean is stock. Assert exactly one `49 "\xc2\xa9 Tonium"` per file and nothing else changed. |
| 8 | image | new file `/bin/resources/licenses/THIRD-PARTY-NOTICES.txt` | | the 13152 stock Legal bytes, extracted byte-exactly from the stock pmplayer before #1 (no trailing LF added) | Pure data. legal.txt promises it. |
| 9 | image | new files `/bin/resources/licenses/GPL-2`, `LGPL-2.1` | | `/usr/share/common-licenses/GPL-2`, `LGPL-2.1` (WSL Debian) | Same. |
| 10 | image | `/bin/pmplayer.real` | stock | the patched pmplayer | See build notes. |

The same bytes with the Legal block as in #1 give: Legal notice = the funny notice, Support pane =
`github.com/neavesj88/pacemaker-re`, Model row = `Pacemaker Revival`. Firmware, Software, Version and Serial
rows are untouched code paths (values still come from system.xml).

Optional, not needed for the goal: `Pacemaker Player` (0x83e51c) could be overwritten in place with the 4-byte-per-char
string `Revival Player` (14 chars + NUL fits the 68 bytes; `Pacemaker Revival` does not, it needs 72). The only effect is the
single-instance lock file name. Recommendation: leave it.

Build notes for `scripts/build-pfw.sh`:

- `--diag` renames `/bin/pmplayer` to `pmplayer.real` *after* all `/path=file` replacements have run, so passing
  `/bin/pmplayer=<patched pmplayer>` as a replacement before the wrapper step makes the patched binary the `.real`
  one. The wrapper itself stays the committed one. `replace` keeps the owner and mode.
- `+/bin/resources/licenses/...` needs the directory to exist; **it does not exist in the stock image**
  (`/bin/resources` only has `fonts` and `symbols`), and `add()` refuses a missing parent. build-pfw.sh needs a
  small `mkdir` feature (debugfs `mkdir`, owner and mode copied from `/bin/resources`) before the licence files go in.
- Image free space was not checked here (adds about 13 KB + 18 KB + 26 KB); `e2fsck -fn` at the end of build-pfw.sh will catch a full image.
- The rootfs copy of english.lang (cramfs) is not read by the About screen; leave it.

## What must NOT be changed

- `Tonium;Device;` (wide, 0x859280) and its tail at 0x85929c: it is the creator id in the user's `music.db`.
- `toniumId`, `com.tonium.peer`, `com.tonium.app_pid`, `.Pacemaker`, `/mnt/public/.Pacemaker/system.xml`:
  serialization keys, X11 property names and the on-disk data folder.
- The `getModelName` literals (`P211` at 0x8487c8 and `666` at 0x84885c with their pool words 0x5cd6e0 / 0x5cd6e8):
  keep the strings in place (repoint only the six live pool words); do not overwrite `P211` in place with a longer string,
  it has 8 bytes (4 chars + NUL + padding) and the next item starts right after.
- Do not put any byte >= 0x80 into the Legal text, URL or model string.
- The serial-derived hardware checks (`"00"`, `"0001"`) and the order of fields in `getDeviceInfoString`.
- The pool word 0x39c08c (Legal pointer) stays 0x84cb90, so the new text must start at 0x844b90.

## Unverified

Nothing was run on hardware. Not verified: that `Pacemaker Revival` (17 chars) fits the value column of the Model row without
clipping (the list is 186 px wide, row layout in `PmListComponent` was not decompiled; the longest stock value is the 9-digit
firmware number); how the 33-char URL looks after the 0.7 horizontal squeeze; how the empty area below a short Legal text
looks when scrolled to the end. The patch plan was dry-run on a copy of the stock binary in Python (pointer-following read
back: Legal text equals `legal.txt`, URL and all six model words resolve to the new strings, `getModelName` literals untouched,
size unchanged), not through Ghidra again.

## How this was found

Ghidra 12.1.4 headless on a scratch copy of `work/ghidra/fw-180096069-pmplayer` (read-only, `-noanalysis`), plus Python
scans of the raw ELF (pool words, `bl` targets, string tables). New helper scripts in `scripts/ghidra/`:
`XrefAddr.java` (references and absolute pool words for given VAs), `CallsWithArg.java` (call sites with a constant argument),
`FindImm.java` (instructions with a given immediate). Decompiled with `DecompileMatching.java`, disassembled with `DisasmAt.java`.
Because `analyzeHeadless.bat` splits commas into separate arguments, scripts that take lists read all arguments.

## As built (firmware 180096074)

`scripts/build-180096074.sh` = the 073 recipe with FW=180096074 plus the rows below. The patches are applied by
`scripts/patch-about.py` (stock sha256 + size checked, then the Legal hash, the 4 NULs, the old URL, the model
literals and all eight pool words are asserted before a byte is written; any mismatch refuses). Output is 11 286 972
bytes, sha256 `f0fb0f36529734afa337ec5bc8b6d570e9a92f60709132cb8a8aeb196cac8d50`, 13 034 bytes differ from stock
(the freed block 0x844b90..0x847f06 and 8 pool words).

| # | where | what | applied |
|---|---|---|---|
| 1 | pmplayer 0x844b90..0x847f06 | zeroed, then `device/about/legal.txt` + NUL (2910 bytes) at 0x844b90, `Pacemaker Revival\0` at 0x8456f0, `github.com/neavesj88/pacemaker-re\0` at 0x845704 | yes |
| 2 | pmplayer 0x394088 | `f4 fe 84 00` -> `04 d7 84 00` (Support pane URL) | yes |
| 3 | pmplayer 0x847ef4 | `www.pacemaker.net\0` zeroed (covered by the block zeroing in #1; the test asserts the string is gone) | yes |
| 4 | pmplayer 0x350874, 0x350924 | `c8 87 84 00` -> `f0 d6 84 00` (PwrSettings default model) | yes |
| 5 | pmplayer 0x351650, 0x351c00 | `c8 87 84 00` -> `f0 d6 84 00` (PwrMgr, serial prefix 00) | yes |
| 6 | pmplayer 0x35164c, 0x351bfc | `5c 88 84 00` -> `f0 d6 84 00` (PwrMgr, serial prefix 01) | yes |
| 7 | `device/about/lang/{english,japanese,korean}.lang` | string 49 `(c) Tonium` -> `(c) Revival contributors`; english/japanese keep 236 `Vocal`; the recipe re-derives each from the image's stock file and asserts equality | yes |
| 8 | image `/bin/resources/licenses/THIRD-PARTY-NOTICES.txt` | the 13152 stock Legal bytes (sha256 `d2a84f37...c639211`), mode 644 | yes |
| 9 | image `/bin/resources/licenses/GPL-2`, `LGPL-2.1` | copied from WSL Debian `/usr/share/common-licenses`, mode 644 | yes |
| 10 | image `/bin/pmplayer.real` | the patched player (`/bin/pmplayer=...` is replaced before `--diag` renames it); the wrapper is the committed one | yes |

Other build changes: `scripts/build-pfw.sh` gained `mkdir:/path` (directory, mode 755, owner of the parent) and
`=/path=file` (new data file, mode 644). The recipe finds the gitignored `work/` and `vendor/` by walking up from the
checkout (override `DATA=`), so it runs from a worktree without copying firmware into the repo. The licence files and
the stock Legal text are generated into `work/build/074/licenses/` at build time, not committed (they are derived from
the vendor image / the OS licence files).

Tests: `PM_WORK=<dir with fw-180096069> python3 scripts/test-patch-about.py` (14 tests, runs patch-about.py on the
stock binary and checks: size, determinism, every string reads back through its pointer, only the planned ranges
changed, strings that must survive are intact, refuses an already-patched or modified input, non-ASCII/NUL legal text,
lang files equal stock + intended edits only). Image checks done on the built .pfw: e2fsck clean, `/bin/pmplayer.real`
byte-identical to the patched output, the wrapper identical to 073's, notices/GPL-2/LGPL-2.1 byte-identical to their
sources, and a file-level diff against the 073 image shows only pmplayer.real, the three lang files, system.xml (x2),
cramfs.little and the new licenses directory.
