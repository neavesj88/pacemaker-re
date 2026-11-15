---
id: pm-computer-connection
title: Connecting to a computer
topic: Using the Pacemaker
order: 9
keywords: usb, connect, computer, mass storage, drive, eject, do not disconnect, editor, tonium editor, transfer, music.db, hidden folder, sync, copy tracks, windows
summary: How the Pacemaker looks to a computer, how to connect and disconnect safely, and what the original Editor and this app do with it.
---

# Connecting to a computer

Source: the official User's Manual v2.0 and the project's notes on the Pacemaker's library. See [Controls tour](pm-controls-tour.md) for what "verified" means.

## What the computer sees

Over USB the Pacemaker is **plain USB mass storage**: a removable FAT32 drive. Your music lives in a hidden folder called `.Pacemaker`, under `Music` with scrambled names, and the library is a database file, `music.db`, in the same folder. (Verified) Do not rename or move things in there by hand.

## Connect

1. If the Pacemaker is only charging, press the **settings button** so it turns on the data connection. (Verified)
2. It appears as a drive. In the app, press **Rescan**, then open it from the library picker: see [Getting started](getting-started.md).

## Disconnect safely

1. In the app use **Eject** on the Device page, or use the Windows eject.
2. **Wait for "Do not disconnect!" to clear on the Pacemaker's screen.**
3. Then unplug the cable. Never unplug during a transfer. (Verified)

## Putting music and playlists on it

- **This app:** [Sync](sync.md) copies tracks, mixes and cases in both directions, after showing you a preview. [Libraries](libraries.md) copies by dragging.
- **The original Tonium Editor:** its installer is on the Pacemaker's own drive. You drag tracks, cases and mixes into the Pacemaker pane. Deleting there deletes from the Pacemaker. The manual says tracks cannot be copied from the Pacemaker back to the Editor, but cases and mixes can. (Verified, from the manual.) The Editor's update and community servers no longer work.
- Rows this app adds carry a stamp that says which program added them. How the original Editor reacts to that has never been tested, so if the original Editor misbehaves on a library this app wrote, suspect that first. (Unverified)

## Firmware

A firmware file is copied into the `.Pacemaker` folder in the same way, and installs at the next start: see [Installing firmware with the app](install-firmware.md). **Do not start the Pacemaker with the USB cable connected while a firmware update is waiting.**

## Slow or unreliable copying

Copying many gigabytes over the Pacemaker's USB can be slow. For a big transfer, the storage in a PC card reader is faster and more reliable if it can be removed: see [Ways to install firmware](firmware-methods.md).
