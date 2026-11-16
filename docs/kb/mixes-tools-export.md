---
id: mixes-tools-export
title: Mix tools and audio export
topic: Mixes
order: 3
keywords: tools, tool, reverb, echo, roll, wah, vocal, crush, filter, eq, trans, loop, reverse, ignore, effects in mix, vocal filter, export, export audio, render, ogg, mp3, flac, wav, loudness, lufs, target, tags, cue sheet, track list, partial export
summary: Add effects, loops and reverse regions to a clip (including the Vocal tool), then render the mix to an audio file with loudness, format and tags.
---

# Mix tools and audio export

## Tools on a clip

A **tool** is an effect, loop or region applied to part of one clip. Select a clip and press **+ Tool** in its panel: the tool is added at the playhead. Tools show as blocks in a thin strip under the mix bar, at the same place as the clip. Drag a block to move it inside its clip and drag its edge to resize it. The tool's curves stretch with it.

| Tool | What it does |
|---|---|
| Reverb | Adds room. Controls: Time and Wet/Dry |
| Echo | Repeating echoes. Controls: Time, Beat division and Wet/Dry |
| Roll | A beat-repeat roll. Controls: Time, Beat division and Wet/Dry |
| Wah | A swept band filter. Control: Cut-off from -100 to +100 (0 is off) |
| **Vocal** | The Revival vocal filter: see below |
| Crush | A bit-crush effect. Control: Crush |
| Filter | A high-pass or low-pass sweep. Control: Cut-off from -100 to +100 |
| EQ | High, Mid and Low |
| Trans | Chops the clip on and off in eighth notes from the start of the block, at a gate tempo you set (0 follows the clip's tempo). It has no curves. Not offered until you tick "Offer the Trans tool when adding a tool" in the mix panel, because its file layout has **not been checked on a real Pacemaker** |
| Loop | Repeats a piece of the clip. You set Repeats, and "repeat again". The Loop out and Loop cut transition recipes make loops like this for you (Crossfades, tempo and Match all) |
| Reverse | Plays the region backwards |
| Ignore | Marks a region to be skipped |

Every effect parameter has a curve. Click the curve to add a node, drag to move it, right-click a node (or select it and press Delete) to remove it. The first and last node stay at the ends of the block. **Reset** returns a curve to its neutral value, and the **muted/on** box switches the tool off without deleting it.

Two tools of the same kind cannot overlap on one clip (saving is refused until you fix it). In the preview and the audio export a loop's joins are softened over about a millisecond and a half so a repeat does not click; the saved file is not changed by that.

### The Vocal tool

Vocal and Wah are the **same tool in the mix file** (Wah with a Cut-off). The mix file has no Vocal tool, so the app remembers which of your Wah tools you made as Vocal: choose **Vocal** from the + Tool menu, or tick "use as the vocal filter" in a Wah tool's panel.

- In the app, mix previews and audio exports play Vocal tools as the vocal filter and Wah tools as Wah.
- On a Pacemaker, the tool plays as the vocal filter **only with Revival firmware 180096073 or later and the Vocal filter switched on** in the Device page (see [The Device page](device-page.md)). Otherwise the Pacemaker plays it as an ordinary Wah.
- The mark is kept in this app, not in the mix. A copy of a Vocal tool is a Wah tool until you mark it. Save as and Duplicate keep the marks.
- The filter works on the centre of the stereo picture, where most vocals sit. Wide or doubled vocals are only partly affected. See [Effects on the Pacemaker](pm-effects.md).

## Exporting the audio

Press **Export audio...** in the mix toolbar. Nothing in your library changes.

### Part of the mix

Export **the whole mix**, **a time range** (From and to), or **from one track to another** (from a track to the end of another). A part starts and ends cleanly: effects start fresh at the cut, so a reverb tail from before the start is not in this part, and a tool already running at the start begins from its start. **Preview this part** plays it as the export will make it.

### Loudness

- **Target loudness**: Off (keep the level the mix has), Streaming (-14 LUFS), Club (-9 LUFS) or Custom (any value from -60 to 0 LUFS).
- **Level the tracks**: no, to the median loudness of the tracks, or to a fixed loudness.
- **Measure loudness** reads the mix and finds the gains the export will use. Nothing is written. A measurement goes out of date if the mix, the range or the settings change.
- The gain is applied before a master limiter, which keeps peaks under -0.2 dBFS. A target the limiter would have to fight for is missed, and the result says by how much. **More settings** has the limits (how far the gain may raise or lower the mix, a true-peak ceiling, the fade at a cut).
- "Play the editor's preview at the export level" makes the live preview match.

### File and tags

| Format | Notes |
|---|---|
| Ogg Vorbis | Small lossy files, what the original Editor wrote. Quality from smallest to best |
| MP3 | Lossy, plays everywhere. Constant or variable bitrate, joint or plain stereo, encoder effort |
| FLAC | Lossless, usually half to two thirds the size of a WAV |
| WAV (16-bit) | Uncompressed |

The dialog shows an estimate of the file size. You choose the file name in the save window, and a name ending in another format's extension uses that format. Tags (title, artist, cover and so on) are written into the exported file only.

Press **Choose file and export...**. The export measures first if loudness is on, then renders. **Cancel export** stops it and keeps no file.

### Track list and CUE sheet

**Track list** gives each track's start time as text, CSV, YouTube chapters or a CUE sheet. The Settings page says where a track's start time falls, how seconds are rounded, how close tracks are joined for YouTube, and whether a CUE sheet is written next to every audio export (Settings, Mixes, Track list and CUE sheet).

## Defaults live in Settings

Tools, tempo matching, audio export and the track list all have settings with defaults under **Settings, Mixes**: see [The Settings page](settings-page.md).
