---
id: pm-effects
title: Effects, EQ and the Vocal filter
topic: Using the Pacemaker
order: 5
keywords: effects, fx, filter, hi-cut, lo-cut, wah, crush, reverb, roll, echo, trans, delay, key, transpose, vocal filter, vocal, acapella, fx layout, pad, kill-all, revival-vocal, revival-no-preload, centre channel
summary: The effects the Pacemaker has, how to use them from the touchpad, how to change which effect is on which pad slot, and the Revival Vocal filter.
---

# Effects, EQ and the Vocal filter

Source: the official User's Manual v2.0, the firmware's settings and the Revival notes. See [Controls tour](pm-controls-tour.md) for what "verified" means. For EQ and gain, see [Mixing and beatmatching](pm-mixing.md).

## Using an effect

1. **Hold the P-switch up.** The pad now picks effects instead of EQ. (Verified)
2. **Swipe once from the centre** towards a direction. This sets **dry/wet** (or the filter value). **Swipe twice** to set **time** (Roll, Echo) or **size** (Reverb). Then **stroke the rim** without lifting your finger. (Verified)
3. Release the P-switch when you are done.

Only **one beat effect** can run at a time, except Trans. Effect settings **reset when a new track loads**. The **kill-all** button switches everything off and a second press brings it back. (Verified)

## Default pad layout (P-switch up)

| Direction | Effect |
|---|---|
| West | Hi-cut / lo-cut filter |
| North | Reverb |
| East | Roll |
| South | Echo |

(Verified from the manual and the firmware's defaults.) The other effects, **Wah, Crush, Trans, Delay and Key**, are not assigned by default. There are eight pad slots in all: the four directions with the P-switch up, and four more without it, which hold EQ and gain by default.

## Changing the pad layout

**Settings, Configure, FX layout, Edit** (see [The settings menu](pm-settings-menu.md)). Each of the eight slots takes one of the 13 functions: Gain, Mid, Treble, Bass, Reverb, Echo, Roll, Hi-lo cut, Crush, Trans, Wah, Delay and Key. **Restore** puts the defaults back. The firmware has a "Conflicting audio effect" message, so assigning the same function twice is probably checked. (Likely, not traced.)

## What the effects do

| Effect | What it does |
|---|---|
| **Hi-cut / lo-cut** (Filter) | A filter that sweeps both ways around a neutral centre. One side cuts the highs, the other cuts the lows |
| **Crush** | Crushes the sound: a sample-and-hold decimator on one side and a high-pass blend on the other |
| **Wah** | A swept band filter (range -100 to +100, 0 is off) |
| **Reverb, Echo, Roll, Delay** | Space, repeats and beat-repeat. Roll and Echo have a time, Reverb a size |
| **Trans** | A gate that chops the sound in time |
| **Key** | Transposes: swipe once from the centre to move in semitones within one octave, or twice to tune in percent of a semitone |

Effects work per channel after time-stretch and before the crossfader. (Verified from the firmware.)

## The Vocal filter (Revival firmware)

Revival firmware replaces the **Wah** effect with a **vocal filter**. The pad still says "Vocal" instead of "Wah" because the language files were edited. You still have to **assign Wah to a pad slot** under FX layout.

- **Which direction?** The Wah pad is a rotary donut centred on 0. In the app's default (Device page, Vocal filter card: **Vocal**), **right keeps the vocals** and **left cuts them**. The other card option, **Vocal inverted**, swaps the directions.
- **Focus (keep):** low and high bands and the side signal fade away. At full, only the centre band remains in mono, with a boost of up to about 3 dB.
- **Cut:** the centre band fades (about -48 dB at 1 kHz at full), while bass notes of male voices (100 to 200 Hz) and the hiss of "s" sounds (around 4 kHz) are barely touched.
- **It is a centre-channel trick, not true separation.** It works on dead-centre vocals and fails on wide, doubled or reverb-heavy ones. It also removes other centred sounds in that band, such as a snare.

### Switching it on

1. The Pacemaker must run **Revival firmware 180096073 or later**: see [Installing firmware with the app](install-firmware.md).
2. In the app, open the Device page and choose **Vocal** or **Vocal inverted** on the **Vocal filter** card. It creates the empty flag file `.Pacemaker/revival-vocal` (a file starting with the letter `i` swaps the directions). The flag is read **once at start**, so the Pacemaker must be restarted.
3. On the Pacemaker, assign Wah to a pad slot, and use it with the P-switch as for any effect.

**Switch it off:** choose **Off** on the card, or delete the flag file. **Emergency off:** create an empty `.Pacemaker/revival-no-preload` file and restart: every preloaded library is unloaded and the unit behaves like stock firmware.

Saved mixes that use the Wah tool sound like Vocal on the Pacemaker while the flag is on, because it is the same effect. In the app's Mix editor, Vocal and Wah tools are told apart: see [Mix tools and audio export](mixes-tools-export.md).

> **Not yet heard on real hardware.** The filter was tested on a PC and in an emulator. A boot log from a real unit shows the hook installed and the audio path running, but nothing was playing, so **how it sounds on the Pacemaker has not been judged.** Treat it as experimental.
