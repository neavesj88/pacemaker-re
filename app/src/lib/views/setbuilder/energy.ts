// Where the set builder reads a track's energy from: the one resolver every energy reader shares
// (state/energySource.svelte.ts), which applies the "Energy source" setting (your rating, the
// Energy (auto) estimate, or yours else the estimate). Whole levels 1 to 10, or null when the
// track has none, in which case the energy curve in the settings has nothing to follow.
//
// It is synchronous on purpose: the solver is pure and the list calls it for every row, and the
// set's track table is derived from it, so a change of the setting re-evaluates the set.
import { energySource } from "$lib/state/energySource.svelte";

export const energyOf = (trackId: number): number | null => energySource.level(trackId);
