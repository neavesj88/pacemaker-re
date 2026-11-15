---
id: stems
title: Stems (vocals and instrumentals)
topic: Library tools
order: 7
keywords: stems, vocal, acapella, instrumental, separation, ai, download model, onnx, licence, engine
summary: Make vocal and instrumental versions of tracks on this computer, saved in a folder you choose.
---

# Stems

**Stems** (under Tools) splits tracks into a vocal (acapella) file and an instrumental file. It runs on this computer and no audio leaves it.

## First-time setup

The separation engine and its model are downloaded once, and only after you read and accept the licence notice of each part. Each download is checked against a fixed checksum and thrown away if it does not match. You can remove a downloaded part again from the same card. If a part's licence status is unclear, it is marked "licence unclear" so you can decide.

## Making stems

1. Choose tracks: tick them in the list, press **Choose N shown**, or use **the selected tracks in Tracks**.
2. Choose a folder on this computer to save the files in. A folder inside the library or the Pacemaker is refused: putting the results into the library is a separate, confirmed step afterwards.
3. Review the plan: it lists every file that will be made and every file that would be replaced.
4. Start. A progress bar shows the time left. The Pacemaker is only read for its source audio.

Settings (Settings, Stems) sets what the engine runs on, the output format, the overlap, the suffixes added to the file names (for example "acapella" and "instrumental"), whether existing files are skipped or replaced (each one listed and confirmed first), and where the model and the engine library live.

## The result

When it is done, a result card lists what was made, anything that failed or was skipped with the reason, and notes if samples were limited to full scale to avoid clipping. You can then add the files to the library. You are shown every file and asked before anything is copied. The library does not keep tags or a link to the original; they appear as ordinary tracks.

If you want a vocal filter while mixing rather than separate files, see [Mix tools and audio export](mixes-tools-export.md).
