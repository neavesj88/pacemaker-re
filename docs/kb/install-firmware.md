---
id: install-firmware
title: Installing firmware with the app
topic: Firmware and hardware
order: 1
keywords: firmware, install firmware, pfw, update, upgrade, firmware card, copy to device, eject, unplug usb, power on, settings reset, revival firmware, 180096073, dfu, recovery, version number, checksum
summary: Step by step with the Firmware card on the Device page. The card in a PC reader is the reliable way, the Pacemaker boots with USB unplugged, and a new firmware number resets the settings.
---

# Installing firmware with the app

The app can put a firmware file (a `.pfw` file) onto the Pacemaker and tell you what to do next. The Pacemaker installs it itself the next time it starts. This article is the app's own **Firmware** card on the [Device page](device-page.md) (its **?** opens this article). For the other ways to get a firmware file onto the unit, see [Ways to install firmware](firmware-methods.md).

## What you need

- A `.pfw` firmware file. The app does not download firmware. The stock firmware is Tonium's `180096069`. Revival firmware is built from this project's scripts from the stock image, and is not stored in Git, because the stock image is Tonium's and is not redistributed.
- The Pacemaker open in the app. A library on this computer has no Firmware card.
- At least about a fifth of a battery, or the power adapter. (This is community advice from a web page that could not be re-checked, and not something the app checks.)

## The reliable way: the storage in a PC reader

If the Pacemaker's storage can be taken out (for example a microSD card on an adapter, see [Upgrading the storage to a microSD card](storage-upgrade.md)), this is the dependable route.

1. Switch the Pacemaker off and take the storage out.
2. Put it in a card reader on this computer. It appears as a drive. Press **Rescan** in the app and open it.
3. Go to **Device** and look at the **Firmware** card. It shows the firmware, software and bootloader numbers and the masked serial number.
4. Press **Install firmware...** and choose the `.pfw` file.
5. Read the check. The app shows the file name and size, the firmware, software and bootloader numbers, whether the checksum matches and the number installed now. **If it lists problems, the file cannot be installed and there is no Copy button.**
6. Press **Copy to device**. The file is copied into the `.Pacemaker` folder and read back to check it. Nothing else on the storage changes.
7. A box titled **Waiting on the device** appears. Follow it:
   1. Press **Eject now** (or eject with Windows), so nothing is still writing.
   2. Put the storage back in the Pacemaker.
   3. **Make sure the USB cable is unplugged.**
   4. Power the Pacemaker on and leave it alone. The screen says a firmware upgrade is in progress. It restarts by itself when done.
8. Open the Device page again and check the Firmware card shows the new number.

## If the Pacemaker is docked over USB

You can use the same steps with the Pacemaker docked, then **unplug the USB cable before you power it on**. The copy itself usually works, but the project owner found copying over the Pacemaker's own USB connection **unreliable** on the unit tested. The reason is not recorded in the project notes (a slow storage path is only a guess), so use the card reader when you can.

## Why unplug USB

On the unit tested, booting with the USB cable connected to a computer made the install hang after the kernel logo (Windows reported "Device Descriptor Request Failed"). With the cable unplugged, the update ran. This is **verified on that one unit**, and the app's steps repeat it.

## What the Pacemaker does with the file

- It looks in the `.Pacemaker` folder for any valid `.pfw` (the file name does not matter). A valid file has the right trailer and a matching checksum. If there are several, the highest software number wins.
- Both the firmware number and the bootloader number must be above 12837. This is not a "newer than installed" test.
- It moves the file to its system partition, flashes the kernel and root system **only if the firmware number differs** from the one running, then restarts and keeps a copy.
- A **new firmware number deletes the settings file**, so the Pacemaker's settings go back to the defaults. Save them first as a preset (Settings presets card) or note them, and apply them after. See [The settings menu](pm-settings-menu.md).
- An **equal** number flashes nothing: the Pacemaker accepts the file and changes nothing. The app tells you when the file has the number already installed.

## Choosing a firmware

- Stock `180096069` is what the Pacemaker shipped with.
- Revival firmware adds the Vocal filter, a different boot logo, and for some storage adapters a fix so the whole card is used. The Vocal filter needs `180096073` or later. See [Effects on the Pacemaker](pm-effects.md) and [Upgrading the storage to a microSD card](storage-upgrade.md).
- Only install a file that matches your unit. A mismatched firmware number without a matching root system ends in "FW Upgrade is NOT OK".

## If it does not start

Leave it alone for a while and do not switch it off. If nothing happens, see [Troubleshooting and FAQ](troubleshooting.md) and [Button combinations and recovery](pm-button-combos-recovery.md).
