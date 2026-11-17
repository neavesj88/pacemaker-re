---
id: install-firmware
title: Installing firmware with the app
topic: Firmware and hardware
order: 1
keywords: firmware, install firmware, pfw, update, upgrade, firmware card, copy to device, eject, unplug usb, power on, settings reset, revival firmware, 180096073, dfu, recovery, version number, checksum
summary: The Firmware card on the Device page checks a .pfw file, copies it to the Pacemaker and shows the steps to finish the install.
---

# Installing firmware with the app

The **Firmware** card on the [Device page](device-page.md) copies a `.pfw` file to the Pacemaker, which installs it at its next start. Other methods: [Ways to install firmware](firmware-methods.md).

You need a `.pfw` file (the app does not download firmware) and a battery above about a fifth or the power adapter.

## Install firmware

1. Switch the Pacemaker off and take the storage out (for example a microSD card, see [Upgrading the storage to a microSD card](storage-upgrade.md)).
2. Put it in a card reader. Press **Rescan** in the app and open the drive.
3. Go to **Device** and read the **Firmware** card: firmware, software and bootloader numbers.
4. Press **Install firmware...** and choose the `.pfw` file.
5. Read the check: file name, size, numbers, checksum result and the number installed now. If it lists problems there is no Copy button.
6. Press **Copy to device**.
7. In the **Waiting on the device** box:
   1. Press **Eject now** (or eject with Windows).
   2. Put the storage back in the Pacemaker.
   3. Make sure the USB cable is unplugged.
   4. Power the Pacemaker on and leave it alone. It restarts by itself when done.
8. Open the Device page again and check the Firmware card shows the new number.

> Always power on with USB unplugged. Do not switch the Pacemaker off during the upgrade.

With the Pacemaker docked over USB, follow the same steps and unplug USB before powering on.

## Before you install

- The firmware and bootloader numbers must be above 12837.
- A new firmware number resets settings to defaults. Save them first as a preset (Settings presets card) and apply them after. See [The settings menu](pm-settings-menu.md).
- A file with the same firmware number as installed changes nothing.

## Firmware versions

| Version | Notes |
| --- | --- |
| Stock `180096069` | Original firmware |
| Revival | Adds the Vocal filter (needs `180096073` or later) and a different boot logo |

Only install a file that matches your unit; a firmware number without its matching root system ends in "FW Upgrade is NOT OK". See [Effects on the Pacemaker](pm-effects.md).

## If it does not start

See [Troubleshooting and FAQ](troubleshooting.md) and [Button combinations and recovery](pm-button-combos-recovery.md).
