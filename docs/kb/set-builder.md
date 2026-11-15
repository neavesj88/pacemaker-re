---
id: set-builder
title: Set builder
topic: Library tools
order: 1
keywords: set builder, set, order, harmonic, camelot, key moves, energy curve, lock, leave out, solve, order the set, save as case, create mix, target length
summary: Puts tracks in a good running order by key, tempo and (optionally) an energy curve, then saves the result as a case or lays it out as a mix.
---

# Set builder

**Set builder** (under Tools) orders a pile of tracks into a set. It never changes the case you start from.

## Steps

1. Choose a source at the top: a case, or **Selected tracks** (select tracks in the Tracks view first, or right-click and choose Build a set). A case also has a **Build a set** button.
2. Press **Order the set**. The tracks are put in an order that moves smoothly between keys and tempos. Press **Re-solve** to try again after you change the settings.
3. Adjust by hand. You can drag tracks (or press Alt+Up and Alt+Down), **lock** a track so ordering leaves it where it is (Lock all and Unlock all), or **leave out** tracks you do not want. Left-out tracks are listed below and can be added back.
4. **Save as case...** makes a new case from the order. **Create mix...** lays the tracks out as a mix with the beat-match and transition tools.

**Undo** (Ctrl+Z) steps back through your edits and **Original order** returns to the order the tracks came in. **Reload** reads the source again and drops locks and edits (you are asked first).

## The settings beside the list

- **Key moves that count as good**: same key, relative major or minor (8A to 8B), one step round the wheel (8A to 7A or 9A), two steps, diagonal (8A to 9B or 7B) and a semitone up or down. A pair no ticked move covers is a clash. A track with no key counts as neutral.
- **Tempo**: the largest jump between neighbours (a percentage), and whether half and double time count as matching (70 BPM after 140). A slider decides how much key counts against tempo.
- **Energy curve**: None, or Warm-up, peak, cool-down; where the peak falls and how strictly it is followed. It needs tracks with an energy under the Energy source setting: see [BPM, keys and waveforms](bpm-keys-waveforms.md). With none, the curve changes nothing and the panel says so.
- **Target length**: with a target, the tracks that fit least are left out until the total is as close as it can be. It is the sum of play times; a mix made from the set comes out a little shorter because the transitions overlap.
- Order the set as soon as tracks are loaded, and a size limit for automatic ordering (a bigger source loads in its own order and waits for Re-solve).

All of these are saved, and the Settings page has the same entry: see [The Settings page](settings-page.md). Mixes are covered in [Building a mix](mixes-building.md).
