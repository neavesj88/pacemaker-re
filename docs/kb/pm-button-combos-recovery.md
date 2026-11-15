---
id: pm-button-combos-recovery
title: Button combinations and recovery
topic: Using the Pacemaker
order: 10
keywords: button combination, held keys, power on, dfu, recovery, debug menu, diagnostic mode, production test, restore hdd, check fs, clean fs, factory reset, reset, hard drive needs checking, restore hard drive, brick, unbrick
summary: The key combinations held at power-on (DFU, debug menu, diagnostics), what each one does, and which can wipe your library.
---

# Button combinations and recovery

Source: the project's notes on the firmware (some of it read from the firmware's code, one combination tested on hardware). See [Controls tour](pm-controls-tour.md) for what "verified" means. The firmware names Forward and Back as "search forward" and "search back". Matching them to the physical buttons is **inferred**.

All of these are done with the Pacemaker **fully off**.

| Hold at power-on | What it does | Danger | How sure |
|---|---|---|---|
| **Play + Cue + Forward + Back**, for about 5 seconds, **USB unplugged** | **DFU** (recovery) mode: no kernel logo and the left light is on. Then plug in USB and the computer sees a "DFU Gadget" (5e1d:a7a4). If USB is plugged in first, the Pacemaker boots normally from the cable | Flashing from DFU can leave the unit unable to start if the wrong files are used | Entering DFU **verified on hardware**. Flashing from it is untested here |
| **Channel 2 + Forward + Back** | **Debug menu** with Check FS, Clean FS, **Restore HDD** and Reboot | **Restore HDD erases the library** | From the firmware's code (static analysis), not tested |
| **Channel 1 + Forward + Back** | **Diagnostic or production-test mode** | Its HDD Check **formats the drive** | From the firmware's code, not tested |

**Do not use the debug menu's Restore HDD or the diagnostic mode's HDD Check unless you mean to erase everything.**

## "Restore hard drive?"

If the system or music partition fails to mount **five times** (a blank or foreign drive), the Pacemaker shows "Sorry friend... hard drive needs checking". Press **Continue**, and it asks **Restore hard drive?** It needs two presses to go ahead. **Restore formats both partitions.** Nothing is wiped until you confirm. (Verified from the firmware's code.) Afterwards it says "Hard drive restored. Please upgrade firmware." until you put a `.pfw` file on the storage: see [Installing firmware with the app](install-firmware.md).

A loose ZIF ribbon also causes "hard drive needs checking". Reseat it before you restore anything.

## DFU, in steps

See [Ways to install firmware](firmware-methods.md), method 5. In short: enter DFU with the combination above, plug in USB, write the kernel, then the root system, then the status block last. Use only restore files made by the project's script.

## What is not known

- **There is no documented hold-to-reset combination.** The only known ways are holding the settings button for more than 4 seconds to switch off, DFU, and the debug menu.
- **How to unlock** a locked Pacemaker: the settings have Lock and Unlock, but the gesture was not traced.

If the Pacemaker will not start, see [Troubleshooting and FAQ](troubleshooting.md).
