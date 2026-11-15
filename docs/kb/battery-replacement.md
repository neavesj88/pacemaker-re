---
id: battery-replacement
title: Replacing the battery
topic: Firmware and hardware
order: 5
keywords: battery, replace battery, lipo, lithium polymer, 3.7 v, 1100 mah, lpd 723048, 803448, 103450, charge, protection circuit, short battery life, opening the case, solder, polarity, multimeter, safety, swollen
summary: What people have fitted in the Pacemaker, what is and is not known about the battery and its connector, and how to measure the old cell and choose a replacement without guessing. Li-ion safety first.
---

# Replacing the battery

A Pacemaker's battery is a single lithium-polymer cell. After many years it holds only a fraction of its charge (people report 30 to 45 minutes from worn cells, with no proper test). Replacing it is possible, but **the information is thin**: it comes from one community repair page and a forum thread, not from Tonium. Read the "not known" section before you buy anything.

> **Safety.** A lithium cell that is short-circuited, crushed, pierced, over-charged or reversed can **catch fire**. Never use a swollen cell, and never try to charge a deeply flat or swollen one. Never use a cell without a protection board. Work on a non-flammable surface, and do not squeeze a cell into a space that is too small.

## What is reported

| Fact | Source and confidence |
|---|---|
| Single lithium-polymer cell, **3.7 V nominal**, about **1100 mAh** | One forum poster cites the datasheet of the "LPD 723048" cell. Likely, but the datasheet could not be fetched to check |
| Size of the original: about **48 x 30 x 7.2 mm** (the same numbers as that cell's datasheet) | Community repair page. Likely. It is probably the same single source, not two. One other source says about 50 x 30 x 10 mm, **which disagrees**, so measure your own |
| The replacement needs **4.2 V charge voltage** and a **built-in protection circuit**, and the charger gives **1 A** charge current | One community repair page. Unconfirmed in the firmware: no charge voltage or current is recorded in this project |
| The original is joined by a **hand-soldered lead and connector** to the board. People cut the old connector off and soldered it to the new cell | Forum posts. Likely |
| With no battery fitted, the Pacemaker runs from the mains adapter | Forum and the repair page. Useful as a test |
| The charger chip is a Freescale **MC13783** power chip | Verified from the firmware sources in this project |

### What people have fitted

| Cell | Size | Notes |
|---|---|---|
| 103450, 1800 mAh | 50 x 34 x 10 mm | Fitted. **Very tight**. A report of it reading 66 percent at install and 82 percent after an hour of heavy use (the only run figure) |
| 803448, 1400 mAh | 48 x 34 x 8 mm | Fitted. About 2 mm thinner. The easier option |
| 1000 mAh | (size reported with a typo) | Fitted. "More room than needed" |
| 1500 mAh, 3.7 V | (2019) | Fitted. 1 to 2 mm too tall until the old hard-drive retaining brackets were removed |
| 103050, 1600 mAh | 50 x 30 x 10 mm | Ordered. Not reported as fitted |

A 1300 mAh iPod Mini cell was suggested once and never reported fitted.

## What is not known

- The **connector's pitch, number of pins, pin order and polarity**. No source says. Do not trust wire colours.
- An official part number and whether the original has its own protection board.
- Original capacity from Tonium: no official figure was found.
- The number and place of the case screws, hidden clips, and whether a ribbon has to be disconnected to reach the battery. One person opened it with a small Philips screwdriver and called the battery "easy to reach". The photo albums that show it are on a site that may no longer work.
- The charging settings in the firmware (the figures are in the stock player, not in the open parts).
- How the Pacemaker behaves with a cell of the wrong size, and how long each replacement lasts.

## How to choose without guessing

1. **Measure the old cell** once the case is open: length, width, thickness (with a caliper), and read the printed model and capacity on it. Photograph it in place, with its lead and connector.
2. **Choose a single-cell lithium-polymer pouch, 3.7 V (4.2 V full), with a protection board, that is no bigger than the old cell** in all three sizes. If you are unsure, pick the thinner one: a 48 x 34 x 8 mm cell has been fitted, a 10 mm one only with difficulty. A cell the same size as the original (723048) is the lowest risk.
3. Look for a **charge rating of at least 1 A**, as the community says, and **buy from a reputable seller**. The cells people used were generic marketplace listings.
4. **Never use a salvaged cell without a protection board.**

## The steps (as people did it)

1. **Switch off and unplug everything**: the mains adapter and the USB cable. Do not work on it while a charger is connected.
2. **Open the case** with a small Philips screwdriver (PH00 or PH0) and plastic tools. The screw count and any clips are undocumented, so photograph every layer. Do not pry at the display (a square screen under round glass), the ribbon cables or the ZIF hard-drive cable. A loose ZIF ribbon gives the "hard drive needs checking" error.
3. **Note the polarity.** Photograph the connector and the leads, then **measure the leads with a multimeter** before disconnecting anything.
4. **Cut the original connector off** with a few millimetres of lead. Solder it to the new cell's leads **with the right polarity**, and cover each joint with heat-shrink or Kapton tape. Do not bend or heat the tab area of a pouch cell. Or connect to the board's header only if you can confirm the pin order.
5. **Check the fit before closing.** The cell must clear the connector on the left of the bay, the hard-drive locating stays and the two moulded plastic feet, and **nothing may press on the pouch**. If it is too tall, use a thinner cell (the 1400 mAh one) or, if the hard drive has been replaced by something smaller, remove the retaining brackets that are no longer needed.
6. **Close up, reseat the ZIF ribbon fully,** and switch on with the **mains adapter connected** the first time. Watch the battery percentage in the Pacemaker's status screen.

## If something goes wrong

- **It shuts down at once at start with a charged cell.** Suspect **polarity** first, then the firmware's under-voltage guard. The firmware sets a low-battery warning at about 2.6 V plus a margin and forces a shutdown at start if the battery is too low (inferred near 3.4 V). A deeply flat new cell may look dead. Do not try to jump-start a swollen or deeply flat pouch cell.
- **The Pacemaker does not start at all with a new cell.** Try it on the mains adapter with no battery.
- **A reversed or wrong connection can destroy the power chip or the board.** Measure first.

See also [Power and charging](pm-power-charging.md).
