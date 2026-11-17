---
id: pm-effects
title: Effects, EQ and the Vocal filter
topic: Using the Pacemaker
order: 5
keywords: effects, fx, filter, hi-cut, lo-cut, wah, crush, reverb, roll, echo, trans, delay, key, transpose, vocal filter, vocal, acapella, fx layout, pad, kill-all, revival-vocal, revival-no-preload, centre channel
summary: Use the Pacemaker's effects from the touchpad, change the pad layout and enable the Revival Vocal filter.
---

# Effects, EQ and the Vocal filter

For EQ and gain, see [Mixing and beatmatching](pm-mixing.md).

## Use an effect

1. Hold the P-switch up.
2. Swipe once from the centre towards a direction to set dry/wet (or the filter value). Swipe twice to set time (Roll, Echo) or size (Reverb).
3. Stroke the rim without lifting the finger.
4. Release the P-switch.

Only one beat effect runs at a time, except Trans. Effect settings reset when a new track loads. Kill-all restores only if no EQ or effect value changed since; see [Mixing and beatmatching](pm-mixing.md#kill-all).

## Pad layout

Default with the P-switch up:

| Direction | Effect |
|---|---|
| West | Hi-cut / lo-cut filter |
| North | Reverb |
| East | Roll |
| South | Echo |

Wah, Crush, Trans, Delay and Key are not assigned by default. Four more slots (without the P-switch) hold EQ and gain.

### Change the layout

1. Open **Settings, Configure, FX layout, Edit**.
2. Assign a function to each of the eight slots: Gain, Mid, Treble, Bass, Reverb, Echo, Roll, Hi-lo cut, Crush, Trans, Wah, Delay or Key.
3. Choose **Restore** for the defaults.

## Effect reference

| Effect | Function |
|---|---|
| Hi-cut / lo-cut (Filter) | One side cuts highs, the other cuts lows |
| Crush | Sample-rate reduction on one side, high-pass on the other |
| Wah | Swept band filter, range -100 to +100, 0 is off |
| Reverb, Echo, Roll, Delay | Space, repeats and beat-repeat. Roll and Echo have a time, Reverb has a size |
| Trans | Gate that chops the sound in time |
| Key | Transposes. Swipe once for semitones within one octave. Swipe twice to tune in percent of a semitone |

## Vocal filter

Revival firmware replaces Wah with a vocal filter, shown as "Vocal" on the pad. Turn the pad right or left of 0:

| Card option | Right | Left |
|---|---|---|
| Vocal (default) | Keeps vocals | Cuts vocals |
| Vocal inverted | Cuts vocals | Keeps vocals |

Works on centre-panned vocals only; other centred sounds (snare) are affected too.

### Enable the Vocal filter

1. Run Revival firmware 180096073 or later. See [Installing firmware with the app](install-firmware.md).
2. In the app, open the Device page and choose **Vocal** or **Vocal inverted** on the **Vocal filter** card.
3. Restart the Pacemaker.
4. Assign Wah to a pad slot under FX layout and use it with the P-switch like any effect.

### Disable the Vocal filter

- Choose **Off** on the card.
- Emergency off: create an empty `.Pacemaker/revival-no-preload` file on the unit and restart. Disables all Revival libraries.

Saved mixes that use the Wah tool play as Vocal while the filter is on. See [Mix tools and audio export](mixes-tools-export.md).
