---
id: background-analysis
title: Background analysis
topic: Analysis and grids
order: 3
keywords: background analysis, waveforms, keys, energy, sections, pause, resume, playback, held back, confidence floor, could not be analysed, failed, try again, hide, run now, status strip, progress
summary: Fills in missing waveforms, keys, energy and sections for every track in the open library while you work, with pause rules and a retry list.
---

# Background analysis

With a library open, the app analyses missing items for every track in the background, in any view. It resumes after a restart.

## What runs

| Kind | Result |
|---|---|
| Waveforms | Pictures in the table, Inspector and player. Runs first |
| Keys | Key, only for tracks with none. A key you set is never replaced |
| Energy | Energy (auto) estimate, 1 to 10 |
| Sections | Intro, builds, drops, breakdowns, outro. Needs the beat grid |

Beat grid and BPM are made when a track is added: see [Getting started](getting-started.md).

## Analyse a Pacemaker

A Pacemaker is analysed only on request. In the settings, click **Run now on this Pacemaker**. Keys are written to the Pacemaker's library.

## Status strip

A strip under the views shows the current task, the track, a progress bar and a **Pause** or **Resume** button. Pause stays until you press Resume.

## Pause rules

Analysis waits while:

- A track or mix preview plays. Setting: Settings, Analysis, Background analysis, Wait while a track or mix preview plays.
- A Sync plan is on screen. Also a setting.

## Processor use

- **Tracks analysed at once**: set separately for energy and sections.
- **Analyse at low priority**: on by default on four processors or fewer. Covers background analysis and analysis you start.
- **When the computer is on battery**: keep going (default), slow down or wait. Wait applies to background analysis only.

## Held back keys

The background **confidence floor** (per cent, 0 to 100) blocks keys found below it. Those tracks are listed as "a key held back by the confidence floor" and are checked again when you lower the floor. 0 writes every result. Manual key detection has its own floor.

## Couldn't-analyse list

Files that fail (damaged, unreadable, too short) are listed under **Couldn't be analysed** in the settings, with the reason. Click **Try these again** to retry.

The strip also shows a note with:

- **Show the list** and **Hide the list**: unfold or fold the failures.
- **Try again**: retry them.
- **Hide**: dismiss the note until new failures appear.

## Settings

Under **Settings, Analysis, Background analysis**:

- Switch background analysis, or each kind, on or off.
- Set the pause rules and the confidence floor.
- **Check this library now** looks for missing work immediately.

Energy scaling and section sensitivity are in [The Settings page](settings-page.md).
