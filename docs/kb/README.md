# Pacemaker Revival knowledge base

These are the help articles of Pacemaker Revival, written for a DJ rather than a developer. They are plain Markdown, so you can read them here on GitHub. The app **bundles these very files** at build time and shows them in its **Help** view (press F1), where they are searchable offline. There is one copy: edit the article here and the app picks it up on the next build.

If you are new, start with [Getting started](getting-started.md). If something is wrong, try [Troubleshooting and FAQ](troubleshooting.md).

Articles about hardware and the Pacemaker itself say how sure each statement is: **verified** (read from the manual or the firmware, or tested on a unit), **likely**, or **unverified**.

## Start here

- [Getting started](getting-started.md): The two kinds of library, what the Welcome screen offers, how to add tracks and what the app does with them in the background.
- [Using the Help](using-help.md): How to search the help, the keyboard shortcuts, and where the same articles can be read on GitHub.
- [The Settings page](settings-page.md): One page for every setting of the app, grouped by area, with its own search that also finds help articles.

## Tracks

- [The Tracks view](tracks-view.md): The table of every track in the open library, its columns, filters, sorting, saved filters and the right-click menu.
- [Search syntax](search-syntax.md): Everything the search box understands, with an example for each: words, phrases, fields, ranges, comparisons, keys, tags, dates, flags, not and or.
- [The Inspector, tags, ratings and colours](track-inspector-labels.md): The panel on the right of the Tracks view, and how to tag, colour, rate and set the energy of tracks. Tags, colours and energy stay in this app.

## Analysis and grids

- [BPM, keys and waveforms](bpm-keys-waveforms.md): How the app reads tempo (and the BPM range setting that decides half or double time), how keys and Camelot codes work, and what the waveform pictures and colours are.
- [The beat grid editor](grid-editor.md): Check and correct a track's beat grid: nudge it, halve or double the tempo, set the downbeat and the cue point, undo, and save.
- [Background analysis](background-analysis.md): What the app analyses by itself, when it pauses, why a key can be held back, and what the couldn't-analyse list means.

## Cases

- [Cases and smart cases](cases.md): Cases are the Pacemaker's playlists. Make them by hand, or let a smart case fill itself from rules.

## Mixes

- [Building a mix](mixes-building.md): Make a mix from the track table under the timeline, move and trim clips, play it live, and save it.
- [Crossfades, tempo and Match all](mixes-crossfades-tempo.md): How crossfades work, the six transition recipes, beat-matching, matching every track to the mix tempo (half and double time aware), and fine BPM steps.
- [Mix tools and audio export](mixes-tools-export.md): Add effects, loops and reverse regions to a clip (including the Vocal tool), then render the mix to an audio file with loudness, format and tags.

## Library tools

- [Set builder](set-builder.md): Puts tracks in a good running order by key, tempo and (optionally) an energy curve, then saves the result as a case or lays it out as a mix.
- [History and the play log](history.md): What was played and when, built from the Pacemaker's play counters, with tabs for sessions, most played, never played, cases and mixes, and the play log.
- [Duplicates](duplicates.md): Finds tracks that are the same recording, shows how the copies differ and merges them, with removing and deleting files as separate choices.
- [Library gap map](gap-map.md): A picture of how your library spreads over tempo, key, genre and energy, with the thin spots called out.
- [Import and export](import-export.md): Bring tracks and playlists in from files and other DJ software, watch folders for new music, and send cases, mixes and the library out as lists.
- [Request list matcher](request-matcher.md): Paste or open a list of wanted tracks and see which you already own, which to check and which are missing.
- [Stems (vocals and instrumentals)](stems.md): Make vocal and instrumental versions of tracks on this computer, saved in a folder you choose.
- [Tidy metadata](tidy-metadata.md): Fix titles, artists, genres and other text across many tracks with recipes, preview every change, and optionally write tags back into the audio files.
- [Free space](free-space.md): Plans which tracks to archive or remove so the music fits on a Pacemaker. Only offered for a Pacemaker.

## Libraries and sync

- [Sync](sync.md): Copy tracks, mixes and cases from one library to the other, either way, after seeing exactly what would change.
- [Libraries (two panes)](libraries.md): See two libraries side by side and drag tracks, mixes or cases from one to the other. A drag only ever copies.

## Device page

- [The Device page](device-page.md): Every card on the Device page: storage, library health, firmware, vocal filter, backup, restore, undo, safety and the Pacemaker's own settings, and how to eject.

## Firmware and hardware

- [Installing firmware with the app](install-firmware.md): Step by step with the Firmware card on the Device page. The card in a PC reader is the reliable way, the Pacemaker boots with USB unplugged, and a new firmware number resets the settings.
- [Upgrading the storage to a microSD card](storage-upgrade.md): Replace the Pacemaker's 1.8-inch hard drive with a microSD card on a ZIF adapter. What to buy (in general terms), what we used, what works, what to avoid, and why firmware decides how much of the card you can use.
- [Other ways to upgrade the storage](storage-alternatives.md): A comparison of the ways to replace the 1.8-inch hard drive (microSD adapters, iFlash boards, CompactFlash, mSATA, a ZIF SSD, another hard drive), with what is verified and what is only reported.
- [Ways to install firmware](firmware-methods.md): Five ways to get a firmware file onto the Pacemaker: the app, copying by hand, the restore-library script, the Pacemaker's own USB, and DFU recovery. Each as short steps, with what is verified.
- [Replacing the battery](battery-replacement.md): What people have fitted in the Pacemaker, what is and is not known about the battery and its connector, and how to measure the old cell and choose a replacement without guessing. Li-ion safety first.

## Using the Pacemaker

- [Controls tour](pm-controls-tour.md): What every control on the Pacemaker does: the display, the touchpad, the P-switch, the crossfader and the buttons. Start here for the player's guide.
- [Power and charging](pm-power-charging.md): How to switch the Pacemaker on and off, charge it from USB or the adapter, and how long a charge lasts.
- [Browsing and loading tracks](pm-browsing-loading.md): Open the track list, move through it, filter by genre, BPM and more, and load a track onto a channel.
- [Mixing and beatmatching](pm-mixing.md): How to cue a track in your headphones, match its tempo by hand or with one press, blend with the crossfader and use the EQ.
- [Effects, EQ and the Vocal filter](pm-effects.md): The effects the Pacemaker has, how to use them from the touchpad, how to change which effect is on which pad slot, and the Revival Vocal filter.
- [Cues, loops and reverse](pm-cues-loops.md): Setting and using a cue point, making and adjusting a loop, and playing in reverse on the Pacemaker.
- [Recording mixes and making cases](pm-recording.md): How to record a live mix on the Pacemaker, what Halt does, and how to make and edit cases (playlists) on the unit.
- [The settings menu](pm-settings-menu.md): One table of every setting in the Pacemaker's settings menu, with its options and default, and how to change them from the app.
- [Connecting to a computer](pm-computer-connection.md): How the Pacemaker looks to a computer, how to connect and disconnect safely, and what the original Editor and this app do with it.
- [Button combinations and recovery](pm-button-combos-recovery.md): The key combinations held at power-on (DFU, debug menu, diagnostics), what each one does, and which can wipe your library.

## Problems and answers

- [Troubleshooting and FAQ](troubleshooting.md): Fixes for the common problems: the Pacemaker not seen, firmware not installing, "hard drive needs checking", slow loading on an SD card, a half-tempo BPM, and missing keys on drum tracks.

## Writing an article

Each article is one file in this folder. The file name is the article's id plus `.md`, and it starts with a front matter block:

```
---
id: the-file-name-without-md
title: The title shown in the app
topic: one of the topics above
order: 1
keywords: words, people, might, search, for
summary: One sentence for the list and the search results.
---
```

Then `# Title` and the text. Link to another article with a normal relative link such as `[text](storage-upgrade.md)` or `[text](storage-upgrade.md#what-works)`: it works on GitHub and in the app. Add every new article to this index. The tests (`npm test` in `app`) check the front matter, the links and this index.

Keep names, serial numbers and personal library details out of the articles: this repository is public.
