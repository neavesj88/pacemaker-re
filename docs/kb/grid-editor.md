---
id: grid-editor
title: The beat grid editor
topic: Analysis and grids
order: 2
keywords: beat grid, grid editor, beats, downbeat, nudge, halve, double, set bpm, undo, redo, save, cue point, metronome, tap tempo, drag beats, str file, re-analyse, offset, tempo map, split
summary: Check and correct a track's beat grid: nudge it, halve or double the tempo, set the downbeat and the cue point, undo, and save.
---

# The beat grid editor

The beat grid tells the Pacemaker where every beat of a track falls, so sync and beatmatching lock to the music. The editor (**Beat grid editor** under Tools, or Edit grid on a track) lets you check one track's grid against its waveform and correct it.

The grid is stored in the track's `.str` file, the same file the Pacemaker reads. Nothing is written until you save.

## Opening a track

Choose a track from the list at the top, or press **Edit grid** in the Inspector or the right-click menu. If a track has no usable grid, the editor says so. Use **Re-analyse** to build one from the audio, then **Save**.

## Looking at the grid

The big waveform shows beat lines, with bar lines (downbeats) stronger. Under it a strip shows tempo changes along the track, and an overview shows where you are. The grid card shows the BPM, the offset of the first beat, the number of beats and the tempo map.

- Space plays or pauses. The wheel zooms; Shift and wheel (or dragging) scrolls. **Fit** shows the whole track and **Follow** scrolls with the playhead.
- **Metronome** clicks on every beat while you play, with a higher click on bar lines. There is a nudge control for the click if your output is late or early.

## Correcting the grid

| Tool | What it does |
|---|---|
| Nudge -20, -5, -1, +1, +5, +20 ms | Moves every beat earlier or later |
| beat left and right | Moves the beat lines one whole beat earlier or later |
| Set downbeat | Makes the beat nearest the playhead a bar line |
| Drag beats | Lets you grab single beat lines on the waveform and drag them (hold Alt to move all later beats too) |
| half BPM | Keeps every second beat |
| double BPM | Adds a beat between every two |
| Set BPM | Rebuilds a constant grid at the number you type, from the first beat |
| Re-analyse | Replaces the grid with a fresh analysis (nothing is saved yet) |
| Set cue, Clear cue | Uses the playhead as the track's single cue point |

Further down, the tempo tools handle harder tracks:

- **Split at playhead** cuts the grid into sections that each have their own tempo, for tracks that speed up or slow down. Each section can be rebuilt at a tempo or spaced evenly.
- **Tap tempo** works out a tempo from your taps (press T). It averages the taps and can apply the result to a section.
- **Beat lines** adds a beat at the playhead, removes the selected one or nudges it.
- **Check against the audio** compares the beat lines with the attacks in the audio, lists runs where the grid drifts, and can move the lines onto the audio there.

## Undo and save

- Ctrl+Z undoes and Ctrl+Shift+Z redoes (Ctrl+Y works too).
- **Save...** shows what will change: the new grid (beats, BPM, how far the first beat moved) and the cue point. Confirm to write it.
- If mixes use the track, the dialog lists them. A mix clip keeps the tempo the track had when the mix was made, so those mixes may drift and need beatmatching again. You tick a box to save anyway.
- The cue point is written as a position in sample frames. The unit is **not verified against a real Pacemaker**, so the dialog asks you to tick Write the cue anyway.
- The previous grid cannot be restored from the editor. A copy of the database is made before the first write of each session: see [The Device page](device-page.md).

Leaving the editor with unsaved changes asks whether to keep editing or discard them.

## Many tracks at once

Right-click tracks and choose **Beat grid tools (preview)** to see suggested fixes for many tracks, each with its old and new BPM, before anything is written. The **Suspect** filter in [the Tracks view](tracks-view.md) finds tracks whose grid looks wrong for their genre. For the half and double rule, see [BPM, keys and waveforms](bpm-keys-waveforms.md).
