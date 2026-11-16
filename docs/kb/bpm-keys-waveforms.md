---
id: bpm-keys-waveforms
title: BPM, keys and waveforms
topic: Analysis and grids
order: 1
keywords: bpm, tempo, bpm range, 92-184, 78-156, half time, double time, half tempo, drum and bass, 174, 87, re-analyse, key, camelot, wheel, key detection, set key, waveform, waveform colours, matrix green, neon, energy, sections
summary: How the app reads tempo (and the BPM range setting that decides half or double time), how keys and Camelot codes work, and what the waveform pictures and colours are.
---

# BPM, keys and waveforms

## BPM and the half or double problem

Tempo analysis can land on a number that is half or double the one you would count. A drum and bass track at 174 can read 87, and a slow track at 70 can read 140. Both are "right" for the audio, so the analysis folds every result into one window of tempos, the **BPM range**. A track whose true tempo falls outside the window is shown at half or double.

You choose the window in **Settings, Analysis, BPM range**:

| Range | Use it for |
|---|---|
| 98-196 | Fast drum and bass and hardcore at full time (180 reads as 180). 90 BPM hip-hop then reads as 180 |
| **92-184** (default) | Drum and bass at full time (about 174). House, techno and everything from 92 to 183 keeps its own number. Drum and bass at 184 or faster reads at half time. Music at 88 to 91 BPM reads double, so use 68-136 for that |
| 78-156 | The same as the original Tonium Editor: drum and bass reads at half time (about 87), and 158 reads as 79 |
| 68-136 | Slow music at its own tempo. Anything from 136 up reads at half time (140 reads as 70) |

The new range applies to tracks analysed from now on: adding tracks, Re-analyse, the grid editor's Re-analyse and the grid batch. Tracks already in the library keep their number until you re-analyse them.

### Fixing tracks that read the other way

- After you change the range, or when tracks came from the original Editor, the sidebar shows "N read outside the range" with a **Review** button. The review lists those tracks and offers to re-analyse them with the new range.
- For one track, use **Halve BPM** or **Double BPM** in the Inspector (or the right-click menu, which also works on many tracks). You see the old and new BPM before anything is written. Halving keeps every second beat; doubling adds a beat midway between every two.
- The same buttons are in [the beat grid editor](grid-editor.md).

The Pacemaker syncs tracks by BPM, so tracks the original Editor analysed (drum and bass at about 87) and tracks analysed here (about 174) can disagree on tempo. Pick one range and stay with it. See also [Troubleshooting](troubleshooting.md).

## Keys and Camelot

Keys are shown as **Camelot codes**: a number from 1 to 12 and a letter. A means minor and B means major. The Inspector shows the code and the musical name together (8A is A minor).

Two tracks mix well when their codes are the same, one number apart on the wheel (8A with 7A or 9A), or the same number with the other letter (8A with 8B). Key colours in the table help you see this. The key filter in the Tracks view has "Compatible with" choices, and the search `key:~8A` finds the same set (see [Search syntax](search-syntax.md)).

### Getting a key onto a track

- **Detect keys.** Right-click tracks and choose Key: detect, or use the sidebar button that offers to detect keys for tracks that have none. Background analysis does the same on a library on this computer.
- **Set by hand.** Key: set in the right-click menu, or Edit beside the key in the Inspector.
- **Replace existing keys** is off by default: a key you set yourself is never overwritten. A **confidence floor** (per cent) skips results the detector is unsure about, and lists them afterwards.
- Tracks with no steady pitch (drum-only, silent or atonal tracks) get no key. The sidebar says "N tracks have no key to find", and detecting again finds nothing. Set those by hand.

## Waveforms

The Waveform column and the Inspector draw a small picture of each track. The player shows a bigger one with the beat grid on it. The grid lines stop being drawn when bars get too narrow, which you can change under Settings, Appearance, Beat grid on the player.

### Waveform colours

Settings, Appearance, **Waveform colours**:

- **Matrix green, calm** is the default: the app's own green in three quiet steps, darkest at the outside.
- **Neon** is the pink, lilac and bright green of the original mockup.

The setting applies to the player, the track pictures and the grid editor. The clips in a mix keep their own colours so you can tell them apart.

## Energy and sections

- **Energy** is your own 1 to 10 rating. **Energy (auto)** is an estimate from the audio. Which one counts where a single number is needed is the **Energy source** setting.
- **Sections** finds the intro, builds, drops, breakdowns and outro of each track and marks them on waveforms. The mix editor can snap to them. If a track reads "Already analysed; nothing to do" but you want more or fewer sections, change the sensitivity in the Sections panel and analyse again.

Both are kept by this app and are not written into the library. See [Background analysis](background-analysis.md).
