// Pure logic of "keep the Pacemaker copies of smart cases current": the sentences that name what a
// refresh will change (or changed), and the signature that lets "Not now" be remembered. Types
// only are imported, so `smartRefresh.test.mjs` runs it with plain node.
import type { RefreshItem, RefreshMode, RefreshReport, TrackRef } from "$lib/api/cases";

export const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;

/** "Title by Artist" (just the title when the artist is unknown). */
export function trackLine(t: TrackRef): string {
  const title = t.title.trim() || "(untitled)";
  return t.artist.trim() ? `${title} by ${t.artist.trim()}` : title;
}

/** Items that would be (or were) written. */
export const changed = (r: RefreshReport) => r.items.filter((i) => i.action === "create" || i.action === "update");
export const skipped = (r: RefreshReport) => r.items.filter((i) => i.action === "skip");

/** What differs, in a few words: "3 added", "1 removed", "new order", "renamed back". */
export function changeParts(i: RefreshItem): string[] {
  const parts: string[] = [];
  if (i.added.length) parts.push(`${i.added.length} added`);
  if (i.removed.length) parts.push(`${i.removed.length} removed`);
  if (i.reordered) parts.push("new order");
  if (i.renamed_from !== null) parts.push("renamed back");
  return parts;
}

/** One line per smart case: what its Pacemaker copy gets (`done` = it has been written). */
export function itemLine(i: RefreshItem, done: boolean): string {
  const name = `“${i.name}”`;
  const copy = `“${i.device_name}”`;
  switch (i.action) {
    case "create": {
      const verb = done ? "made" : "will be made";
      const why = i.recreated ? " again (it was deleted on the Pacemaker)" : "";
      return `${name}: ${copy} ${verb}${why} with ${plural(i.after, "track")}`;
    }
    case "update": {
      const parts = changeParts(i).join(", ");
      return `${name}: ${copy} ${done ? "now has" : "will have"} ${plural(i.after, "track")} (was ${i.before}; ${parts})`;
    }
    case "unchanged":
      return `${name}: ${copy} is up to date`;
    case "skip":
      return `${name}: not refreshed, ${copy} left as it was. ${i.error ?? ""}`.trim();
  }
}

/** The tracks that come and go in an item, one line each, for the dialog's detail list. */
export function itemTrackLines(i: RefreshItem): { sign: "+" | "-"; text: string }[] {
  return [
    ...i.added.map((t) => ({ sign: "+" as const, text: trackLine(t) })),
    ...i.removed.map((t) => ({ sign: "-" as const, text: trackLine(t) })),
  ];
}

/** The sentence after a refresh ran (toast / banner): counts, then every smart case by name. */
export function doneSummary(r: RefreshReport): string {
  const did = changed(r);
  const skippedItems = skipped(r);
  if (did.length === 0 && skippedItems.length === 0) return "The Pacemaker copies of your smart cases were already up to date.";
  const bits = did.map((i) => {
    if (i.action === "create") return `“${i.name}” ${i.recreated ? "made again" : "made"} (${plural(i.after, "track")})`;
    return `“${i.name}” (${changeParts(i).join(", ")})`;
  });
  let text = did.length ? `Refreshed ${plural(did.length, "smart case")} on the Pacemaker: ${bits.join("; ")}.` : "No smart case was refreshed.";
  if (skippedItems.length) {
    text += ` Not refreshed: ${skippedItems.map((i) => `“${i.name}” (${i.error ?? "unknown reason"})`).join("; ")}.`;
  }
  return text;
}

/** A string that is the same for the same pending changes, so an answered "Not now" is not
 *  asked again until something different would change. Skips are included: a smart case that
 *  starts or stops being skipped is news. */
export function planSignature(r: RefreshReport): string {
  return r.items
    .filter((i) => i.action !== "unchanged")
    .map((i) => [i.id, i.action, i.added.map((t) => t.track_id).join(","), i.removed.map((t) => t.track_id).join(","), i.reordered ? "r" : "", i.renamed_from ?? "", i.error ?? ""].join("|"))
    .join(";");
}

/** When a deleted Pacemaker copy of a smart case comes back, per the refresh setting: for the
 *  "this will be made again ..." notes in the delete and merge confirmations. */
export function remakeWhen(mode: RefreshMode): string {
  return mode === "auto" ? "automatically straight afterwards" : mode === "ask" ? "when you answer yes to the refresh question" : "the next time you refresh";
}

/** Whether the report is worth surfacing from the automatic check: a write is pending, or a
 *  smart case cannot be refreshed (it would otherwise only show when the button is pressed). */
export const needsAttention = (r: RefreshReport) => r.changes > 0 || skipped(r).length > 0;

/** One sentence per smart case that was not refreshed, with the reason. */
export const stuckLines = (items: RefreshItem[]) => items.map((i) => `“${i.name}” was not refreshed: ${i.error ?? "unknown reason"}`);

/** What the automatic check remembers between runs. */
export interface RefreshMemory {
  /** The plan the user answered "Not now" to. */
  dismissed: string;
  /** A plan whose write failed (or that has nothing to write but skips): not retried / repeated. */
  failed: string;
  /** Plans the automatic mode has already written since things last settled. */
  written: Set<string>;
}

export type CheckAction = "nothing" | "ask" | "write" | "stuck" | "loop";

/** What the automatic check does with `plan` under `mode`. "stuck": only skips are left, there
 *  is nothing to write, say so (once). "loop": automatic mode would write a plan it has already
 *  written and the library has not settled in between, so something keeps undoing the writes
 *  (two smart cases changing each other, or an outside program): it stops instead of writing
 *  the card again and again. Updates `mem` (forgets what no longer applies). */
export function decideCheck(plan: RefreshReport, mode: RefreshMode, mem: RefreshMemory): CheckAction {
  if (mode === "off") return "nothing";
  if (!needsAttention(plan)) {
    mem.dismissed = "";
    mem.failed = "";
    mem.written.clear();
    return "nothing";
  }
  if (plan.changes === 0) mem.written.clear(); // converged: what is left cannot be written
  const sig = planSignature(plan);
  if (sig === mem.dismissed || sig === mem.failed) return "nothing";
  if (mode === "ask") return "ask";
  if (plan.changes === 0) return "stuck";
  return mem.written.has(sig) ? "loop" : "write";
}

/** The sentence when automatic refresh stops because it would write the same changes again. */
export function loopMessage(plan: RefreshReport): string {
  const names = changed(plan).map((i) => `“${i.name}”`).join(", ");
  return `Automatic refresh of smart cases stopped: the changes to ${names} were already written and came back, so something keeps undoing them (for example smart cases whose rules read each other's Pacemaker copies). Nothing more is written automatically until the changes are different; “Refresh smart cases” shows them.`;
}
