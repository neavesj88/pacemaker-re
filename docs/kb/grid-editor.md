---
id: grid-editor
title: The beat grid editor
topic: Analysis and grids
order: 2
keywords: beat grid, grid editor, beats, downbeat, nudge, halve, double, set bpm, undo, redo, save, cue point, metronome, tap tempo, drag beats, str file, re-analyse, offset, tempo map, split
summary: Check and correct a track's beat grid by nudging beats, halving or doubling the tempo, setting the downbeat and cue point, then saving.
---

# The beat grid editor

The beat grid marks every beat of a track. Nothing is written until you save.

## Open a track

- Pick a track from the list at the top, or
- Click **Edit grid** in the Inspector or right-click menu, or open **Beat grid editor** under Tools.

If a track has no usable grid, click **Re-analyse**, then **Save**.

## Read the grid

The waveform shows beat lines and stronger bar lines (downbeats).

- Space plays or pauses.
- Wheel zooms. Shift+wheel or dragging scrolls.
- **Fit** shows the whole track.
- **Follow** scrolls with the playhead (Settings > Mixes > Following the playhead). Scrolling or zooming switches it off. Tick it again to resume.
- **Metronome** clicks every beat, higher on bar lines. Its nudge control offsets the click.

## Correct the grid

| Tool | Action |
|---|---|
| Nudge -20, -5, -1, +1, +5, +20 ms | Moves every beat earlier or later |
| beat left and right | Moves the beat lines one whole beat |
| Set downbeat | Makes the beat nearest the playhead a bar line |
| Drag beats | Drag single beat lines. Hold Alt to move all later beats too |
| half BPM | Keeps every second beat |
| double BPM | Adds a beat between every two |
| Set BPM | Rebuilds a constant grid at the typed number, from the first beat |
| Re-analyse | Replaces the grid with a fresh analysis (not saved yet) |
| Set cue, Clear cue | Uses the playhead as the track's single cue point |

## Tempo tools

- **Split at playhead** cuts the grid into sections with their own tempo, for tracks that speed up or slow down. Each section can be rebuilt at a tempo or spaced evenly.
- **Tap tempo** (press T) averages your taps and can apply the result to a section.
- **Beat lines** adds a beat at the playhead, removes the selected one or nudges it.
- **Check against the audio** lists runs where the grid drifts and can move the lines onto the audio.

## Undo and save

- Ctrl+Z undoes. Ctrl+Shift+Z or Ctrl+Y redoes.
- **Save...** lists the changes (new grid, cue point). Confirm to write. Tick Write the cue anyway to save a cue point.
- If mixes use the track, the dialog lists them. Those mixes may need beatmatching again. Tick the box to save anyway.
- Saving cannot be undone from the editor. A database copy is made before the first write of each session ([The Device page](device-page.md)).
- Leaving with unsaved changes asks whether to keep editing or discard.

## Fix many tracks

Right-click tracks and choose **Beat grid tools (preview)** to see suggested fixes with old and new BPM before anything is written. The **Suspect** filter in [the Tracks view](tracks-view.md) finds tracks whose grid looks wrong for their genre. For half and double tempo, see [BPM, keys and waveforms](bpm-keys-waveforms.md).
