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
are not About-screen text. This was the state when 074 first built; the "Tonium sweep" section at the end takes all of them
(except the bootloader string) out of the same 074 image.

## Patch plan

All patches are against stock `pmplayer`. Every one asserts the stock bytes first and refuses on mismatch, the file
size stays 11 286 972, and the result is deterministic. File offsets below are file offsets in `pmplayer`; "VA" = file offset + 0x8000.

New block layout inside the 13156-byte region 0x844b90..0x847ef4 (zero everything first, then write):

| what | file offset | VA | bytes |
|---|---|---|---|
| Legal text = `device/about/legal.txt` verbatim + NUL | 0x844b90 | 0x84cb90 | 2909 + 1 (variable length; the model and URL slots sit at the end of the freed block) |
| `Pacemaker Revival` + NUL | 0x847e00 | 0x84fe00 | 18 |
| `github.com/neavesj88/pacemaker-re` + NUL | 0x847e20 | 0x84fe20 | 34 (ends 0x847e42) |
| zero fill to 0x847ef4 | 0x845726 | | 10 190 free |

| # | file | file offset | old | new | why safe |
|---|---|---|---|---|---|
| 1 | pmplayer | 0x844b90..0x847ef4 | the 13152-byte stock Legal text (sha256 `d2a84f37...c639211`, starts `Pacemaker Legal Notices:`) + 4 NULs | zeros, then the block layout above | The only reference is pool word 0x39c08c = 0x84cb90, which stays valid because the new text starts at the same address. 2910 bytes < 13156. ASCII only, as the C-locale `mbstowcs` needs. Lines are LF-separated and wrapped at 186 px by the existing code. |
| 2 | pmplayer | 0x394088 (VA 0x39c088) | `f4 fe 84 00` (0x84fef4) | `20 fe 84 00` (0x84fe20) | Single user (`SettingString::paint`), consumed as `char*` -> `mbstowcs`, 33 chars fit the 186 px / 0.7 squeeze budget. |
| 3 | pmplayer | 0x847ef4..0x847f05 | `www.pacemaker.net` + NUL (18 bytes) | 18 zero bytes | Optional hygiene; only after #2, since #2 removes its single reference. The following padding and the next item at 0x847f0c are untouched. |
| 4 | pmplayer | 0x350874 and 0x350924 (VA 0x358874, 0x358924) | `c8 87 84 00` (0x8487c8, `P211`) | `00 fe 84 00` (0x84fe00) | `PwrSettings` constructors, default model. `char*` -> `juce::String(const char*)`, copied by value. |
| 5 | pmplayer | 0x351650 and 0x351c00 (VA 0x359650, 0x359c00) | `c8 87 84 00` | `00 fe 84 00` | `PwrMgr` constructors, serial prefix `00`. |
| 6 | pmplayer | 0x35164c and 0x351bfc (VA 0x35964c, 0x359bfc) | `5c 88 84 00` (0x84885c, `666`) | `00 fe 84 00` | `PwrMgr` constructors, serial prefix `01`. Without it a unit with that prefix would still show `666`. |
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
- The rootfs copy of english.lang (cramfs) is not read by the About screen; the first 074 build left it alone, the Tonium sweep changes its string 49.

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
| 1 | pmplayer 0x844b90..0x847f06 | zeroed, then `device/about/legal.txt` + NUL (2910 bytes) at 0x844b90, `Pacemaker Revival\0` at 0x847e00, `github.com/neavesj88/pacemaker-re\0` at 0x847e20 | yes |
| 2 | pmplayer 0x394088 | `f4 fe 84 00` -> `20 fe 84 00` (Support pane URL) | yes |
| 3 | pmplayer 0x847ef4 | `www.pacemaker.net\0` zeroed (covered by the block zeroing in #1; the test asserts the string is gone) | yes |
| 4 | pmplayer 0x350874, 0x350924 | `c8 87 84 00` -> `00 fe 84 00` (PwrSettings default model) | yes |
| 5 | pmplayer 0x351650, 0x351c00 | `c8 87 84 00` -> `00 fe 84 00` (PwrMgr, serial prefix 00) | yes |
| 6 | pmplayer 0x35164c, 0x351bfc | `5c 88 84 00` -> `00 fe 84 00` (PwrMgr, serial prefix 01) | yes |
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

If the Model value clips on the device, shorten `MODEL` in `scripts/patch-about.py` (the slot holds about 18 bytes), e.g. to `Revival`.

## Image file modes

`build-pfw.sh replace()` used to write `mode 0100$m` with `m=0755` (debugfs prints the permission bits with a leading 0), which
came out as 0777, so every replaced file was world-writable (073 had this for english/japanese.lang and the kernel modules too).
It now builds the mode as `0100000 | 8#$m`, so replaced files keep the mode of the file they replace. Checked in the built
074 image: pmplayer and pmplayer.real 0755, the three lang files 0644, zImage 0755, cramfs.little 0644.

## Tonium sweep (firmware 180096074, same version number, not installed yet)

Request: replace every remaining "Tonium" in the firmware with Pacemaker Revival wording, except the functional
identifiers and the bootloader. Three independent read-only sweeps (Ghidra on pwrApplication and pmplayer, a raw scan
of the unpacked image, the extracted cramfs and the decompressed kernel, narrow / UTF-16 / UTF-32, case-insensitive,
file names too) found the same set; `scripts/sweep-tonium.py` repeats that scan on any unpacked tree and fails on
anything outside its keep list. It also unpacks every gzip member it finds inside a file, recursively (the kernel payload, and the gzip'd build
config inside the kernel; added after the review found the nested one). On the stock 180096069 tree it reports 23
unexpected hits (rows 1 to 7 below, row 1b being the nested one) and 10 kept ones; on the built 074 image it reports 0
unexpected and 11 kept. Nothing below has run on hardware.

| # | where (stock offsets) | text | who sees it | decision | patch |
|---|---|---|---|---|---|
| 1 | kernel (decompressed 0x24df14, gzip payload of boot/zImage) | `Tonium Pacemaker (MX31)` = `machine_desc.name` of MX31ADS | dmesg `Machine:` line and `Hardware` in /proc/cpuinfo only; nothing parses it | patched | `scripts/patch-kernel-name.py` -> `Pacemaker Revival` |
| 1b | kernel build config, a gzip stream inside the decompressed kernel (`IKCFG_ST` ... `IKCFG_ED`, served as /proc/config.gz) | `CONFIG_LOGO_TONIUM_CLUT224=y` (Kconfig symbol name only) | `zcat /proc/config.gz` from a shell only | patched | same script -> `CONFIG_LOGO_CUSTOM_CLUT224=y`, recompressed to exactly the old length |
| 2 | rootfs `evrout.ko` .modinfo | `author=Tonium AB.` | `modinfo` from a shell only | patched | `scripts/patch-modinfo.py` -> `author=Revival contributors` |
| 3 | rootfs `pmic_input.ko` .modinfo | `author=Tonium AB` | same | patched | same script, same wording |
| 4 | rootfs cramfs `/usr/local/share/fonts/english.lang` | `49 "(c) Tonium"` | nobody: pwrApplication never fetches string 49 | patched | `patch-about.py lang` -> `49 "(c) Revival contributors"`, passed to build-rootfs.sh; first asserted byte-identical to the image's stock english.lang |
| 5 | image `/bin/libcontainer_{aiffc,flac,id3,mp3,mpeg4,ogg,snd,wav}.so`, UTF-32LE | `<FMT> container plugin by Tonium AB` (8) | nobody (dead data, see below) | patched | `scripts/patch-plugin-strings.py` -> `<FMT> container plugin (Revival)` |
| 6 | same 8 libraries | `(C) 2007, Tonium AB. All rights reserved - all wrongs reversed` (8) | nobody | patched, see the attribution note | -> `Pacemaker Revival - all wrongs reversed` |
| 7 | image `/bin/libdecoder_{aac,au,flac,mp3,pcm,vorbis}.so`, UTF-32LE | `<FMT> decoder plugin by Tonium` (6) | nobody | patched | -> `<FMT> decoder plugin (Revival)` (same length) |
| 8 | image `boot/redboot.bin` 0x1ef3c | `Tonium` + pad = USB iManufacturer of the DFU gadget | Windows USB descriptor viewers in DFU mode only (not dfu-util; this PC's Device Manager shows no manufacturer for it) | **left byte-identical** | none (see below) |
| 9 | `pwrApplication` 0x1b889c / 0x1b88ac, `pmplayer` 0x88a260 area | `com.tonium.peer`, `com.tonium.app_pid` | nobody; DirectFB window property keys (not X11), read back by the same process | kept: functional | none |
| 10 | `pmplayer` | `toniumId` (boost tag, narrow + UTF-32), `Tonium;Device;` (UTF-32 creator id in music.db), C++ symbols `setToniumId` / `getToniumId` in the hash tables | nobody on screen; mixes, music.db and the Editor use them | kept: functional | none |
| 11 | image `/bin/resources/licenses/THIRD-PARTY-NOTICES.txt` | `Tonium AB` x3 inside the verbatim original legal text and GPL source offer | nobody on the device; part of the image | kept verbatim: it is the original legal text that Legal notice points to | none |
| 12 | USB mass-storage descriptors (g_file_storage.ko etc.) | no Tonium text at all (`Linux File-Stor Gadget`, VID 1D5E) | Windows | nothing to do; VID/PID untouched | none |

Nothing else exists: no other .ko, no other binary, no Tonium in /etc, no file name, ext3 label or .pfw trailer text. The
compressed `boot/zImage` and `boot/cramfs.little` are swept in their decompressed form (rows 1 to 4). The four-dot P logo
bitmaps in the rootfs are a mark, not text, and FINDINGS 5c records them as unused; they are not touched.

Not covered, stated once: the kernel was built with `CONFIG_KALLSYMS=y`, so its symbol table very probably holds the
identifier of the compiled-in logo (`logo_tonium_clut224` in the Tonium logo patch). kallsyms names are stored as
token sequences, so no literal "tonium" exists in the image and the sweep cannot see it. A quick attempt to locate and
decode the kallsyms token table in the decompressed kernel found nothing, so whether it is there is unverified. It could
only show in /proc/kallsyms, and renaming it is not an in-place text edit (token sequences change length, the name
block is length-prefixed and indexed by the markers table), so it is left. The rows above cover everything that exists as text.

### Row 1: kernel machine name

`MACHINE_START(MX31ADS, ...)` is a `struct machine_desc` at decompressed offset 0x1516c: `nr` 0x1bf at +0, `phys_io`
0x43f00000, `io_pg_offst` 0x3500, `name` pointer at +12 (0x15178) = 0xC0255F14, `boot_params` 0x80000100. The name string
is 24 bytes with `pm_keypad` right behind it, and that pointer is the only word that references it. RedBoot passes the
machine type number (0x1bf) in r1 and the kernel picks the board by number, never by name; the name is only printed
(`Machine: %s`, `Hardware : %s`). No userland reads it: pmplayer, pwrApplication, eventSequencer and pmptest contain
`/proc/cpuinfo` only through their static libc and none contains `Hardware`.

Patch: the 24-byte slot gets `Pacemaker Revival` + NUL fill (17 of the 23 usable characters). The type number and the
pointer are asserted unchanged, so the board still matches; no other kernel byte changes. `Pacemaker Revival (MX31)`
(24 chars) would not fit and needs the pointer moved, which was not worth it for a dmesg line. The script starts from
the kernel 073 proved on hardware (`work/build/073/zImage-revival`, sha256 `547d8e1b...ac505`) and takes the gzip
header and the compressed span from the stock zImage (sha256 `4d7ad831...a1df3`), recompresses with the same chooser
as the logo step (zlib -9 or 7-Zip, whichever is smaller; zlib alone is about 1 435 900 bytes against a span of
1 429 194 and does not fit, 7-Zip gives 1 370 584), zero-fills to the end of the span and keeps the CRC32/ISIZE trailer.
Result (with row 1b below): 1 442 000 bytes (same as stock), payload 1 370 575 of 1 429 194 (58 619 spare), 23 kernel
bytes differ from the 073 kernel in the name slot and 7 301 inside the build config stream, nowhere else; sha256
`c66f221863a0d27ae3abff9cf00e984beeb101f96c74dd9a0ab3173b8964748b`.
`--verify` re-checks that without 7-Zip and without recompressing anything (it compares structurally, so it does not
depend on the zlib build; the 074 build script calls it, because WSL has no 7-Zip); the recompression
itself runs on Windows:

    tools/venv/Scripts/python scripts/patch-kernel-name.py work/fw-180096069/boot/zImage work/build/073/zImage-revival work/build/074/zImage-revival-name

No Pillow is needed for it (`patch-kernel-logo.py` now imports it only inside its own main()).

### Row 1b: build config inside the kernel

The review of the first sweep found one more: the kernel embeds its build configuration (IKCONFIG, `/proc/config.gz`) as a
gzip member between the markers `IKCFG_ST` and `IKCFG_ED` (decompressed kernel offsets 2 363 580 to 2 370 927, 7 347
bytes). It holds `CONFIG_LOGO_TONIUM_CLUT224=y` once, which a scan of the decompressed kernel misses because the stream is
compressed a second time. `patch-kernel-name.py` rewrites that line to `CONFIG_LOGO_CUSTOM_CLUT224=y` (same length),
recompresses the text with zlib -9 and fills the old 7 347 bytes exactly, using a gzip FCOMMENT field of filler dots for
the difference, so the markers, the span and every other kernel byte stay put and `zcat` reads one clean member (checked
with `gzip -t` and `zcat | diff`: one changed line). Only `/proc/config.gz` reads those bytes. It is a name in a Kconfig
listing, not a functional setting: the symbol no longer matches the Tonium logo patch's Kconfig name, which does not
matter because nothing re-reads the config. The patch refuses a config that is not one gzip member filling the span, that
has flags set, or whose Tonium line is not exactly the stock one.

The kernel is flashed to NOR (mtd2/3) by the update, so a bad stream would not boot; DFU recovery exists. Mitigation: the
decompressed result is asserted equal to the 073 kernel plus the name, every byte outside the payload span is asserted
identical to stock, and the gzip CRC/length are checked. The boot itself needs a hardware test like 073 had.

### Rows 2 and 3: module author tags

`.modinfo` is a list of NUL-separated strings. The 2.6.19 loader (and module-init-tools) walks it for `vermagic`
(modprobe also for `depends` and `alias`); `author` is never matched. Re-packing the strings tightly gives 176 bytes
(evrout) and 133 bytes (pmic_input) against sections of 184 and 140 bytes, so `author=Revival contributors` (27 bytes with
its NUL) does not fit in either. `patch-modinfo.py` therefore moves `.modinfo` to the end of the file (4-byte aligned),
changes only that section's `sh_offset` and `sh_size` in the section header, and zeroes the old bytes so no Tonium is left.
A relocatable .ko has no program headers and no relocation refers to `.modinfo`. Correction to the first write-up, which
said it had no symbols: `.symtab` holds local OBJECT symbols in that section (`__mod_author45`, `__mod_description46`,
`__mod_license47`, `__mod_vermagic5`, `__module_depends`, and `__mod_alias51` in evrout; `st_value` = entry offset,
`st_size` = length + 1) plus a SECTION symbol. After the repack they would have been stale (right names, wrong offsets).
The loader never looks at them (the section has SHF_ALLOC cleared, see below), so it was harmless, but the patch now
rewrites their `st_value` and `st_size` to the new entry positions as well (`readelf -s` and the tests check that each
symbol describes exactly one entry of the new blob). Asserted: only the .modinfo header entry and those symbol fields
differ, all other bytes identical, alignment and bounds checked. evrout.ko 13 256 -> 13 444 bytes, pmic_input.ko 5 208 -> 5 352 bytes. If the new author did fit, the script
writes it in place instead (tested with a short author).

Checked on the built rootfs: readelf parses both files (21 sections, `.modinfo` at 0x33c8 and 0x1458) and the real
module-init-tools `modinfo` from the device rootfs, run under qemu-arm-static in a chroot of the extracted cramfs, prints
`author: Revival contributors` plus unchanged description, license, `vermagic: 2.6.19.2 preempt mod_unload ARMv6`,
`depends` and `alias`.

Whether the 2.6.19.2 kernel accepts the moved section: upstream `kernel/module.c` of v2.6.19 was read this time
(github.com/torvalds/linux, tag v2.6.19). `load_module` only requires `sh_offset + sh_size <= len` for every section
(`goto truncated` otherwise), sets `sh_addr = hdr + sh_offset` for all of them, clears SHF_ALLOC on `.modinfo` (so it is
never copied into module memory and its position is irrelevant), and `get_modinfo` walks `sh_addr` for `sh_size` bytes
through `next_string`, which skips any run of NULs (single NUL separators included). `simplify_symbols` only adds
`sh_addr` to symbols, no range check. Nothing orders sections by file offset, and the file simply grew at the end. The
GPL drop has no kernel source, so Tonium's own patches to module.c cannot be ruled out, and nothing has been loaded on
the device or in an emulator: **first boot check: `lsmod` (or dmesg) must list evrout and pmic_input**. A module that
failed to load would cost the event router or the PMIC input driver (no buttons); DFU recovery exists. The no-move
fallback is a short author that fits in place: `patch-modinfo.py IN OUT "Revival crew"` (verified to build).

### Row 4: cramfs english.lang

`scripts/patch-about.py lang` on the stock cramfs file; the build script first asserts that file is byte-identical to the
image's stock `/bin/resources/fonts/english.lang`. Only string 49 changes (UTF-8, `fromUTF8`, same as the image copies).
build-rootfs.sh rebuilds the cramfs and re-verifies metadata and that exactly the intended files differ.

### Rows 5 to 7: audio plugin libraries

14 libraries, 22 UTF-32LE strings (the sweep's count matches a direct search: 8 x 2 + 6). They are filled into a plugin
description by `<Fmt>Container::info` / `<Fmt>Decoder::info`; in pmplayer the only reader of a container description
(`AudioPluginManager::getContainerPluginDescs`) has no caller, `getContainerPlugin` calls `info(NULL)` and the decoder
path calls vtable slot +0xc, never +8. By static analysis nobody ever shows or logs them (not proven by execution).
The strings are packed back to back, so nothing may grow: each is overwritten in place with the new text and zero fill
(the terminating NUL cell stays). Per library: stock sha256 asserted, every string asserted at its exact offset, output
differs from stock only inside those strings (container libs 70 bytes, decoder libs 9 bytes), no Tonium left. A scan of
each library for absolute words pointing into the strings found pointers to the string starts (relocation and GOT words)
plus one aligned word that may be an interior pointer (libdecoder_flac, to the tail ` plugin by Tonium`; not
checked whether it is one). It would stay valid anyway, because the decoder replacement keeps the length and the tail text.

Attribution note, stated once: row 6 is the copyright line of Tonium AB for the plugin code they wrote, and rows 5 and 7
name them as the author. Following the request they are replaced, but the replacement for row 6 carries no `(C)` and no
"all rights reserved", so it is a name tag and not a claim of copyright over Tonium's code. Row 11 (the verbatim legal
notices that Legal notice points to, which also hold the GPL source offer) is kept as it is for the same reason; if the
owner wants those three mentions edited too, say so.

### Row 8: boot/redboot.bin stays byte-identical

`boot/redboot.bin` (sha256 `636cea0b...d40db`, 144 116 bytes) holds `Tonium` at 0x1ef3c (8-byte slot, usb_string table
entry at 0x227b4 -> 0x87f1ef3c). It is only the DFU gadget's iManufacturer string. Why it is not patched:

- A `.pfw` flashes the bootloader only if its trailer `bootloaderver` differs from the number stored at byte 4 of
  /dev/mtd/1 (decompiled `SWUpgrade::isBootLoaderVersionEqual` 0x6eea0 and `startFwUpgrade` 0x70630 in pwrApplication).
  The content of redboot.bin is never compared. Every Revival image keeps 12838, so a changed redboot.bin inside it would
  be silently ignored. To make it flash, `bootloaderver` would have to rise (any value above 12837 is accepted) and the two
  12838 constants inside RedBoot (0x7488, 0x7904) would have to match, or the unit would reflash it on every later update.
- The write path (`mtdWrite` 0x6f0dc: flash_eraseall /dev/mtd/0, write, read back and compare) has no backup, no
  rollback and no A/B for mtd0. A power cut after the erase leaves no RedBoot, and RedBoot is the DFU recovery. The path
  has never run on this device line (all seven shipped images carry 12838).
- The benefit is one string in a USB descriptor that this PC's Device Manager does not even show.

The build script asserts that the image's `/boot/redboot.bin` has the stock sha256 and that the trailer says
`bootloaderver=12838`, so a later change cannot flash the bootloader by accident. The owner decides separately; if wanted,
treat it as its own opt-in image with a proven hardware recovery path first. `Revival` (7 characters) is what fits the
slot without moving the pointer.

### Build, tests, result

`scripts/build-180096074.sh` now also: verifies the kernel (`patch-kernel-name.py --verify`), builds `evrout.ko`,
`pmic_input.ko`, the 14 plugin libraries and the cramfs english.lang, passes the first three to build-rootfs.sh
(`/lib/modules/.../evrout.ko=`, `.../pmic_input.ko=`, `/usr/local/share/fonts/english.lang=`) and the libraries to
build-pfw.sh (`/bin/<lib>=`), and after the build extracts the image and the cramfs and checks: redboot.bin stock,
`/boot/zImage` equals the verified kernel, trailer `bootloaderver=12838`, `fwver=180096074`, md5 OK, then
`sweep-tonium.py` over the image tree (which now unpacks the kernel and its nested build config by itself) and the rootfs
(11 kept, 0 unexpected).

Tests: `PM_WORK=<work dir> python scripts/test-tonium-sweep.py` (22 tests: name slot, type number and pointer; only the
name and the build config's logo line differ from the 073 kernel; the config is one gzip member of the old length with one
line changed, refusals for patched, trailing, truncated or duplicated-marker configs; the sweep unpacks nested gzip;
modinfo symbols follow their entries in both variants; refusal of patched or wrong zImages; the built zImage re-verified and independently
gunzipped; modinfo of both modules read back the way the loader walks it, in-place and moved variants, refusals, no other
module mentions Tonium; all 14 libraries and 22 strings, wording, coverage of every stock Tonium string, refusals; cramfs
english.lang equals stock plus string 49 only; build script wiring and no redboot replacement; sweep tokenizer and keep
list) and `scripts/test-patch-about.py` (14 tests, unchanged), all passing.

Built image `work/build/pacemaker-revival-180096074.pfw`, 51 200 512 bytes (the trailer uid is random per build, so the
hash changes with every rebuild unless the content is unchanged): sha256 `16c819128a693c81a083610c76b304430370d74cc8ea51f0701e32b232250128`;
cramfs (deterministic) `work/build/cramfs-180096074.little`, 10 715 136 bytes, sha256
`663dcadeb94c45c852d8fc5bdf54fbcca432448ba26a171599f2d00dc7e3ce8f` (changed from `2b59737d...` by the module symbol fix). e2fsck clean. A file-level diff against the first 074
build (kept as `pacemaker-revival-180096074.pfw.prev-pre-sweep`, sha256 `62c9e21d...b2b15`) shows only the 14 plugin
libraries, `boot/zImage`, `boot/cramfs.little` and, inside the cramfs, evrout.ko, pmic_input.ko and
`/usr/local/share/fonts/english.lang`. The version stays 180096074 because it has not been installed yet.

Unverified: everything on hardware (boot with the renamed kernel, loading the two modules with the moved `.modinfo`, music
playback with the patched plugin libraries); the "dead data" verdict for the plugin strings is static analysis; the moved
`.modinfo` against Tonium's actual 2.6.19.2 build (upstream source read, see rows 2 and 3); whether the kallsyms table holds
`logo_tonium_clut224` (not located).

### Still open: the bootloader string, an owner decision

The one item of the request that is not done is the `Tonium` USB manufacturer string in `boot/redboot.bin` (row 8): the
image keeps it byte-identical on purpose (see row 8 for why a patched copy would be ignored, and why forcing a flash
carries a brick risk with no rollback). Doing it needs three things together: a `bootloaderver` bump in the trailer and the
two 12838 constants in RedBoot, the 8-byte patch (`Revival`), and a hardware recovery path for mtd0 (programmer, JTAG or
serial boot, none verified on this board). It needs the owner's explicit yes as its own opt-in image.

### Module author tags: in place (13 Nov 2026)

The build now uses `patch-modinfo.py`'s default author `Revival project`, which fits inside the existing `.modinfo`
of both evrout.ko and pmic_input.ko after re-packing. The files keep their size and section table (test
`test_default_author_fits_in_place`), so the moved-section layout that was never loaded on a device is no longer
shipped. The move path stays in the script for longer names. pfw sha256 after this change:
`495bc0fa16e81800a9a7f9ace60fcb89868d37c2cf0a758d8a1ff12569ce625a`.
