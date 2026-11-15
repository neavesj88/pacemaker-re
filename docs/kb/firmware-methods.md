---
id: firmware-methods
title: Ways to install firmware
topic: Firmware and hardware
order: 4
keywords: firmware methods, install firmware manually, copy pfw, restore-library.py, usb mass storage, dfu, dfu-util, recovery, brick, zadig, usbipd, hidden folder, pfw, settings only
summary: Five ways to get a firmware file onto the Pacemaker: the app, copying by hand, the restore-library script, the Pacemaker's own USB, and DFU recovery. Each as short steps, with what is verified.
---

# Ways to install firmware

All of these put a `.pfw` file where the Pacemaker looks for it, or rescue a unit that will not start. The Pacemaker does the install itself at the next start. Read [Installing firmware with the app](install-firmware.md) first: it explains what the Pacemaker does with the file.

## 1. The app's Firmware card (recommended)

The Device page, Firmware card, **Install firmware...**. The app checks the file, copies it, reads it back and shows what to do next. See [Installing firmware with the app](install-firmware.md).

## 2. Copy the file by hand with the storage in a PC reader

1. Switch the Pacemaker off and take the storage out, as for [the storage upgrade](storage-upgrade.md).
2. Put it in a card reader. Open the **FAT32 volume** (the large one). Windows will not show the ext3 system partition and may offer to format it: **say no**.
3. Show hidden files, and open the hidden folder `.Pacemaker`.
4. Copy the `.pfw` file into it. The file name does not matter. The folder is searched but not its subfolders.
5. Eject the card from Windows, put it back in the Pacemaker, **leave USB unplugged** and switch on. Wait for it to restart.

This is verified in the Pacemaker's code (the folder it scans, the checks it makes), and it is the same copy the app makes. A **new firmware number** deletes the settings file, so settings reset.

## 3. The restore-library.py script

For putting a whole backed-up library onto a freshly prepared card, with checks. It is in the project's `scripts` folder and needs Python 3.

```
restore-library.py BACKUP_DIR MANIFEST TARGET_ROOT [--pfw FILE] [--settings-only]
```

- `BACKUP_DIR` is a backup folder made by the Device page's Backup (it holds a copy of the library). `MANIFEST` is the `MANIFEST.sha256` file the Backup writes inside that folder (one SHA-256 and path per line). `TARGET_ROOT` is the FAT32 volume of the new storage.
- **A default backup holds no audio.** The Backup card only copies the music files when **Include the audio files** is ticked, so to get the music back with this script, make the backup with that box ticked.
- Each file is checked against the list, copied, read back and checked again. A file that is already there with the right checksum is skipped, so it is **safe to run again** after an interruption.
- By default it skips `settings.dat` (the Pacemaker deletes it after an install) and `system.xml` (rewritten at every start).
- `--pfw FILE` also copies the firmware into `.Pacemaker`.
- `--settings-only` copies just `settings.dat`, for after the install, to bring your settings back.

Typical order: run it with `--pfw`, put the card in the Pacemaker, start it with USB unplugged, then run it again with `--settings-only`. The script's `--help` prints the same usage.

## 4. Over the Pacemaker's own USB cable

The Pacemaker appears as a drive when docked. You can copy the `.pfw` into `.Pacemaker` as in method 2, eject, **unplug USB**, and then switch on. This is the only way if the storage cannot be taken out (a hard drive).

**It is the least reliable.** The project owner found copying over the Pacemaker's own USB unreliable. Nothing in the project notes records why: the Pacemaker moves data for this kind of storage with its processor and that path was never measured, and in one test the Pacemaker hung when it was started with the cable connected, but those are only clues. If you use it, always use the app's **Eject**, wait for "Do not disconnect" to clear, check the file with the app, and never switch on with USB plugged in.

## 5. DFU recovery (for a Pacemaker that will not start)

This is a **rescue**, not a normal install, and the app does not do it. Entering the mode is verified. **Writing the flash with dfu-util has not been run end to end**, so check every step before relying on it.

1. With USB **unplugged** and the Pacemaker fully off, hold **Play, Cue, Forward and Back** together, switch on, and keep holding for about 5 seconds. There is no kernel logo, and the left light is on.
2. Then plug in USB. Windows shows a device called "DFU Gadget" (5e1d:a7a4). If you plug USB in first, the Pacemaker starts normally from the cable's power.
3. With `dfu-util` you can list the parts it exposes (Redboot, status, kernel, rootfs, data, test). That listing is verified (under Linux in WSL with usbipd-win; on Windows a driver such as Zadig is needed and is untested).
4. To restore, write the kernel, then the root system, then the status block last (with the reset flag):
   - `dfu-util -d 0x5e1d:0xa7a4 -a kernel -t 4088 -D zImage.dfu`
   - `dfu-util -d 0x5e1d:0xa7a4 -a rootfs -t 4088 -D cramfs.dfu`
   - `dfu-util -d 0x5e1d:0xa7a4 -a status -t 4088 -R -D start.dfu`
5. The kernel and root-system files come from the project's `make-restore-dfu.py` script, built from a `.pfw`. The `start.dfu` file is Tonium's own 38-byte file and **cannot be generated or shared here**.
6. After a DFU restore the Pacemaker still needs a firmware file on its storage (method 2), then starts normally.

A wrong kernel or root-system set can leave the unit unable to start, so use only files made by the project's script from a firmware you trust. If the Pacemaker only shows "hard drive needs checking", you do not need DFU: see [Troubleshooting and FAQ](troubleshooting.md).

See also [Button combinations and recovery](pm-button-combos-recovery.md).
