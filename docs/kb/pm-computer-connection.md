---
id: pm-computer-connection
title: Connecting to a computer
topic: Using the Pacemaker
order: 9
keywords: usb, connect, computer, mass storage, drive, charging, data connection, eject, do not disconnect, pacemaker revival, editor, tonium editor, transfer, copy tracks, music.db, hidden folder, sync, export mix, windows
summary: Connect the Pacemaker to a computer over USB, manage tracks, cases and mixes with Pacemaker Revival, export mixes and eject safely.
---

# Connecting to a computer

On a Revival setup the computer software is Pacemaker Revival, this app. It replaces the original Pacemaker Editor, which is described at the end.

## Connect

The Pacemaker appears as a USB mass storage drive. Do not rename or move files in its hidden `.Pacemaker` folder.

1. Power on the Pacemaker and connect the USB cable to the computer. Wait until the drive appears in Explorer or Finder.
2. If the Pacemaker is only charging (it is off, shows the charging screen and the **Play** button blinks), press the settings button to turn on the data connection.
3. In the app press **Rescan**, then open the Pacemaker from the library picker: see [Getting started](getting-started.md).

## Install the computer software

1. Install Pacemaker Revival on the computer.
2. Connect the Pacemaker as above.
3. Open it from the library picker.

The original Editor installers sit on the Pacemaker's drive. You do not need them with Revival. To install the original Editor instead, see the last chapter of this article.

## Copy music, cases and mixes

| Task | How |
|---|---|
| Copy tracks, cases and mixes in either direction | [Sync](sync.md): choose From and To, check, then sync |
| Copy a few items by dragging | [Libraries](libraries.md) |
| Remove items from the Pacemaker | Turn on **Removals** in [Sync](sync.md), or review unused files on the [Device page](device-page.md) |
| Build and edit a mix | [Building a mix](mixes-building.md) |
| Keep the Pacemaker's playlists in step | [Cases and smart cases](cases.md) |

A mix sent to the Pacemaker appears there as a track list, and each track's start becomes a cue point.

## Export a mix as an audio file

1. Open the mix in the mix editor.
2. Press **Export audio...**, choose range, loudness and format, then **Choose file and export...**. See [Mix tools and audio export](mixes-tools-export.md).
3. To play the file on the Pacemaker, add it to the library as a track and copy it over: see [Import and export](import-export.md). The Pacemaker treats it as an ordinary track.

## Disconnect safely

1. Press **Eject** on the [Device page](device-page.md), or use the Windows eject.
2. Wait for "Do not disconnect!" to clear on the Pacemaker's screen.
3. Unplug the cable.

> Never unplug during a transfer: it can corrupt the library.

## Install firmware

Copy the firmware file into the `.Pacemaker` folder. It installs at the next start: see [Installing firmware with the app](install-firmware.md). Do not start the Pacemaker with USB connected while an update is waiting.

## Large transfers

For many gigabytes, use a PC card reader on the removable storage: see [Ways to install firmware](firmware-methods.md).

## Use the original Editor instead

The original Editor manages tracks, cases and mixes, exports mixes to audio files and builds mixes stand-alone. To install it:

1. Power on the Pacemaker and connect it over USB. Wait for the drive to appear.
2. Open the Pacemaker drive in Finder or Explorer.
3. Find the installer for your operating system and run it, following its prompts.

It does the same jobs as Revival for tracks, cases and mixes with these differences.

| Task | In the Editor |
|---|---|
| Send tracks, cases or mixes to the Pacemaker | Select them and drag them into the Pacemaker pane |
| Remove them from the Pacemaker | Select them in the Pacemaker pane and press Delete |
| Bring cases and mixes back | Drag them from the Pacemaker pane to the Editor library |
| Bring tracks back | Not possible |
| Export a mix | Right-click the mix and choose Export, then pick a location and name |
| Eject | The Eject button to the left of the Pacemaker library |
| Publish a mix online, automatic upgrades | Used the manufacturer's servers, which no longer work |

Add an exported mix to the Editor library to drag it over as a track.
