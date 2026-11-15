---
id: pm-mixing
title: Mixing and beatmatching
topic: Using the Pacemaker
order: 4
keywords: mixing, beatmatch, beat graph, crossfader, headphone cue, phones, cue in headphones, tempo, pitch, bend, sync, eq, gain, treble, mid, bass, kill-all, tempo change
summary: How to cue a track in your headphones, match its tempo by hand or with one press, blend with the crossfader and use the EQ.
---

# Mixing and beatmatching

Source: the official User's Manual v2.0, the firmware's key handling and the app's notes. See [Controls tour](pm-controls-tour.md) for what "verified" means.

## A first mix, step by step

1. **Load** a track on Channel 1 and press **Play** ([Browsing and loading tracks](pm-browsing-loading.md)).
2. Press **Channel 2** and load a second track. Do not play it yet.
3. **Cue it in your headphones.** Hold the **P-switch down** and slide the **crossfader** toward the channel you want to hear in the phones: all the way left is channel 1, all the way right is channel 2, and in between is a blend. Still holding the P-switch down, **stroke the rim** of the touchpad to set the phones volume. (Verified)
4. **Match the tempo** (below).
5. **Start the second track** on the beat, then slide the **crossfader** from one side to the other. (Verified)

## Matching the tempo

**One press.** With tracks playing on both channels, **hold the P-switch down and press Play** on the channel you want to adjust. Its tempo and phase sync to the other channel. (Verified) The Pacemaker relies on the **beat grid** stored with each track, so a track whose grid is missing or unreadable probably cannot sync properly. (Likely)

**By hand.** Hold the **P-switch up** and **stroke the touchpad rim** clockwise or anticlockwise to change tempo. The relative change and the BPM show on the screen. Release the P-switch when done. A quick stroke along the rim **bends** the tempo for a moment. (Verified) The manual says the bend can be turned on in the settings, but no setting with that name appears in the decoded settings tree. **Vinyl mode** is the nearest. (Unverified)

**The beat graph** shows both channels. White bars drifting right mean the active track is faster, drifting left mean slower. White bars that do not line up with the grey bars mean the phase is off, so bend the track. (Verified)

## The crossfader

It is **touch-sensitive and relative**: your finger can land anywhere and the indicator moves with your movement, not to where you touch. Full left is channel 1 only, full right is channel 2 only. The **Effect x-fader** setting (Off, Bass or Filter) can fold an effect into the crossfader. The **Phones x-fader** setting (Relative or Normal) changes how the headphone crossfader shows. (Verified)

## EQ and gain

1. Touch the **middle of the pad**.
2. Swipe towards **north, east, south or west**.
3. When your finger reaches the rim, **stroke around it** to change the value, all in one continuous movement.

Without the P-switch, the default layout is **Treble north, Mid east, Bass south and Gain west**. (Likely: it comes from the firmware's default settings, and the manual does not state it.) These values apply to the **current track only** and reset when you load a new track. (Verified) The line-out volume is set under **Settings, Audio, Lineout, Volume**.

**Kill-all** (the FX button) switches off all EQ and effect changes, and a second press brings them back. If you change any EQ or effect value after pressing kill-all, the earlier state can no longer be recalled. Kill-all does not affect Gain or Key. (Verified)

## Keep the headphone volume low

Phones range (Low or High impedance) and a volume limit are under **Settings, Audio, Phones**. See [The settings menu](pm-settings-menu.md). Effects are covered in [Effects, EQ and the Vocal filter](pm-effects.md).
