---
id: pm-button-combos-recovery
title: Button combinations and recovery
topic: Using the Pacemaker
order: 10
keywords: recovery file, revival-no-preload, button combination, held keys, power on, dfu, recovery, debug menu, diagnostic mode, production test, restore hdd, check fs, clean fs, factory reset, reset, hard drive needs checking, restore hard drive, brick, unbrick
summary: The key combinations held at power-on (DFU, debug menu, diagnostics), what each does, and which can erase the library.
---

# Button combinations and recovery

## Power-on combinations

Hold the keys with the Pacemaker fully off.

| Hold at power-on | What it does | Danger |
|---|---|---|
| Play + Cue + Forward + Back for about 5 seconds, USB unplugged | DFU (recovery) mode. Then plug in USB: the computer sees a "DFU Gadget" | Flashing wrong files can leave the unit unable to start |
| Channel 2 + Forward + Back | Debug menu: Check FS, Clean FS, **Restore HDD**, Reboot | **Restore HDD erases the library** |
| Channel 1 + Forward + Back | Diagnostic mode | **HDD Check formats the drive** |

With USB plugged in before the keys are held, the Pacemaker boots normally.

## Restore hard drive

> Restoring the hard drive erases the library. Copy tracks, cases and mixes to the computer first if you can.

If it shows "Sorry friend... hard drive needs checking":

1. Reseat the ZIF ribbon first. A loose ribbon causes this message.
2. Press **Continue**. It asks **Restore hard drive?**
3. Confirm with two presses. Both partitions are formatted.
4. After "Hard drive restored. Please upgrade firmware.", put a `.pfw` file on the storage: see [Installing firmware with the app](install-firmware.md).

## Start without Revival's add-ons

If a Revival firmware will not start, put the empty file `.Pacemaker/revival-no-preload` on the storage and start again. On a Pacemaker that opens in the app, press **Add recovery file** on the Firmware card of the Device page; **Remove recovery file** undoes it: [The Device page](device-page.md#firmware).

## Recover with DFU

See [Recover with DFU](firmware-methods.md#recover-with-dfu). Use only restore files built by `make-restore-dfu.py`.

## Switch off

Hold the settings button for more than 4 seconds. If the Pacemaker will not start, see [Troubleshooting and FAQ](troubleshooting.md).
