---
id: pm-settings-menu
title: The settings menu
topic: Using the Pacemaker
order: 8
keywords: settings menu, settings, shutdown, volume, restore all, play mode, mix mode, mix save, halt, lock, autocue, loop travel, time stretch, vinyl mode, effect x-fader, phones x-fader, beat graph, lineout warning, normalize, phones range, standby, intensity, timeout, nightlight, buttons blink, end alert, track time, language, fx layout, track list filters, settings.dat
summary: One table of every setting in the Pacemaker's settings menu, with its options and default, and how to change them from the app.
---

# The settings menu

Press and release the **settings button** with a track loaded to open the menu. The settings are stored in `.Pacemaker/settings.dat`, which the app can read and edit on the [Device page](device-page.md) (the **Device settings** card, and **Settings presets** to save and re-apply them). A **new firmware number deletes that file**, so settings reset to the defaults: see [Installing firmware with the app](install-firmware.md).

Source: the official manual v2.0, and the settings the firmware itself defines. See [Controls tour](pm-controls-tour.md) for what "verified" means. **Restore all** puts preferences back to default but does not touch tracks or mixes.

## On the first screen and in Playback

| Setting | Options | Default | What it does |
|---|---|---|---|
| Shutdown | (action) | | Switches the Pacemaker off |
| Volume | | | Line-out volume of the mix |
| Restore all | (action) | | Resets preferences, not tracks or mixes |
| Play mode / Mix mode | (action) | | Switches between the simple player and the two-channel mixer. Play mode uses less power |
| Mix save | On, Off | Off (forced off at every start) | Records your mix: see [Recording mixes and making cases](pm-recording.md) |
| Halt | Continue, Save, Lock, New | | Ends or continues a recording |
| Lock, Unlock | (action) | | Locks the Pacemaker. **How to unlock was not traced** (unverified) |
| Repeat (Play mode) | Off, One, All | Off | Repeat behaviour in Play mode |
| Shuffle (Play mode) | On, Off | Off | Shuffle in Play mode |

## Settings, Mix control

| Setting | Options | Default | What it does |
|---|---|---|---|
| Autocue | On, Off | On | Cues a track at its first beat when it loads |
| Fixed loop length | Off, 1/64 to 4 | Off | Loop length. **Not in the manual; how it works was not traced** |
| Snap loops | On, Off | On | **Not in the manual; not traced** |
| Beat jump length | 1/16 to 8 | 1/2 | **Not in the manual; how a jump is triggered was not traced** |
| Loop travel | On, Off | On | Named in the firmware; the manual does not explain it |
| Time stretch | On, Off | On | Keep the pitch when the tempo changes |
| Vinyl mode (the manual calls it "Vinyl pause") | On, Off | Off | Not explained further in the sources used here |
| Effect x-fader | Off, Bass, Filter | Off | Folds an effect into the crossfader |
| Phones x-fader | Relative, Normal | Normal | How the headphone crossfader shows |
| Beat graph | Off, On | On | The beat graph on the screen |
| Lineout warning | On, Off | On | A warning about the line-out |

## Settings, Audio

| Setting | Options | Default | What it does |
|---|---|---|---|
| Normalize | On, Off | On | Evens out loudness using each track's stored gain |
| Phones, Range | Low imp, High imp | Low imp | Headphone impedance. Keep volume low |
| Phones, Volume limit | | | A raw engine value; not editable in the app |
| Phones EQ | Treble, Mid, Bass | 0 | Headphone tone |
| Lineout, Volume and EQ | | | Raw values; not editable in the app |
| Digital (Play mode) | On, Off | Off | Digital output |

## Settings, Configure

| Setting | Options | Default | What it does |
|---|---|---|---|
| Track list filters | Mix, Case, Album, Artist, Genre, Label, BPM, Key, Added, Rating, Type, Bitrate, Year, Remix | On for Mix, Case, Album, Artist, Genre and BPM; off for the rest | Which filters the track list offers |
| Track list | Linked, Split | Split | One shared list, or one per channel |
| Track time, Display | Left, Played | Sources disagree (unverified) | Shows the time left or the time played |
| End alert (Mix mode) | On, Off | On | Warns near the end of a track |
| FX layout, Edit | 8 slots (north, east, south, west, and the same with the P-switch up) | EQ and gain without the P-switch, effects with it | Which function sits on which pad slot: see [Effects, EQ and the Vocal filter](pm-effects.md) |
| FX layout, Restore | (action) | | Restores the default layout |
| Language | English, Korean, Japanese | English | The menu language |

## Settings, Power

| Setting | Options | Default | What it does |
|---|---|---|---|
| Standby | Off, 1 min, 5 min | 5 min | Time before it goes to standby |
| Intensity | Low, Medium, Bright | Medium | Backlight brightness |
| Timeout | Off, 10 s, 30 s | 30 s | How long the screen's backlight stays on |
| Nightlight | On, Off | Off | A night-light setting (the manual does not describe it further here) |
| Buttons blink | On, Off | On | Whether the buttons blink |

## About

Status (battery percent, space available and used, number of mixes and tracks), Version (serial number, model, firmware, software, support) and **Legal notice**. These are read-outs. The app masks the serial number.

## Track list menu

In the track list, the settings button opens a menu with **Add to case**, **Add all to case** and **Remove**: see [Recording mixes and making cases](pm-recording.md).
