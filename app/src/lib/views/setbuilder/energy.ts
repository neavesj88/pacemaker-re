// Where the set builder reads a track's energy from: the energy you rated it (Tracks > Tags,
// colour, energy), else the app's own estimate (Energy (auto)), else nothing. Whole levels 1 to 10,
// or null when neither exists, in which case the energy curve in the settings has nothing to follow.
//
// It is synchronous on purpose: the solver is pure and the list calls it for every row.
import { energy } from "$lib/state/energy.svelte";
import { labels } from "$lib/state/labels.svelte";

export function energyOf(trackId: number): number | null {
  const e = labels.get(trackId)?.energy ?? energy.of(trackId);
  if (typeof e !== "number" || !Number.isFinite(e)) return null;
  const r = Math.round(e);
  return r >= 1 ? Math.min(r, 10) : null;
}
