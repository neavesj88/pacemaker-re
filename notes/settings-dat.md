# `.Pacemaker/settings.dat`

The device's user settings: the whole settings menu state (backlight, EQ, volumes, loop/FX
options, track-list filters...) plus a "most recently used" stamp per setting. It is a
**boost::serialization text archive** of one `SettingRepository` object. The new Editor does not
need to write it (the device creates it on first shutdown), but it can read and edit it.

Parser and byte-exact writer: `scripts/settingsdat.py` (`--check` re-emits the file and compares).
Verified against a real device dump (1955 bytes, 104 settings, firmware 180096069): parse then
re-emit is **byte-identical**.

Everything below was read from the unstripped ARM `pmplayer`. Addresses are virtual addresses in
**180096069** (the firmware that wrote the dump) unless marked 14142. The two builds share the
file layout; they differ in the repository class version (16 vs 13) and in the setting ids.

## Who reads and writes it

| What | Function (180096069) |
|---|---|
| path | `getSettingsFilename` 0x34e150: `osal::getApplicationDataPath("settings.dat")`, i.e. `.Pacemaker/settings.dat` on the FAT volume (the dump has it there) |
| load (once, at app start) | `SettingRepository::load` 0x35f6d8, called from `PMRunningApp::PMRunningApp` 0x342e90 |
| save | `SettingRepository::save` 0x35f0b4 -> `priv_save` 0x35ebbc (text_oarchive into a stringbuf, then `futile::stringToFile`). Called from `PMRunningApp::~PMRunningApp` 0x33ace0 (shutdown) and `SettingsTask::execute` 0x39568c (task type 0) |
| not in this file | the UI language, saved separately by `is::saveLanguage()` right after `priv_save` (`/mnt/private/language_setting.dat`) |

`load` deserialises into a temporary repository, then for each loaded setting finds the
registered one with the same id and calls its `CopyValue` (a type mismatch is logged with
`duff_logf`); ids the firmware does not know are simply added. Afterwards it calls
`valueChanged(false)` on every non-string setting so the device applies the values, and copies the
repository's MRU counter. Consequences for an editor: a file may contain any subset of the
settings (missing ones keep their defaults), and float values are clamped by the setting's own
min/max on load (`FloatSetting::CopyValue`, read in 14142 at 0x351550; not re-read in 180096069).

## Archive grammar

Header and first two lines of the dump, token by token:

    22 serialization::archive 4  0 16  0 0  104 0  1 1 0
    0  0 0  2004 604 0  1  1
    ^objid ^Setting-base class info (tracking, version)   ^value of item 0   ^class id of item 1
    1  2003 604 9  0  3 1 0
    ...
    103 301 201 0 0 4983

All tokens are separated by one space; boost starts a new line before every object id, so the file
has 104 newlines (one per item, none at the end). No trailing newline.

| tokens | meaning | evidence |
|---|---|---|
| `22 serialization::archive` | signature (length-prefixed) | `basic_text_oarchive::init` |
| `4` | boost archive library version (`ARCHIVE_VERSION()`) | `init` 0x670cf8 (14142) |
| `0 16` | class info of `SettingRepository`: tracking=0, class version=**16** (0x10). 14142 uses 13 | `oserializer<..SettingRepository>::version` 0x36219c returns 0x10; `SettingRepository::serialize<text_oarchive>` 0x3669b8 throws unless version == 0x10 |
| `0 0` | class info of `std::vector<Setting*>`: tracking=0, version=0 | `oserializer<..vector<Setting*>>::version` 0x362368 |
| `104 0` | element count, then the collection `item_version` (written when library version > 3) | vector `save_object_data` 0x36b168 |
| per item | see below | |
| `4983` (last token) | repository field at +0x4c, the global MRU counter | `serialize<text_oarchive>` 0x3669b8 writes `*(this+0x4c)` after the vector; `updateMRUCounter` increments it |

(`class_id_optional` writes nothing in a text archive: `save_override(class_id_optional_type)` at
0x6703ac (14142) is empty. That is why repo/vector class info is only `tracking version`.)

`SettingRepository::serialize` writes, in order: five calls that register the pointer
serializers, then **only the saveable settings** (`Setting+0x24 != 0`) as a `vector<Setting*>`,
then the counter. The registration order fixes the boost class ids (repository is 0):

| class id | class | `pointer_oserializer<...>::instance` order in 0x3669b8 |
|---|---|---|
| 1 | `BoolSetting` | 0xad1f90 |
| 2 | `FloatSetting` | 0xad1fa0 |
| 3 | `FloatListSetting` | 0xad1fb0 |
| 4 | `ParentSetting` | 0xad1fc0 |
| 5 | `StringSetting` | 0xad1fd0 |

**Per item** (boost pointer save, `basic_oarchive_impl::save_pointer` 0x649564 in 14142):

1. class id. The **first** time a class appears this is followed by its class info
   `tracking version` (always `1 0` here); after that only the id.
2. object id: 0, 1, 2 ... in order (the parser checks it).
3. only for the very first item: class info of the `Setting` base (`0 0`).
4. the `Setting` base: `id`, `parent id`, `lastUsed`. `oserializer<..Setting>::save_object_data`
   0x366634 writes object offsets +4, +0xc, +0x20. `getID` / `getParentID` / `getLastUsed` return
   exactly those (0x34f4f8 / 0x34f4f0 / 0x34f4e8 in 14142; same in 180096069).
5. the class's own value, if any:

| class | value tokens | encoding | evidence |
|---|---|---|---|
| BoolSetting | 1 | `0`/`1` | `save_object_data` 0x36acb8, field +0x30 |
| FloatSetting | 1 | C++ `ostream << double` at precision 17 = `%.17g` (set to 0x11 in 0x36ae00) | field +0x38 |
| FloatListSetting | 1 | integer **index** into the setting's option list | 0x36a0cc, field +0x28 |
| ParentSetting | 0 | menu folder, no value | 0x3641e8 (base only) |
| StringSetting | 0 | **not stored**: only the base is serialised. The strings are live read-outs filled in by `PMRunningApp::ISetting_ValueRequested` 0x337a00 (serial, model, firmware, software, battery %, free/used GB) or fixed labels | 0x369818 (base only) |

Round trip detail that matters for a writer: because class info is emitted on first use, the
order of items decides where `1 0` pairs appear. `scripts/settingsdat.py` recomputes that, so any
item order re-emits to a valid archive (tested with shuffled items).

## Ids, folders and meaning

`id` is the firmware's numeric setting id, `parent` is the id of the folder (a `ParentSetting`) it
sits in; parent 0 is the root. Both are fixed by constructors in pmplayer, so the whole table is
known (all 104 ids in the dump are accounted for). Folder titles and the three settings classes
that create the rest: `PMSettingsAlways` 0x33fe7c, `PMSettingsPlayMode` 0x33c1f8,
`PMSettingsMixMode` 0x33c6f0, and `SettingMediator::SettingMediator` 0x397754 (folders via
`FUN_00395854` from a 18-row table plus 2400/2402/2403 and five read-only strings).
Constructor arguments are `(id, title STRING_ID, parent id, default, subtype)`. Titles come from
`resources/fonts/english.lang` in the firmware image (`N "text"` per line); the script reads that
file for `--names`. **ids from 14142 are a different, older scheme** (small numbers such as 0x55 for
what 180096069 calls 2004), so the table is only valid for the 180096069 id scheme.

Menu tree and meaning (type: B bool, F double, L list index, S string/no value; `mix`/`play`
= created by the mix-mode or play-mode settings object, the rest are global):

| id | title | type, default | what the code shows |
|---|---|---|---|
| **1 Playback** | | folder | |
| 800 | Repeat (play) | L Off/One/All, 0 | subtype 100 |
| 801 | Shuffle (play) | B off | |
| **2 About** (3 Status, 4 Version) | | | read-outs |
| 1000, 2005 | Battery (%) | S | `getBatteryLevel` |
| 1001, 1002 | Available / Used (GB) | S | `getHddCapacityAvailableInGb` / `...UsedInGb` |
| 1003, 1004 | Mixes, Tracks | S | |
| 1100..1104 | Serial no, Model, Firmware, Software, Support | S | 1100 serial, 1101 model, 1102 firmware, 1103 software (`ISetting_ValueRequested`) |
| 1200 | Legal notice | S | |
| **5 Settings** | | | |
| **6 Mix control** | | | all `mix` |
| 1300 | Autocue | B on | |
| 1301 | Fixed loop length | L Off,1/64,1/32,1/16,1/8,1/4,1/2,1,2,4; 0 | subtype 113 |
| 1302 | Snap loops | B on | |
| 1303 | Beat jump length | L 1/16,1/8,1/4,1/2,1,2,4,8; 3 (=1/2) | subtype 114 |
| 1304 | Loop travel | B on | -> `pe::AudioPlayer::setLoopTravel` |
| 1305 | Time stretch | B on | |
| 1306 | Vinyl mode | B off | |
| 1307 | Effect x-fader | L Off/Bass/Filter, 0 | subtype 111 |
| 1308 | Phones x-fader | B false=Relative true=Normal, on | subtype 5 |
| 1309 | Beat graph | L Off/On, 1 | subtype 112 |
| 1310 | Lineout warning | B on | -> `PmTrackList::setLineoutWarning` |
| **7 Audio** | | | |
| 1600 | Normalize | B on | |
| 8 Phones: 100 | Range | L Low imp/High imp = 1/0, 0 | subtype 104 |
| 101 | Volume limit | F, default 2*pi (6.2831854820251465) | raw engine units |
| 200 Phones EQ: 1400/1401/1402 | Treble/Mid/Bass | F, 0 | sent to the audio thread (`ui2rt::SendChangedSetting`) |
| 201 Lineout: 300 | Volume (mix) | F, 2.284794807434082 | audio thread |
| 301 | Digital (play) | B off | -> `ALSAMixer::setDigitalOutput` |
| 400 Lineout EQ: 1500/1501/1502 | Treble/Mid/Bass | F, 0 | audio thread |
| **401 Configure** | | | |
| 402 Track list filters: 1700..1713 | Mix, Case, Album, Artist, Genre, Label, BPM, Key, Added, Rating, Type, Bitrate, Year, Remix | B; on for Mix..BPM except Label (1705), off for the rest | -> `PmTrackList::enableNavigationColumn` |
| 500 | Track list | B false=Linked true=Split, on | -> `PmTrackList::setLayout` |
| 600 Track time: 1800 | Display | B false=Left true=Played, on | -> `TrackArea::setShowTimeRemain(!value)` |
| 1801 | End alert (mix) | B on | -> `TrackArea::enableEndOfTrackWarning` |
| 601 FX layout / 602 Edit: 1900..1907 | 8 pad slots North, East, South, West, P-up/North, P-up/East, P-up/South, P-up/West | L index into [Gain, Mid, Treble, Bass, Reverb, Echo, Roll, Hi-lo cut, Crush, Trans, Wah, Delay, Key]; defaults 2,1,3,0 then 4,6,5,7 | subtype 110; changes go through `sendFxMap` |
| 1908 | Restore (FX config) | S | action |
| 603 Language: 700/701/702 | English / Korean / Japanese | S | actions -> `is::setLanguage` |
| **604 Power** | | | |
| 2000 | Standby | L Off,1 min,5 min = 0/60/300 s, idx 2 | -> `PwrSettings::setStandbyTimeOut` |
| 2001 | Intensity | L Low,Medium,Bright = 60/125/200, idx 1 | -> `PwrMgr::setBacklightIntensity` |
| 2002 | Timeout | L Off,10 sec,30 sec = 0/10/30, idx 2 | -> `PwrSettings::setBacklightTimeOut` |
| 2003 | Nightlight | B off | sent via `ui2uihelper::SendChangedSetting` |
| 2004 | Buttons blink | B on | same |
| root / unfiled | | | |
| 900 | Shutdown | S (action) | |
| 901 | Volume (mix, root) | F, 2.284794807434082 | audio thread |
| 2100 | Restore all | S (action) | |
| 2203 / 2204 | Play mode (in mix settings) / Mix mode (in play settings) | S (actions) | switch mode |
| 2205 | Mix save | B off | `Halt::runModal(value)`; `PMRunningApp` ctor forces it to false right after `load` |
| 2206 | Halt | S (action) | |
| 2300 / 2301 | Lock / Unlock | S (actions) | |
| 2200, 2201 | (no title), play mode | F, default pi (3.1415927410125732) | audio thread; guess: per-channel volumes in play mode |
| 2202 | (no title), mix mode | F, 2.284794807434082 | audio thread; guess: a mix-mode volume |
| 2400 Track list menu (2402 Add to case, 2403 Add all to case), 2404 Remove | | folders / S | track-list context menu |

Folder tree (ids with titles): 1 Playback; 2 About > 3 Status, 4 Version; 5 Settings > 6 Mix
control, 7 Audio > (8 Phones > 200 Phones EQ), (201 Lineout > 400 Lineout EQ), 401 Configure >
(402 Track list filters), (600 Track time), (601 FX layout > 602 Edit), (603 Language), 604 Power;
2400 Track list menu > 2402, 2403.

Value labels for lists and bools are decoded from `FloatListSetting::setupValidValues` 0x35fefc
and `BoolSetting::setupValidValues` 0x35c214 (label STRING_ID per index; some lists also map the
index to a number: backlight 60/125/200, timeouts 0/10/30, standby 0/60/300, impedance 1/0). In
`BoolSetting` the label for false is at +0x28 and for true at +0x2c (`getValueAsString` 0x35e770).

## The MRU stamp (`lastUsed`, and the trailing counter)

`Setting::valueChanged(true)` 0x35d290 does `counter += 1; setting.lastUsed = counter`, where the
counter is `SettingRepository+0x4c`. If the setting's flag at +0x25 is set it stamps its **parent**
folder instead (`setDelegatesMRUToParent`). `SettingRepository::getMRUList` 0x35f1a4 collects the
non-string settings that have listeners (or are folders) and a non-zero stamp and sorts them; this is
the "recent settings" list at the top of the settings menu. `defaultMRU` 0x35d41c seeds six entries
(2001, 2003, 1600, 1300, 800, 801), which is visible in the dump as stamps 8, 9, 10, (11), 12, 13.
The final token is the counter itself, so it must be >= every stamp (dump: counter 4983 = largest
stamp, on item 2202). A writer that edits a value should bump the counter and set the stamp if it
wants the setting to show up in the recent list; leaving stamps alone is harmless.

## What this device's dump holds

104 items: 21 folders, 78 settings that the three settings objects create, plus 5 read-only strings
from the mediator. Values that differ from the firmware defaults (`--changed`): Lineout EQ treble/mid/
bass slightly up (0.021, 0.059, 0.041), phones EQ treble -0.0099, Lineout and root mix-mode Volume
1.3456 (default 2.2848), play-mode floats 2200 = 2.856 and 2201 = 2.2848, track-list filter columns
Mix and Album off, track time shows "Left", and the "P-up/North" FX slot set to Hi-lo cut (7),
which is also what "P-up/West" is (1904 and 1907 both 7; the UI has a "Conflicting audio effect"
string, so this may be a state the menu would not normally allow; not verified). MRU stamps
reach 4983, so the device has been used a lot.

## Confidence

| Part | Confidence |
|---|---|
| Archive grammar, item layout, class ids, value encodings, byte-exact re-emit | **high** - decompiled save/load code for both firmwares, and the dump re-emits byte-identical |
| Repository class version 16 (180096069) / 13 (14142) | high, and strict: `serialize` throws on any other version |
| ids, parents, types, defaults for the 104 ids | high - read from constructors, and the set matches the dump exactly (78 + 21 + 5) |
| Titles, list/bool labels | high for the strings; the id -> title link is the constructor argument |
| Behavioural meaning of ids with a named handler (2000-2004, 301, 500, 1304, 1310, 1700-1713, 1800, 1801, 2205, and the read-only strings) | high |
| 1305-1309 (mix control), 1300-1303, 1900-1907 semantics beyond the title | medium: title and options are certain, the exact audio effect of each flag is not traced |
| Float units of 101, 300, 901, 1400-1402, 1500-1502, 2200-2202 | **low**: raw parameter values passed to the audio thread; the "volume" reading of 300/901/2200-2202 is from titles and defaults |
| `lastUsed` / counter semantics | high for the mechanics, medium for the exact menu behaviour |

## Unknowns

- What `load` does when the file has another class version (boost throws; whether the caller
  catches it and starts from defaults was not traced). Treat a version mismatch as "file ignored or
  fatal" and never write a version other than the firmware's.
- Min/max of each `FloatSetting` (set by `setMinMax` somewhere; `CopyValue` clamps to them), so valid
  ranges for the EQ/volume floats are not known.
- Which settings use `setDelegatesMRUToParent` (inferred: the EQ float groups, because folders 200
  and 400 carry stamps while their children carry 0).
- Whether other firmwares (13557, 14108, 16219, 2012-2015 betas) use the 14142 or the 180096069 id
  scheme; only 14142 and 180096069 were looked at.
- Whether the archive version token `4` plus `item_version` implies a particular boost release; it is
  an old boost (the vector code branches on library version > 3). Not needed for interop.

## Using it

    python -I scripts/settingsdat.py work/device-dump/settings.dat --check     # byte-exact round trip
    python -I scripts/settingsdat.py work/device-dump/settings.dat --names     # decoded menu tree
    python -I scripts/settingsdat.py work/device-dump/settings.dat --changed   # non-default values

As a module: `settingsdat.parse(text)` returns an `Archive` with `.items` (each `.cls .id .parent
.mru .value`) and `.mru_counter`; `settingsdat.emit(archive)` returns the file text.
