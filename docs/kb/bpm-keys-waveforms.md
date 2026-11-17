---
id: bpm-keys-waveforms
title: BPM, keys and waveforms
topic: Analysis and grids
order: 1
keywords: bpm, tempo, bpm range, 92-184, 78-156, half time, double time, half tempo, drum and bass, 174, 87, re-analyse, key, camelot, wheel, key detection, set key, waveform, waveform colours, matrix green, neon, energy, sections
summary: Set the BPM range that decides half or double time, detect and set Camelot keys, choose waveform colours, and read energy and sections.
---

# BPM, keys and waveforms

## BPM range

Every tempo is folded into one window, the BPM range. A tempo outside the window reads at half or double (174 reads 87 in a 78-156 window).

Set it in **Settings, Analysis, BPM range**.

| Range | Result |
|---|---|
| 98-196 | Fast drum and bass and hardcore at full time (180 reads 180) |
| **92-184** (default) | Drum and bass at about 174. 184 and faster reads at half time. 88 to 91 reads double |
| 78-156 | Matches the original Tonium Editor: drum and bass reads about 87 |
| 68-136 | Slow music at its own tempo. 136 and faster reads at half time |

The range applies to tracks analysed afterwards (adding tracks, Re-analyse, the grid editor's Re-analyse, the grid batch). Existing tracks keep their BPM until re-analysed. Use one range for the whole library.

## Fix half or double BPM

1. After a range change, click **Review** on the sidebar's "N read outside the range" note.
2. Re-analyse the listed tracks.

For single tracks, use **Halve BPM** or **Double BPM** in the Inspector or the right-click menu (works on many tracks). The old and new BPM show before anything is written. The same buttons are in [the beat grid editor](grid-editor.md). See also [Troubleshooting](troubleshooting.md).

## Keys and Camelot

Keys show as Camelot codes: a number 1 to 12 and a letter (A minor, B major). The Inspector also shows the musical name (8A is A minor).

Compatible keys: the same code, one number apart (8A with 7A or 9A), or the same number with the other letter (8A with 8B). Find them with "Compatible with" in the Tracks key filter, or the search `key:~8A` ([Search syntax](search-syntax.md)).

### Detect or set a key

- Detect: right-click tracks, **Key: detect**, or use the sidebar button for tracks with no key. [Background analysis](background-analysis.md) also detects keys.
- Set by hand: **Key: set** in the right-click menu, or **Edit** beside the key in the Inspector.
- **Replace existing keys** is off by default.
- **Confidence floor** (per cent) skips uncertain results and lists them afterwards.
- Tracks with no steady pitch get no key ("N tracks have no key to find"). Set those by hand.

## Waveforms

Waveforms show in the Waveform column, the Inspector and the player. Set grid visibility in Settings, Appearance, Beat grid on the player.

Set colours in Settings, Appearance, **Waveform colours**. Mix clips keep their own colours.

| Option | Look |
|---|---|
| **Matrix green, calm** (default) | Green |
| **Neon** | Pink, lilac and bright green |

## Energy and sections

- **Energy** is your own rating, 1 to 10. **Energy (auto)** is estimated from the audio. **Energy source** picks which is used where one number is needed.
- **Sections** marks intro, builds, drops, breakdowns and outro on waveforms. The mix editor can snap to them. For more or fewer sections, change the sensitivity in the Sections panel and analyse again ("Already analysed; nothing to do").
