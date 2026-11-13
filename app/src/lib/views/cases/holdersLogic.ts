// Pure part of "which cases hold this track": the rows the dialog and the inspector chips show.
// Types only are imported, so `holdersLogic.test.mjs` runs it with plain node.
import type { TrackHolding } from "$lib/api/cases";

export type HoldingRow = { case_id: number; name: string; note: string };
export type SmartRow = { id: number; name: string; note: string };

/** The cases that hold the track, each with where it sits there, and the smart cases that match
 *  it. A smart case whose Pacemaker copy already holds the track is shown once, as that copy
 *  (the row says it is a smart case's copy), not a second time as a smart case. */
export function holdingRows(h: TrackHolding): { cases: HoldingRow[]; smart: SmartRow[] } {
  const copies = new Set(h.cases.filter((c) => c.smart_copy_of !== null).map((c) => c.smart_copy_of));
  const smartName = new Map(h.smart.map((s) => [s.id, s.name] as const));
  return {
    cases: h.cases.map((c) => ({
      case_id: c.case_id,
      name: c.name,
      note: `${c.position} of ${c.of}${c.smart_copy_of !== null ? `, Pacemaker copy of smart case “${smartName.get(c.smart_copy_of) ?? "?"}”` : ""}`,
    })),
    smart: h.smart
      .filter((s) => !copies.has(s.id))
      .map((s) => ({ id: s.id, name: s.name, note: s.on_device ? "rules match; its Pacemaker copy does not hold it yet" : "rules match; not a case on the Pacemaker" })),
  };
}
