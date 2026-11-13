// The one rule for "what energy does this track have": the rating you set, the estimate made from
// the audio (Energy (auto)), or the first of the two that exists, under the "Energy source"
// setting. Pure (no store, no `$lib` imports) so it runs under plain node (`npm test`) and every
// area can share it: the Tracks search and label filter, smart-case previews, the gap map, the
// set builder and the printable setlist all read energy through `resolveLevel`.
//
// It mirrors `resolve_level` in src-tauri/src/commands/tags/energy_source.rs, which the smart-case
// rules run when a smart case's Pacemaker copy is rewritten. Both are run against the vectors in
// energy.parity.json, so the preview in the window and the rewrite on the card cannot disagree.

/** "mine": only your rating. "auto": only the estimate. "mine_else_auto": your rating, else the estimate. */
export type EnergySource = "mine" | "auto" | "mine_else_auto";

export const DEFAULT_ENERGY_SOURCE: EnergySource = "mine_else_auto";

export const ENERGY_SOURCES: { id: EnergySource; label: string; help: string }[] = [
  { id: "mine_else_auto", label: "Mine, else auto", help: "Your own 1 to 10 rating where you set one, otherwise the estimate made from the audio." },
  { id: "mine", label: "Mine only", help: "Only the rating you set. Tracks you have not rated have no energy." },
  { id: "auto", label: "Auto only", help: "Only the estimate made from the audio (rounded to a whole level). Your own ratings are ignored." },
];

export const isEnergySource = (s: unknown): s is EnergySource => s === "mine" || s === "auto" || s === "mine_else_auto";

export const energySourceLabel = (s: EnergySource): string => ENERGY_SOURCES.find((x) => x.id === s)?.label ?? s;

/** An estimate (1.0 to 10.0) as a whole level 1 to 10; null when it is not a usable number or
 *  rounds below 1. */
export function autoLevel(auto: number | null | undefined): number | null {
  if (typeof auto !== "number" || !Number.isFinite(auto)) return null;
  const r = Math.round(auto);
  return r >= 1 ? Math.min(r, 10) : null;
}

/** The whole-level energy (1 to 10) of a track, or null when it has none under `source`. `mine`
 *  is the rating you set, `auto` the unrounded estimate. */
export function resolveLevel(mine: number | null | undefined, auto: number | null | undefined, source: EnergySource): number | null {
  const m = typeof mine === "number" && Number.isInteger(mine) && mine >= 1 && mine <= 10 ? mine : null;
  const a = autoLevel(auto);
  switch (source) {
    case "mine": return m;
    case "auto": return a;
    default: return m ?? a;
  }
}

/** Which of the two values `resolveLevel` is using for a track, for dimming the other one in the
 *  table: "mine", "auto", or null when the track has no energy under `source`. */
export function energyUsed(mine: number | null | undefined, auto: number | null | undefined, source: EnergySource): "mine" | "auto" | null {
  const m = typeof mine === "number" && Number.isInteger(mine) && mine >= 1 && mine <= 10;
  const a = autoLevel(auto) !== null;
  if (source === "mine") return m ? "mine" : null;
  if (source === "auto") return a ? "auto" : null;
  return m ? "mine" : a ? "auto" : null;
}
