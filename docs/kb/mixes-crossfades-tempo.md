---
id: mixes-crossfades-tempo
title: Crossfades, tempo and Match all
topic: Mixes
order: 2
keywords: crossfade, transition, fade, curve, bass swap, long blend, echo out, filter sweep, drop swap, hard cut, loop out, loop cut, loop, halving, stutter, recipe, auto transition, beat-match, mix tempo, match all, sync, half time, double time, fine bpm, pitch, stretch, tempo ramp, plays at
summary: Set crossfades, build transitions from eight recipes, beat-match clips, and match every track to the mix tempo.
---

# Crossfades, tempo and Match all

## Crossfades

Where two clips overlap on the bar, the overlap is a crossfade. Click it to open its panel.

| Control | Action |
|---|---|
| Curve | Linear, Fast in, Slow in, Equal power, Cut, or a drawn curve (click to add a node, drag to move it) |
| Fade after | Outgoing clip fades after half or three quarters of the overlap |
| **Mute** | Switch the crossfade off |
| **Beat-match clips** | Beat-match the later clip to the clip before it ([Clip tempo](mixes-crossfades-tempo.md#clip-tempo-and-fine-bpm)) |
| **Fit to overlap** | Shorten a fade that shows "cut to overlap" on the bar |

## Transition recipes

A recipe builds the crossfade and its tools in one step. Select the crossfade, choose a recipe and press **Rebuild as ...**, or select a clip and press **Auto transition**. The clips must overlap by at least the recipe's shortest length.

| Recipe | What it does | Length in bars |
|---|---|---|
| Bass swap | Linear crossfade. Incoming bass stays out until the middle, then the basses swap on one beat | 1 to 32 (16 default) |
| Long blend | Slow S-shaped crossfade, basses traded gradually | 8 to 64 (32 default) |
| Echo out | Outgoing track dissolves into its echoes; incoming track enters over the second half | 1 to 8 (4 default) |
| Filter sweep | Outgoing track thinned by a rising high-pass; incoming track opens from a low-pass | 2 to 32 (8 default) |
| Drop swap | Outgoing track builds (bass out, rising high-pass, beat-repeat on the last bar); incoming track hits full on the last beat | 2 to 16 (8 default) |
| Hard cut | Straight cut. The cut falls at the end of the length (a phrase is 8 bars), or at the start | one beat to 8 (8 default) |
| Loop out | Outgoing track loops a short slice while the incoming track comes in; basses swap in the middle | 2 to 16 (8 default) |
| Loop cut | Outgoing track loops a bar, then half a bar, then half again, down to a quarter beat; incoming track drops in at the end | 2 to 16 (8 default) |

## Auto transition settings

**Settings > Mixes > Auto transitions**:

| Setting | Effect |
|---|---|
| Auto crossfade length | Default 16 bars |
| Crossfade sits | Ends with the outgoing clip (default), or starts on the incoming clip's first beat |
| When a clip moves | Crossfade and tools follow the clips (default), or stay where made |
| Your own tools in the way | What happens to tools you placed where a recipe needs room |

## Loop out and Loop cut

| Setting | Range and default |
|---|---|
| Loop out: loop length | 1 beat to 8 bars, 2 bars |
| Loop out: starts | Start of the overlap (default) or middle |
| Loop cut: first loop | 1 beat to 8 bars, 1 bar. Choose 4 bars for 4, 2, 1 bars, then beats |
| Loop cut: shortest loop | 1/8 to 1 beat, 1/4 beat |
| Loop cut: times each length | 1 to 4, 2 |
| Loop cut: rising filter | Off by default. Adds a rising high-pass over the loops |

Loop cut needs an overlap of 8 bars or more at defaults; loops that do not fit are left out, longest first.

## When you move a clip

The crossfade and its recipe tools re-fit to the new overlap. Tools placed by hand inside the overlap stay put. **Rebuild as ...** snaps the tools back onto a beat.

- With no overlap left the crossfade does nothing; it returns on drag back or Undo.
- Dropping a clip onto another does not create a crossfade.

## Mix tempo

The toolbar has one **mix tempo** box. New tracks and the Match buttons play at it, or at double or half of it, whichever stretches the track least. It is not saved; a reopened mix starts at its first track's tempo.

## Match all

**Match all** plays every track at the mix tempo, half and double aware: a 70 BPM track in a 140 BPM mix plays at 70. A message reports the result (for example "Matched 5 tracks to 128 BPM; 2 kept") with **Details** and **Undo**. **Details** lists:

- tracks matched and tracks already at the tempo,
- tracks skipped because they have a tempo ramp (use **Sync** on the clip),
- tracks left at their own tempo because matching would stretch them too far.

Under **Settings > Mixes > Tempo matching**:

- **Warn above**: the allowed stretch, as a percentage. A track over it gets a stretch badge; its panel offers **Keep its own tempo** or match anyway.
- Whether tracks are matched when added.
- What a new track does when it needs more stretch than allowed.

## Clip tempo and fine BPM

Select a clip to use its panel:

| Control | Action |
|---|---|
| Track BPM | Tempo of the track itself |
| Plays at | Tempo in the mix; type a number to change it |
| Octave: **half time**, **double time** | Correct a wrong automatic octave |
| Pitch / speed | Steps of -0.1, -0.01, +0.01, +0.1 percent, or type a percentage. By default tempo changes keep the pitch (time-stretch) |
| **Sync to mix tempo**, **Sync to previous clip** | Match this clip to either (half or double included) |
| **Add ramp node at playhead** | Make a tempo ramp. Drag the node up or down |
| **Flatten ramp** | Return to one tempo |
| **Beat-match** | Move this clip so its beats sit on the previous clip's; changes the clip's tempo and start |
| Key, Gain | Transpose; gain slider or a curve drawn over the bar |

Beat-match warnings (big stretch, no first downbeat, half-time match) are under **Details** on the message; **Undo** reverts the match. If it needs more stretch than **Warn above**, it shows "Not matched" with the stretch needed. Use **Sync to previous clip**, then **Match anyway**.

## Wrong octave in the mix

Wrong octave on a track: fix its BPM in the library with Halve BPM or Double BPM ([BPM, keys and waveforms](bpm-keys-waveforms.md)).
