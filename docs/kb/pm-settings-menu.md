---
id: pm-settings-menu
title: The settings menu
topic: Using the Pacemaker
order: 8
keywords: settings menu, settings, shutdown, volume, status, restore all, play mode, mix mode, mix save, halt, lock, repeat, shuffle, autocue, loop travel, time stretch, vinyl mode, vinyl pause, effect x-fader, phones x-fader, beat graph, lineout warning, normalize, phones range, volume limit, phones eq, lineout eq, standby, intensity, timeout, nightlight, buttons blink, end alert, track time, language, fx layout, track list filters, about, legal notice, model, settings.dat
summary: Every setting in the Pacemaker's settings menu with its exact options and default, how to change one on the unit, and how to edit them from the app.
---

# The settings menu

## Open the menu and change a setting

1. Load a track, then press and release the settings button.
2. Stroke along the touchpad rim to move through the entries.
3. Swipe right to open a folder or toggle a value. Each swipe steps to the next option.
4. For sliders (volume, EQ, volume limit), swipe right to select them, then stroke the rim to the level you want. For **Phones EQ** and **Lineout EQ**, first swipe right on **Treble**, **Mid** or **Bass** to pick the register, then stroke the rim to set its level.
5. To check the unit, open About, Status.

Settings that depend on the mode say so below. A new firmware number resets all settings to default: see [Installing firmware with the app](install-firmware.md).

## Edit settings from the app

1. Open the [Device page](device-page.md) and find **Device settings**.
2. Change values and press **Write N changes...**.
3. Use **Settings presets** to save the current settings, apply them again later or move them to another Pacemaker.

Levels and EQ are read-only in the app. See [The Device page](device-page.md) for the rules for writing.

## Top level

| Entry | Options | Default | What it does |
|---|---|---|---|
| Shutdown | action | | Switches the Pacemaker off |
| Volume | slider | | Master volume of the line output |
| Restore all | action | | Returns every preference to its factory value. Tracks and mixes are kept |
| Play mode (in Mix mode) or Mix mode (in Play mode) | action | | Switches between the two-channel mixer and the simple player. Play mode runs up to three times longer on a charge |
| Mix save | On, Off | Off | Starts and stops recording a mix. Off after every start: see [Recording mixes and making cases](pm-recording.md) |
| Halt | Continue, Save, Lock, New | | Interrupts or splits a recording |
| Lock, Unlock | action | | Locks the Pacemaker against input, and unlocks it |
| About | folder | | Status, Version and Legal notice (below) |

## Playback (Play mode)

| Setting | Options | Default | What it does |
|---|---|---|---|
| Repeat | Off, One, All | Off | Repeats the track or the whole list |
| Shuffle | On, Off | Off | Plays the list in random order |

## Settings, Mix control

Mix mode only.

| Setting | Options | Default | What it does |
|---|---|---|---|
| Autocue | On, Off | On | Cues each loaded track at its first beat |
| Fixed loop length | Off, 1/64, 1/32, 1/16, 1/8, 1/4, 1/2, 1, 2, 4 | Off | Loop length setting; its exact effect is not confirmed: see [Cues, loops and reverse](pm-cues-loops.md) |
| Snap loops | On, Off | On | Loop snapping setting; its exact effect is not confirmed |
| Beat jump length | 1/16, 1/8, 1/4, 1/2, 1, 2, 4, 8 | 1/2 | Beat jump distance setting; its exact effect is not confirmed |
| Loop travel | On, Off | On | Lets the loop in point push the end point and the reverse |
| Time stretch | On, Off | On | Keeps the key fixed when the tempo changes. Off lets the key follow the tempo |
| Vinyl mode | On, Off | Off | Pause scrubs like a record (On) or stutters like a DJ pause (Off). The manual calls it "Vinyl pause" |
| Effect x-fader | Off, Bass, Filter | Off | Folds the Bass or Filter effect into the crossfader |
| Phones x-fader | Relative, Normal | Normal | How the headphone crossfade indication reads |
| Beat graph | Off, On | On | Shows relative tempo and sync on the display |
| Lineout warning | On, Off | On | Warns before you do something that disrupts the line-out channel |

## Settings, Audio

| Setting | Options | Default | What it does |
|---|---|---|---|
| Normalize | On, Off | On | Brings all tracks to the same relative loudness |
| Phones, Range | Low imp, High imp | Low imp | Matches the headphone impedance. Keep the volume low when you change it |
| Phones, Volume limit | slider | | Caps the headphone volume. Not editable in the app |
| Phones, Phones EQ | Treble, Mid, Bass | 0 each | Tone of the headphone output. Not editable in the app |
| Lineout, Volume | slider | | Master line-out volume. Not editable in the app |
| Lineout, Lineout EQ | Treble, Mid, Bass | 0 each | Tone of the line output. Not editable in the app |
| Digital (Play mode) | On, Off | Off | Digital output |

## Settings, Configure

| Setting | Options | Default | What it does |
|---|---|---|---|
| Track list filters | Mix, Case, Album, Artist, Genre, Label, BPM, Key, Added, Rating, Type, Bitrate, Year, Remix (each On or Off) | On: Mix, Case, Album, Artist, Genre, BPM. Off: the rest | Which filters appear when you browse |
| Track list | Linked, Split | Split | One list shared by both channels, or one list per channel (Mix mode) |
| Track time, Display | Left, Played | Played | Shows time left or time played |
| Track time, End alert | On, Off | On | Time left blinks red when under 30 seconds remain (Mix mode) |
| FX layout, Edit | one of 13 functions per slot | see below | Puts a function on each of eight pad positions |
| FX layout, Restore | action | | Returns the pad to the default layout |
| Language | English, Korean, Japanese | English | Menu language |

### FX layout slots

The eight slots are North, East, South, West, and the same four with the P-switch up. Each takes one of Gain, Mid, Treble, Bass, Reverb, Echo, Roll, Hi-lo cut, Crush, Trans, Wah, Delay or Key. Default, listed as North, East, South, West: Treble, Mid, Bass, Gain without the P-switch; Reverb, Roll, Echo, Hi-lo cut with it. See [Effects, EQ and the Vocal filter](pm-effects.md). On Revival firmware with the Vocal filter on, the Wah slot works as the vocal filter.

## Settings, Power

| Setting | Options | Default | What it does |
|---|---|---|---|
| Standby | Off, 1 min, 5 min | 5 min | Idle time before the Pacemaker goes to standby |
| Intensity | Low, Medium, Bright | Medium | Backlight brightness |
| Timeout | Off, 10 sec, 30 sec | 30 sec | How long the backlight stays on |
| Nightlight | On, Off | Off | Night display lighting |
| Buttons blink | On, Off | On | Whether button lights blink |

## About

| Entry | Shows |
|---|---|
| Status | Battery percent, space available and used (GB), number of mixes and tracks |
| Version | Serial number, model, firmware, software, support |
| Legal notice | The licence notice |

On Revival firmware, **Model** reads "Pacemaker Revival" and **Legal notice** shows the Revival notice.

## Track list menu

Press the settings button in the track list to open **Add to case**, **Add all to case** and **Remove**: see [Recording mixes and making cases](pm-recording.md).
