---
id: mixes-tools-export
title: Mix tools and audio export
topic: Mixes
order: 3
keywords: tools, tool, reverb, echo, roll, wah, vocal, crush, filter, eq, trans, loop, reverse, ignore, effects in mix, vocal filter, export, export audio, render, ogg, mp3, flac, wav, loudness, lufs, target, tags, cue sheet, track list, partial export
summary: Add effects, loops and reverse regions to a clip, then export the mix as an audio file.
---

# Mix tools and audio export

## Add a tool to a clip

1. Select a clip.
2. Press **+ Tool** in the clip panel. The tool is added at the playhead.
3. Drag the block under the mix bar to move it. Drag its edge to resize it.

## Tool types

| Tool | Controls |
|---|---|
| Reverb | Time, Wet/Dry |
| Echo | Time, Beat division, Wet/Dry |
| Roll | Time, Beat division, Wet/Dry |
| Wah | Cut-off -100 to +100 (0 is off) |
| Vocal | Vocal filter. See Vocal tool below |
| Crush | Crush |
| Filter | High-pass or low-pass sweep. Cut-off -100 to +100 |
| EQ | High, Mid, Low |
| Trans | Gate tempo (0 follows the clip tempo). No curves. Tick "Offer the Trans tool when adding a tool" in the mix panel to show it |
| Loop | Repeats, "repeat again". Created by the Loop out and Loop cut transitions |
| Reverse | Plays the region backwards |
| Ignore | Skips the region |

## Edit curves

- Click a curve to add a node. Drag a node to move it.
- Right-click a node, or select it and press Delete, to remove it.
- **Reset** returns a curve to neutral.
- The **muted/on** box switches a tool off without deleting it.

Two tools of the same kind cannot overlap on one clip.

## Vocal tool

Choose **Vocal** from the + Tool menu, or tick "use as the vocal filter" in a Wah tool panel.

Plays as the vocal filter on a Pacemaker only with firmware 180096073 or later and the Vocal filter switched on in the Device page ([The Device page](device-page.md)). Otherwise it plays as Wah. See [Effects on the Pacemaker](pm-effects.md).

A copied Vocal tool becomes Wah until you tick "use as the vocal filter". Save as and Duplicate keep it.

## Export audio

Press **Export audio...** in the mix toolbar.

1. Choose the range: whole mix, a time range (From, to), or from one track to another. **Preview this part** plays it.
2. Set loudness and format (below).
3. Press **Choose file and export...** and name the file.

**Cancel export** stops it and keeps no file.

### Loudness

| Setting | Options |
|---|---|
| Target loudness | Off, Streaming (-14 LUFS), Club (-9 LUFS), Custom (-60 to 0 LUFS) |
| Level the tracks | No, to the median loudness, or to a fixed loudness |

- **Measure loudness** previews the gains without writing a file.
- **More settings**: gain limits, true-peak ceiling, fade at a cut.
- "Play the editor's preview at the export level" matches the live preview to the export.

### Format

| Format | Notes |
|---|---|
| Ogg Vorbis | Small lossy files. Quality setting |
| MP3 | Constant or variable bitrate, joint or plain stereo, encoder effort |
| FLAC | Lossless |
| WAV (16-bit) | Uncompressed |

The dialog shows the estimated file size. Tags (title, artist, cover) go into the exported file.

## Track list and CUE sheet

**Track list** gives each track's start time as text, CSV, YouTube chapters or a CUE sheet. Options are in Settings, Mixes, Track list and CUE sheet. Other tool and export defaults are under **Settings, Mixes** ([The Settings page](settings-page.md)).
