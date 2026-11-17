---
id: firmware-methods
title: Ways to install firmware
topic: Firmware and hardware
order: 4
keywords: firmware methods, install firmware manually, copy pfw, restore-library.py, usb mass storage, dfu, dfu-util, recovery, brick, zadig, usbipd, hidden folder, pfw, settings only
summary: Five ways to put a firmware file on the Pacemaker or recover a unit that will not start: the app, manual copy, restore-library.py, USB and DFU.
---

# Ways to install firmware

| Method | Use |
| --- | --- |
| [App Firmware card](install-firmware.md) | Default |
| Manual copy | Storage in a PC reader |
| restore-library.py | Restore a backed-up library to a new card |
| Pacemaker's own USB | Storage cannot be removed |
| DFU recovery | Unit will not start |

The Pacemaker installs the file at its next start.

## Copy the file by hand

1. Switch the Pacemaker off and take the storage out, as for [the storage upgrade](storage-upgrade.md).
2. Put it in a card reader. Open the FAT32 volume (the large one). Decline any offer to format the other partition.
3. Show hidden files and open the folder `.Pacemaker`.
4. Copy the `.pfw` file into it.
5. Eject the card, put it back in the Pacemaker, leave USB unplugged and switch on.

## Use restore-library.py

Restores a backed-up library onto a prepared card. Found in the project's `scripts` folder. Needs Python 3. Make the backup with **Include the audio files** ticked to restore music.

```
restore-library.py BACKUP_DIR MANIFEST TARGET_ROOT [--pfw FILE] [--settings-only]
```

| Argument | Meaning |
| --- | --- |
| `BACKUP_DIR` | Backup folder made by the Device page Backup |
| `MANIFEST` | The `MANIFEST.sha256` file inside that folder |
| `TARGET_ROOT` | FAT32 volume of the new storage |
| `--pfw FILE` | Also copies the firmware into `.Pacemaker` |
| `--settings-only` | Copies only `settings.dat` |

1. Run with `--pfw`.
2. Put the card in the Pacemaker and start it with USB unplugged.
3. Run again with `--settings-only`.

## Use the Pacemaker's own USB

Docked, the Pacemaker appears as a drive. Use this when the storage cannot be taken out (a hard drive).

1. Copy the `.pfw` into `.Pacemaker`.
2. Press the app's **Eject** and wait for "Do not disconnect" to clear.
3. Unplug USB, then switch on.

> Never switch on with USB plugged in.

## Recover with DFU

For a Pacemaker that will not start. The app does not do this. Needs Linux, or WSL with usbipd-win.

1. With USB unplugged and the Pacemaker off, hold Play, Cue, Forward and Back together, switch on, and keep holding for about 5 seconds. No kernel logo shows and the left light is on.
2. Plug in USB. Windows shows "DFU Gadget" (5e1d:a7a4).
3. Check the parts with `dfu-util`: Redboot, status, kernel, rootfs, data, test.
4. Write the kernel, then the root system, then the status block last:
   - `dfu-util -d 0x5e1d:0xa7a4 -a kernel -t 4088 -D zImage.dfu`
   - `dfu-util -d 0x5e1d:0xa7a4 -a rootfs -t 4088 -D cramfs.dfu`
   - `dfu-util -d 0x5e1d:0xa7a4 -a status -t 4088 -R -D start.dfu`
5. Build the kernel and root-system files from a `.pfw` with the project's `make-restore-dfu.py`. `start.dfu` comes from the original manufacturer and is not provided.
6. Put a firmware file on the Pacemaker's storage (manual copy). The Pacemaker then starts normally.

> Use only files built by `make-restore-dfu.py` from a trusted firmware. A wrong set leaves the unit unable to start.

"Hard drive needs checking" does not need DFU. See [Troubleshooting and FAQ](troubleshooting.md) and [Button combinations and recovery](pm-button-combos-recovery.md).
