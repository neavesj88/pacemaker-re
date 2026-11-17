---
id: battery-replacement
title: Replacing the battery
topic: Firmware and hardware
order: 5
keywords: battery, replace battery, lipo, lithium polymer, 3.7 v, 1100 mah, lpd 723048, 803448, 103450, charge, protection circuit, short battery life, opening the case, solder, polarity, multimeter, safety, swollen
summary: Replace the Pacemaker's single lithium-polymer cell: specification, cells that fit, and the removal and soldering steps.
---

# Replacing the battery

The battery is a single lithium-polymer cell. Measure your own cell before buying.

> A short-circuited, crushed, pierced, overcharged or reversed lithium cell can catch fire. Never use or charge a swollen or deeply flat cell, or a cell without a protection board. Work on a non-flammable surface.

## Cell specification

- Single-cell lithium-polymer pouch, 3.7 V (4.2 V full), about 1100 mAh (LPD 723048), about 48 x 30 x 7.2 mm.
- Built-in protection circuit.
- Charge rating of at least 1 A.
- No larger than the old cell in any dimension.

## Cells that fit

| Cell | Size | Notes |
|---|---|---|
| 803448, 1400 mAh | 48 x 34 x 8 mm | Fits more easily |
| 103450, 1800 mAh | 50 x 34 x 10 mm | Very tight |
| 1500 mAh | - | 1 to 2 mm too tall until the old hard-drive retaining brackets were removed |
| 1000 mAh | - | Fits with room to spare |

## Connector and polarity

The connector pitch, pin count, pin order and polarity are not known for certain. Do not rely on wire colours. Check the old cell's polarity with a multimeter before connecting a new one. Reversed polarity can destroy the power chip or the board.

## Replace the battery

1. Switch off and unplug the mains adapter and USB cable.
2. Open the case with a small Philips screwdriver and plastic tools. Photograph every layer. Do not pry at the display, ribbon cables or the ZIF hard-drive cable.
3. Photograph the connector and leads. Check the lead polarity as in Connector and polarity before disconnecting.
4. Cut the old connector off with a few millimetres of lead. Cut and insulate one lead at a time so the leads never touch.
5. Solder the leads to the new cell with the correct polarity. Cover each joint with heat-shrink or Kapton tape. Do not bend or heat the pouch tab.
6. Check the fit. The cell must clear the connector, the hard-drive locating stays and the two moulded plastic feet, and nothing may press on the pouch. If it is too tall, use a thinner cell or or, if the hard drive was replaced by something smaller, remove the retaining brackets.
7. Re-check polarity against your photos.
8. Close the case, reseat the ZIF ribbon fully and switch on with the mains adapter connected. Check the battery percentage on the status screen.

## Troubleshooting

- Shuts down at start: check polarity first. A too-low battery voltage also forces shutdown.
- If "hard drive needs checking" appears, reseat the ZIF ribbon.
- Does not start with a new cell: run on the mains adapter with no battery fitted.

See also [Power and charging](pm-power-charging.md).
