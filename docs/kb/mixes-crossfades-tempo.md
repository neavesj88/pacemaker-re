---
id: mixes-crossfades-tempo
title: Crossfades, tempo and Match all
topic: Mixes
order: 2
keywords: crossfade, transition, fade, curve, bass swap, long blend, echo out, filter sweep, drop swap, hard cut, loop out, loop cut, loop, halving, stutter, recipe, auto transition, beat-match, mix tempo, match all, sync, half time, double time, fine bpm, pitch, stretch, tempo ramp, plays at
summary: How crossfades work, the eight transition recipes, beat-matching, matching every track to the mix tempo (half and double time aware), and fine BPM steps.
---

# Crossfades, tempo and Match all

## Crossfades

Where two clips overlap on the bar, the overlap is a **crossfade**. Click it to see its panel.

- **Curve.** Presets are Linear, Fast in, Slow in, Equal power and Cut. You can also draw your own: click to add a node, drag to move it.
- **Fade after.** Whether the outgoing clip fades out after half or three quarters of the overlap.
- **Mute** switches the crossfade off.
- A crossfade only plays, draws and exports where **both clips are heard**: it starts no earlier than the incoming clip and ends no later than the end of the outgoing clip (trims included). If a stored fade is longer (you trimmed or moved a clip after making it), the bar shows "cut to overlap" and the panel offers **Fit to overlap** to save the shorter length. The file is never changed unless you do that.
- **Beat-match clips** matches the later clip's tempo and beats to the clip before it.

### Transition recipes

A recipe builds the crossfade and the tools around it in one go. Choose it in the crossfade panel and press **Rebuild as ...**, or select a clip and press **Auto transition** to transition into the next one. The panel lists exactly what will be made before and after.

| Recipe | What it does | Lengths in bars |
|---|---|---|
| Bass swap | Linear crossfade. The incoming bass stays out until the middle, then the two basses trade places on one beat | 1 to 32 (16 default) |
| Long blend | A slow S-shaped crossfade with the basses traded gradually through the middle, so the tracks run together for a long time | 8 to 64 (32 default) |
| Echo out | The outgoing track dissolves into its own echoes while the incoming track comes in over the second half | 1 to 8 (4 default) |
| Filter sweep | The outgoing track is thinned out with a rising high-pass while the incoming track opens up from a low-pass | 2 to 32 (8 default) |
| Drop swap | The outgoing track builds (bass out, rising high-pass, a beat-repeat on the last bar) and the incoming track hits at full on the last beat | 2 to 16 (8 default) |
| Hard cut | A straight cut with no blend. The cut falls at the end of the length (a phrase is 8 bars), or at the start | one beat to 8 (8 default) |
| Loop out | The outgoing track loops a short slice on its beat grid while the incoming track comes in over it; the basses trade places in the middle | 2 to 16 (8 default) |
| Loop cut | The outgoing track loops a bar, then half a bar, then half again, down to a quarter of a beat, and the incoming track drops in at the end | 2 to 16 (8 default) |

### Loop out and Loop cut

Both make ordinary Loop regions on the outgoing clip (the same Loop tool as in the tools strip, one block in the lane for a loop cut's run), so you can drag, resize or delete them like any loop you made. Every loop starts on a beat of the outgoing track's own grid, so a track with a drifting live tempo still loops on its beats. A track with no beat grid is looped by its tempo and the toast says so.

**Loop out** loops one slice from the start (or middle) of the overlap until the crossfade has finished with the outgoing track, with a bass swap at the middle. Settings, Mixes, Auto transitions:

- **Loop out: loop length** (1 beat to 8 bars, 2 bars by default). A loop that cannot repeat twice in the overlap is made shorter, and the toast says so.
- **Loop out: starts** at the start of the overlap (default) or in the middle of it.

**Loop cut** is the classic build into a drop. The loops follow one another, each half as long as the one before, and the last one ends where the incoming track comes in at full volume. The incoming track is silent until then. Settings:

- **Loop cut: first loop** (1 beat to 8 bars, 1 bar by default) and **shortest loop** (1/8 to 1 beat, 1/4 beat by default). Choose 4 bars for the long version: 4 bars, 2 bars, 1 bar, then beats.
- **Loop cut: times each length** (1 to 4, default 2) is how often each length plays.
- **Loop cut: rising filter** (off by default) adds a high-pass that rises over the loops.

If the loops do not all fit in the overlap the longest are left out (a 4-bar overlap fits the default run of 15.5 beats only if the track's beats line up; use 8 bars or more for the full run). Each loop repeats the audio at its own start, so a loop cut steps forward through the track on each halving rather than looping the same bar smaller and smaller: the file format keeps one loop point per loop, and a re-loop chain always keeps the first slice.

The loops are written to the mix file as plain Loop regions, so the Pacemaker plays whatever its Loop does. **Not yet heard on a Pacemaker:** nobody has played a mix with several consecutive loops on the device. The preview and the audio export soften each loop's join over about a millisecond and a half so it does not click; the device may not.

When the incoming clip moves, the loops stay on the outgoing clip's beats and move, in whole beats, to keep ending with the crossfade; when the outgoing clip moves they go with it. Rebuilding a loop recipe removes the old run first.

**Auto crossfade length** (Settings, Mixes, Auto transitions; 16 bars by default) is how long an automatic crossfade is. It is never longer than the audio both clips share and is a whole number of bars. **Crossfade sits** says where it goes: by default it ends with the outgoing clip, starting on a bar line of the incoming clip when it has a beat grid; the other choice starts it on the incoming clip's first beat.

If your own tools are in the way, what happens to them is set in Settings, Mixes, Auto transitions. A recipe needs the two clips to overlap by at least its shortest length; the app tells you if they do not.

### When you move a clip

By default a crossfade and the tools its transition placed follow the clips they sit on. Move the incoming clip earlier and the fade, the outgoing clip's EQ and the incoming clip's EQ all re-fit the new overlap, instead of being left behind. The rule depends on how the crossfade was placed:

- **It covered the whole overlap** (the usual result of Auto when the clips overlap by about the Auto length): it covers the new overlap, a bigger one or a smaller one.
- **It ended with the outgoing clip** (Auto in a longer overlap): it keeps its length in bars and keeps ending there. If the overlap becomes shorter than the fade, the fade shrinks to fit.
- **It began where the incoming clip begins**: it keeps beginning there, with the same length.
- **You placed it by hand** somewhere inside the overlap: it stays where it is. Its tools are still pinned under it.

The tools that follow are the ones on the two clips that lie inside the crossfade and touch its start or its end, which is what every recipe makes (the run of loops of a Loop out or Loop cut is moved whole, never stretched, because a loop's length is whole beats). A tool you added in the middle of a fade, or outside it, simply moves with its clip. When the crossfade gets longer or shorter the tools are stretched in proportion, so a bass swap keeps its place relative to the fade; **Rebuild as ...** snaps it back onto a beat.

If a move leaves the clips with no overlap the crossfade is kept as it was and does nothing, and it comes back if you drag the clip back or press Undo. Dropping a clip onto another does not create a crossfade. A drag is one Undo step.

Settings, Mixes, Auto transitions, **When a clip moves**, can switch this off: the crossfade then stays where it was made and is cut to the shared audio when played, as before.

## The mix tempo

The mix has one **mix tempo** box in the toolbar. New tracks and the Match buttons play at it, or at double or half of it, whichever stretches the track least. It is **not saved in the file**: a reopened mix starts at its first track's tempo.

## Match all

**Match all** plays every track at the mix tempo. It is half and double aware: a 70 BPM track in a 140 BPM mix plays at 70, not stretched to 140. When it finishes a short message says how many tracks were matched (for example "Matched 5 tracks to 128 BPM; 2 kept"), with **Details** and **Undo** on it. Details lists:

- how many tracks were matched, and how many already played at the tempo,
- which tracks it left alone because they have a **tempo ramp** (use Sync on the clip to rescale those),
- which tracks it left at their own tempo because matching would stretch them more than the allowed amount.

The allowed amount is **Warn above** in Settings, Mixes, Tempo matching (a percentage). A track over the limit gets a stretch badge, and its panel offers **Keep its own tempo** or match anyway. Other settings there: whether tracks are matched when added, and what a new track does when it would need more stretch than allowed.

## One clip's tempo and fine BPM

Select a clip. Its panel has:

- **Track BPM**: the tempo of the track itself.
- **Plays at**: the tempo it plays at in the mix. Type a number to change it.
- **Octave**: **half time** and **double time** buttons for when the automatic match chose the wrong octave.
- **Pitch / speed**: fine steps of -0.1, -0.01, +0.01 and +0.1 percent, or type a percentage. This is the fine BPM control for lining beats up by ear.
- **Sync to mix tempo** and **Sync to previous clip** match this clip to one or the other (half or double included).
- **Add ramp node at playhead** makes a tempo ramp: drag the node up or down and the tempo changes linearly between nodes. **Flatten ramp** goes back to one tempo.
- **Beat-match** moves this clip so its beats sit on the previous clip's. It changes the clip's tempo and start. When matching would stretch the track more than **Warn above**, nothing changes and the message says "Not matched" with the stretch it would need; use **Sync to previous clip**, then **Match anyway**, to do it regardless. Any warning (a big stretch, no first downbeat, a half-time match) is under **Details** on the message, and **Undo** on the message takes the match back.

Whether the pitch stays or follows the speed is the clip's tempo mode: by default the audio is time-stretched and the pitch stays.

The clip panel also has Key (transpose) and Gain (a slider, or a curve drawn over the bar).

## If the tempo of a track looks wrong

A track that reads at the wrong octave in the mix probably has the wrong BPM in the library. Fix it once in the library with Halve BPM or Double BPM: see [BPM, keys and waveforms](bpm-keys-waveforms.md).
