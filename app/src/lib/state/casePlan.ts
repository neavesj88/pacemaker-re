// The browser-side twin of pm_formats::library::{plan_merge, plan_transfer} (crates/pm-formats/
// src/library/case_bulk.rs). The real app never calls these: the preview commands run the Rust
// functions so a preview and the write share one implementation. The browser mock (devmock/
// cases.ts) needs the same behaviour, and the node test (views/cases/caseLogic.test.mjs) pins
// both ports to the same numbers the Rust tests use.
import type { MergePlan, TransferPlan } from "../api/cases";

/** The order of a merge: what `sources` (member lists, in the order the user listed the cases)
 * add to `target` (what the destination holds; empty for a new case). */
export function planMerge(sources: number[][], target: number[]): MergePlan {
  const inTarget = new Set(target);
  const seen = new Set(target);
  const added: number[] = [];
  let alreadyThere = 0;
  let repeated = 0;
  for (const id of sources.flat()) {
    if (!seen.has(id)) {
      seen.add(id);
      added.push(id);
    } else if (inTarget.has(id)) alreadyThere++;
    else repeated++;
  }
  return { added, already_there: alreadyThere, repeated, stale: 0 };
}

/** The pure part of a copy / move; `source` is null when the tracks come from the library. */
export function planTransfer(
  selected: number[],
  source: number[] | null,
  dest: number[],
  removeFromSource: boolean,
  isTrack: (id: number) => boolean,
): TransferPlan {
  const inDest = new Set(dest);
  const inSource = source ? new Set(source) : null;
  const plan: TransferPlan = { to_add: [], already_there: [], to_remove: [], not_in_source: [], missing: [] };
  for (const id of new Set(selected)) {
    if (!isTrack(id)) plan.missing.push(id);
    else if (inSource && !inSource.has(id)) plan.not_in_source.push(id);
    else {
      (inDest.has(id) ? plan.already_there : plan.to_add).push(id);
      if (removeFromSource && inSource) plan.to_remove.push(id);
    }
  }
  return plan;
}
