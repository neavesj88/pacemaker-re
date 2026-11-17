---
id: pm-effects
title: Effects, EQ and the Vocal filter
topic: Using the Pacemaker
order: 5
keywords: effects, fx, colour fx, beat fx, filter, hi-cut, lo-cut, wah, crush, reverb, roll, echo, trans, delay, key, transpose, tune, vocal filter, vocal, acapella, fx layout, pad, kill-all, revival-vocal, revival-no-preload, centre channel
summary: Use the Pacemaker's colour and beat effects and Key from the touchpad, change the pad layout and enable the Revival Vocal filter.
---

# Effects, EQ and the Vocal filter

For EQ and gain, see [Mixing and beatmatching](pm-mixing.md#adjust-eq-and-gain).

## Use a colour effect

Colour effects are Hi-cut / lo-cut filter, Wah (Vocal on Revival firmware) and Crush. By default only the filter is on the pad, at west.

1. Hold the P-switch up.
2. Swipe from the centre to the direction of the effect. The current value shows on screen.
3. Without lifting the finger, stroke the rim clockwise or anticlockwise to raise or lower the value.
4. Lift the finger, then release the P-switch.

## Use a beat effect

Beat effects are Roll, Echo, Reverb, Trans and Delay. Each has two values: dry/wet, and time (Roll, Echo) or size (Reverb). By default Reverb is at north, Roll at east and Echo at south. Trans and Delay are not on the pad by default.

1. Hold the P-switch up.
2. Swipe once from the centre to the direction of the effect to set dry/wet. Swipe twice to set time or size.
3. Without lifting the finger, stroke the rim to change the value. The value shows on screen.
4. Lift the finger, then release the P-switch.

Only one beat effect runs at a time, except Trans.

## Use Key

Key has no default place on the pad. Assign it first: see [Change the layout](pm-effects.md#change-the-layout).

| Gesture | Sets |
|---|---|
| Swipe once to its slot, then stroke the rim | Transpose in semitones within one octave, up or down |
| Swipe twice to its slot, then stroke the rim | Tune the pitch in percent of a semitone, up or down |

Key changes are not touched by kill-all.

## Notes for every effect

- Make the swipe and the rim stroke in one movement. Lifting the finger ends the adjustment.
- Effect settings apply to the current track and reset when a new track loads.
- Kill-all restores only if no EQ or effect value changed since; see [Mixing and beatmatching](pm-mixing.md#kill-all).
- The effects built in as tools of a saved mix are described in [Mix tools and audio export](mixes-tools-export.md).

## Pad layout

Default with the P-switch up:

| Direction | Effect |
|---|---|
| West | Hi-cut / lo-cut filter |
| North | Reverb |
| East | Roll |
| South | Echo |

Wah (Vocal on Revival), Crush, Trans, Delay and Key are not assigned by default. Four more slots (without the P-switch) hold EQ and gain.

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
| Key | Transposes in semitones, or tunes in percent of a semitone |

## Vocal filter

Revival firmware replaces Wah with a vocal filter, shown as "Vocal" on the pad and in the FX layout list. It sits in the same slot as Wah, so it is used like Wah: assign it to a pad slot and turn the rim right or left of 0.

| Card option | Right of 0 | Left of 0 |
|---|---|---|
| Vocal (default) | Keeps vocals | Cuts vocals |
| Vocal inverted | Cuts vocals | Keeps vocals |

0 is off. Works on centre-panned vocals only; other centred sounds (snare) are affected too.

### Enable the Vocal filter

1. Run Revival firmware 180096073 or later. See [Installing firmware with the app](install-firmware.md).
2. In the app, open the Device page and choose **Vocal** or **Vocal inverted** on the **Vocal filter** card. See [The Device page](device-page.md#vocal-filter).
3. Restart the Pacemaker.
4. Open **Settings, Configure, FX layout, Edit** and assign **Vocal** to a pad direction. The slot is labelled Vocal on Revival firmware (Wah on stock). Use it with the P-switch like any colour effect.

### Disable the Vocal filter

- Choose **Off** on the card.
- Emergency off: create an empty `.Pacemaker/revival-no-preload` file on the unit and restart. Disables all Revival libraries.

Saved mixes that use the Wah tool play as Vocal while the filter is on. See [Mix tools and audio export](mixes-tools-export.md).
