---
id: pm-cues-loops
title: Cues, loops and reverse
topic: Using the Pacemaker
order: 6
keywords: cue, cue point, dj pause, autocue, loop, loop in, loop out, split loop, double loop, reverse, beat jump, fixed loop length, snap loops, vinyl mode, loop travel
summary: Setting and using a cue point, making and adjusting a loop, and playing in reverse on the Pacemaker.
---

# Cues, loops and reverse

Source: the official User's Manual v2.0 and the firmware's settings. See [Controls tour](pm-controls-tour.md) for what "verified" means.

## Cue

There is **one user cue point per track**. Loading another track clears it. **Autocue** (on by default) cues the track at its first beat when it loads. Cue points set in the Editor transfer to the Pacemaker. The app can set a track's cue from the beat grid editor ([The beat grid editor](grid-editor.md)); the unit of a cue written by the app has not been checked on a real Pacemaker.

1. With a track playing, press **Play** to **DJ-pause**: it stutters at the current position and the play and cue buttons blink. (Verified)
2. Move to the right spot with **Forward** or **Rewind** (hold), or by stroking the touchpad rim.
3. Press **Cue** to set the cue point. (Verified)

After that, **Cue** returns a playing track to the cue point. **Hold Cue** to play from it, and the track returns when you let go. (Verified)

## Loops

1. **Hold the P-switch up.**
2. Press **Rewind** to set the **loop in point** and **Forward** to set the **end point**. Then release the P-switch. (Verified)
3. Setting a loop fails if you release the P-switch, reverse, or change tempo before setting the end point. (Verified)

With the P-switch up, **Cue** exits the loop, and pressing Cue again re-enters it. Cueing, fast-forwarding or rewinding inside a loop also exits it. (Verified)

- **One loop per track** is stored, and a new loop overwrites it. It is saved permanently. (Verified)
- **Adjust:** hold the P-switch up, press Rewind (in) or Forward (end) and stroke the rim. (Verified)
- **Split or double:** hold the P-switch up, hold one of Rewind or Forward and tap the other. A split loop can only be doubled back up to its original length. (Verified)
- **Loop travel** is a setting (on by default) under **Settings, Mix control**, with Time stretch and Vinyl mode (the manual calls it "Vinyl pause"). The sources used here name them but do not say exactly what Loop travel changes. (Unverified)

The firmware also has **Fixed loop length**, **Snap loops** and **Beat jump length** settings that are **not in the manual**. How they are triggered, and what exactly they change, was not traced. (Unverified)

## Reverse

**Hold the P-switch up and press Play** to reverse playback. Press again to go forward. (Verified)

The app's own preview player (at the bottom of the window) also has a Cue button and a 4-beat loop button. That is the app, not the Pacemaker.
