---
id: install-firmware
title: Installing firmware with the app
topic: Firmware and hardware
order: 1
keywords: firmware, included firmware, update to, recovery file, revival-no-preload, install firmware, pfw, update, upgrade, firmware card, copy to device, eject, unplug usb, power on, settings reset, revival firmware, 180096073, dfu, recovery, version number, checksum
summary: The Firmware card on the Device page installs the firmware included with the app or a .pfw file you choose, after a warning, and shows the steps to finish the install.
---

# Installing firmware with the app

The **Firmware** card on the [Device page](device-page.md) copies a `.pfw` file to the Pacemaker, which installs it at its next start. Other methods: [Ways to install firmware](firmware-methods.md).

The app includes a Revival firmware file. A `.pfw` file of your own is also accepted. Both need a battery above about a fifth or the power adapter.

## Install firmware

1. Switch the Pacemaker off and take the storage out (for example a microSD card, see [Upgrading the storage to a microSD card](storage-upgrade.md)).
2. Put it in a card reader. Press **Rescan** in the app and open the drive.
3. Go to **Device** and read the **Firmware** card: firmware, included with the app, software and bootloader numbers.
4. To install the included firmware, press **Update to** followed by its number (see Update to the included firmware below). For a file of your own, press **Install firmware...** and choose the `.pfw` file.
5. For your own file, read the check: file name, size, numbers, checksum result and the number installed now. If it lists problems there is no Copy button. Press **Copy to device**.
6. For any Revival firmware, including the included one, a warning follows: tick both boxes (verified backup, install at your own risk), then press **Copy firmware**.
7. In the **Waiting on the device** box:
   1. Press **Eject now** (or eject with Windows).
   2. Put the storage back in the Pacemaker.
   3. Make sure the USB cable is unplugged.
   4. Power the Pacemaker on and leave it alone. It restarts by itself when done.
8. Open the Device page again and check the Firmware card shows the new number.

> Always power on with USB unplugged. Do not switch the Pacemaker off during the upgrade.

With the Pacemaker docked over USB, follow the same steps and unplug USB before powering on.

## Update to the included firmware

The Firmware card always has a button for the included file:

| Pacemaker has | Button |
|---|---|
| A lower number | **Update to** followed by the number, with "Update available. Settings reset after updating." |
| The same number | **Firmware warning...** (shows the warning; copying changes nothing) |
| A higher or unreadable number | **Install** followed by the number |

A file that cannot be installed shows its problems instead of the warning.

| Setting | Where | Default |
|---|---|---|
| **Offer the included firmware** | Settings, Device, Included firmware | On |

## The warning

Every Revival firmware (a file named Revival, or a number above 180096069) opens **Install firmware** followed by the number. **Copy firmware** stays disabled until both boxes are ticked:

- **I have a verified backup of this Pacemaker.** Shows the time of the last backup this session, or "No backup yet this session" with **Go to the Backup card**.
- **I understand ... I install it at my own risk.** The text says the firmware has no warranty and no recourse.

The dialog also notes that it works best with the microSD storage upgrade ([Upgrading the storage to a microSD card](storage-upgrade.md)) and lists what to do if it does not start: the recovery file, recovery mode and DFU.

## Recovery file

**Recovery file** on the Firmware card shows "On the card" or "Not on the card". **Add recovery file** creates the empty file `.Pacemaker/revival-no-preload`. At the next start the Pacemaker leaves Revival's add-ons off (today the Vocal filter). **Remove recovery file** turns them on again.

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
