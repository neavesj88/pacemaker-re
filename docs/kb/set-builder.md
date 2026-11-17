---
id: set-builder
title: Set builder
topic: Library tools
order: 1
keywords: set builder, set, order, harmonic, camelot, key moves, energy curve, lock, leave out, solve, order the set, save as case, create mix, target length
summary: Orders tracks by key, tempo and an optional energy curve, then saves the result as a case or a mix.
---

# Set builder

**Set builder** (under Tools) orders tracks into a set. The source case is not changed.

## Build a set

1. Choose a source: a case, or **Selected tracks** (select in Tracks, right-click, Build a set). A case also has a **Build a set** button.
2. Press **Order the set**. Press **Re-solve** after changing settings.
3. Adjust the order.
4. Press **Save as case...** or **Create mix...** ([Building a mix](mixes-building.md)).

## Adjust the order

- Drag tracks, or press Alt+Up and Alt+Down.
- **Lock** keeps a track in place. **Lock all** and **Unlock all** apply to every track.
- **Leave out** removes a track from the set. Left-out tracks are listed below and can be added back.
- **Undo** (Ctrl+Z) steps back.
- **Original order** restores the source order.
- **Reload** re-reads the source and drops locks and edits.

## Settings

All settings also appear on the Settings page ([The Settings page](settings-page.md)).

### Key moves

Tick the moves that count as good. Any other pair is a clash. A track with no key is neutral.

- Same key
- Relative major or minor (8A to 8B)
- One step round the wheel (8A to 7A or 9A)
- Two steps
- Diagonal (8A to 9B or 7B)
- Semitone up or down

### Tempo

- Largest jump between neighbours, as a percentage.
- Half and double time count as matching.
- Slider: how much key counts against tempo.

### Energy curve

- None, or Warm-up, peak, cool-down.
- Peak position and strictness.
- Needs track energy; without it the curve has no effect ([BPM, keys and waveforms](bpm-keys-waveforms.md)).

### Target length

Tracks that fit least are left out until the total is closest to the target. A mix made from the set is slightly shorter because transitions overlap.

### Automatic ordering

- Order the set as soon as tracks are loaded.
- Size limit: a bigger source waits for Re-solve.
