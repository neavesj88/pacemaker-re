---
id: background-analysis
title: Background analysis
topic: Analysis and grids
order: 3
keywords: background analysis, waveforms, keys, energy, sections, pause, resume, playback, held back, confidence floor, could not be analysed, failed, try again, hide, run now, status strip, progress
summary: What the app analyses by itself, when it pauses, why a key can be held back, and what the couldn't-analyse list means.
---

# Background analysis

When a library on this computer is open, the app quietly fills in what is missing for every track while you do something else, in any view. It carries on after a restart.

## What runs

| Kind | What it makes | Where it is kept |
|---|---|---|
| Waveforms | The pictures in the table, the Inspector and the player. Done first because they are the quickest | This app |
| Keys | The key, only for tracks that have none. A key you set is never replaced | Written to the library's key column |
| Energy | The Energy (auto) estimate from 1 to 10 | This app, not the library |
| Sections | Intro, builds, drops, breakdowns, outro. Needs the track's beat grid | This app, not the library |

The beat grid and BPM are made when a track is added, not in the background: see [Getting started](getting-started.md).

A Pacemaker is **never** analysed by itself. For a Pacemaker, press **Run now on this Pacemaker** in the settings and it reads the audio of tracks that lack a result. Waveforms, energy and sections are kept on this computer. Keys it finds are written to the Pacemaker's library. Nothing runs on the unit until you press it.

## The status strip

A thin strip under the views shows what is happening: a line of text, the track being worked on, a progress bar and a **Pause** or **Resume** button. It never takes focus and does not block anything.

## When it pauses

- **While a track or mix preview plays.** Analysis competes with playback for the processor, so it waits and carries on when playback stops. Turn this off under Settings, Analysis, Background analysis, Wait while a track or mix preview plays.
- **While a Sync plan is on screen.** A key written now would change a row the plan was made from, and Sync would refuse the plan as out of date. It carries on when the plan is applied or cleared. This is also a setting.
- When you press Pause. It stays paused until you press Resume.

## How much of the computer it uses

The defaults follow the computer. How many tracks are analysed at once depends on the number of processors and the free memory: on a computer with two processors, two tracks at a time, otherwise one fewer than the processors, up to a limit. The same count is used for Add tracks, the beat grid batch and the duplicate check. Energy and sections have their own **Tracks analysed at once** setting if you want to pin it.

On a computer with four processors or fewer, **Analyse at low priority** is on by default, and it applies to analysis you start yourself (Add tracks, energy, sections, duplicates, grids) as well as the background worker. That keeps the app responsive on a small machine; results are the same, only slower while the computer is busy.

**When the computer is on battery** can keep going (the default), slow down or wait. Slowing down puts both the background worker and any analysis you start while on battery at low priority. Waiting applies to the background worker only; analysis you start yourself runs anyway.

## Held back keys

A detected key comes with a confidence score. Background analysis has its own **confidence floor** (per cent, 0 to 100). A key found below it is **not written**. The track is listed as "a key held back by the confidence floor", and it is looked at again as soon as you lower the floor. 0 writes every result. The floor for the manual Key detection has its own, separate setting.

## The couldn't-analyse list

A file that fails (damaged, unreadable or too short to analyse) is listed under **Couldn't be analysed** in the settings, with the kind of analysis and the reason. It is not tried again until the file changes or you ask. Press **Try these again** to retry them.

The strip also shows a short note when some tracks failed, with:

- **Show the list** and **Hide the list** to see or fold the failures.
- **Try again** to retry them.
- **Hide** to dismiss the note. It comes back only if new failures appear.

## Settings

Under **Settings, Analysis, Background analysis** you can switch the whole thing off, switch each kind on or off, and set the pause rules and the confidence floor. **Check this library now** looks for missing work straight away. Energy and sections have their own settings (how the estimate is scaled, how sensitive section detection is): see [The Settings page](settings-page.md).
