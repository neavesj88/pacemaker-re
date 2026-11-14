// The pure rules of the Sync view: which libraries are on offer and the default pair, the preview
// groups, what a left-out track does to the mixes and cases that need it, the space line and the
// words. Run by plain node in `npm test` (syncLogic.test.mjs).
//
// The backend is the authority: it plans again on apply and refuses a plan that moved. What is
// worked out here only has to match what it will do, so the preview says the same thing.
import type { SyncAction, SyncItem, SyncPlan, SyncSettings } from "../../api/sync.ts";
import { libraryLabel, sameRoot } from "../../state/place.ts";
import type { LibraryKind } from "../../api/types.ts";

export const SAME = "already the same";
/** The engine keeps this much of the target free (ops_sync SPACE_MARGIN). */
export const SPACE_MARGIN = 1024 * 1024;
/** Rows drawn per group at first, and added by each "Show more". */
export const ROW_CAP = 150;
export const ROW_MORE = 500;

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** 1.2 GB, 340.5 MB, 12.0 KB, 80 B. */
export function sizeText(n: number): string {
  if (n >= 2 ** 30) return `${(n / 2 ** 30).toFixed(1)} GB`;
  if (n >= 2 ** 20) return `${(n / 2 ** 20).toFixed(1)} MB`;
  if (n >= 2 ** 10) return `${(n / 2 ** 10).toFixed(1)} KB`;
  return `${n} B`;
}

// ---- which libraries, which pair ---------------------------------------------------------------

/** `title` is the full path, shown as a tooltip. */
export type LibOption = { value: string; label: string; kind: LibraryKind; title?: string };

/** "This computer" (only when its library is there or is the open one), each detected Pacemaker, and
 *  the open library when it is neither (a folder chosen by hand). */
export function libraryOptions(
  computer: { path: string; exists: boolean } | null,
  volumes: string[],
  open: { root: string; kind: LibraryKind } | null,
): LibOption[] {
  const out: LibOption[] = [];
  const computerOpen = !!computer && !!open && sameRoot(open.root, computer.path);
  if (computer && (computer.exists || computerOpen)) out.push({ value: computer.path, label: libraryLabel("computer", computer.path, computer.path), kind: "computer", title: computer.path });
  for (const v of volumes) if (!out.some((o) => sameRoot(o.value, v))) out.push({ value: v, label: libraryLabel("pacemaker", v), kind: "pacemaker", title: v });
  if (open && !out.some((o) => sameRoot(o.value, open.root))) out.push({ value: open.root, label: libraryLabel(open.kind, open.root, computer?.path ?? null), kind: open.kind, title: open.root });
  return out;
}

export type Pair = { from: string; to: string };
export type Direction = "send" | "get";

const firstPacemaker = (opts: LibOption[]) => opts.find((o) => o.kind === "pacemaker")?.value ?? null;
const computerOf = (opts: LibOption[]) => opts.find((o) => o.kind === "computer")?.value ?? null;

/** send: this computer to a Pacemaker; get: a Pacemaker to this computer. `other` names the
 *  Pacemaker when the caller knows it; otherwise the one already chosen on a side, else the first. */
export function pairFor(dir: Direction, opts: LibOption[], current: Pair, other?: string | null): Pair {
  const computer = computerOf(opts);
  const chosen = [current.from, current.to].find((r) => opts.some((o) => o.kind === "pacemaker" && sameRoot(o.value, r)));
  const named = other ? opts.find((o) => sameRoot(o.value, other))?.value : null;
  const pm = named || chosen || firstPacemaker(opts);
  if (!computer || !pm) return current;
  return dir === "send" ? { from: computer, to: pm } : { from: pm, to: computer };
}

/** The pair the view starts with. A deep link ({direction, other}) wins; else the open computer
 *  library goes to the first Pacemaker, an open Pacemaker comes to the computer library, else the
 *  computer library goes to the first Pacemaker. Empty strings when there is no pair to offer. */
export function defaultPair(
  opts: LibOption[],
  open: { root: string; kind: LibraryKind } | null,
  link: { direction?: unknown; other?: unknown } = {},
): Pair {
  const none: Pair = { from: "", to: "" };
  if (link.direction === "send" || link.direction === "get") {
    const p = pairFor(link.direction, opts, none, typeof link.other === "string" ? link.other : null);
    if (p.from) return p;
  }
  const computer = computerOf(opts);
  const pm = firstPacemaker(opts);
  if (open && open.kind === "pacemaker" && computer) return { from: open.root, to: computer };
  if (computer && pm) return { from: computer, to: pm };
  return none;
}

/** Choosing `value` on one side. If it is the library already on the other side, that side moves
 *  to the next library on offer, so the two are never the same. */
export function pickSide(side: "from" | "to", value: string, pair: Pair, opts: LibOption[]): Pair {
  const next: Pair = { ...pair, [side]: value };
  const otherSide = side === "from" ? "to" : "from";
  if (next[otherSide] && sameRoot(next[otherSide], value)) {
    const old = pair[side];
    const spare = opts.find((o) => !sameRoot(o.value, value))?.value ?? "";
    next[otherSide] = old && !sameRoot(old, value) ? old : spare;
  }
  return next;
}

export const swapPair = (p: Pair): Pair => ({ from: p.to, to: p.from });

/** The two libraries are chosen and differ. */
export const pairReady = (p: Pair) => !!p.from && !!p.to && !sameRoot(p.from, p.to);

// ---- the preview -------------------------------------------------------------------------------

export type GroupId = "tracks" | "mixes" | "cases" | "removals";
export const GROUPS: { id: GroupId; title: string }[] = [
  { id: "tracks", title: "Tracks" },
  { id: "mixes", title: "Mixes" },
  { id: "cases", title: "Cases" },
  { id: "removals", title: "Removals" },
];

export const isRemoval = (i: SyncItem) => i.key.startsWith("remove-");
/** Something happens (a box to tick); the rest are skipped with a reason. */
export const isActionable = (i: SyncItem) => i.action.type !== "skip";
export const isSame = (i: SyncItem) => i.action.type === "skip" && (i.action as { reason: string }).reason === SAME;

export function groupOf(i: SyncItem): GroupId {
  if (isRemoval(i)) return "removals";
  return i.kind === "track" ? "tracks" : i.kind === "mix" ? "mixes" : "cases";
}

const rank = (i: SyncItem) => (isActionable(i) ? 0 : isSame(i) ? 2 : 1);

/** The items of each group, in the order shown: what happens first, then what is skipped for a
 *  reason, then what is already the same. The engine's order is kept within each. */
export function groupItems(items: SyncItem[]): Record<GroupId, SyncItem[]> {
  const out: Record<GroupId, SyncItem[]> = { tracks: [], mixes: [], cases: [], removals: [] };
  for (const i of items) out[groupOf(i)].push(i);
  for (const g of GROUPS) out[g.id] = out[g.id].map((it, n) => ({ it, n })).sort((a, b) => rank(a.it) - rank(b.it) || a.n - b.n).map((x) => x.it);
  return out;
}

export function actionText(a: SyncAction): string {
  switch (a.type) {
    case "copy": return "Copy";
    case "update": return "Update";
    case "update_plays": return "Update play counts";
    case "repair": return "Repair";
    case "replace": return "Replace";
    case "copy_as_new": return `Copy as "${a.name}"`;
    case "merge": return "Merge";
    case "skip": return `Skip: ${a.reason}`;
    case "remove": return "Remove";
  }
}

/** Why an item is acted on, in words, from the settings the plan was made with. */
export function reasonText(i: SyncItem, s: Omit<SyncSettings, "labels_follow">, sourceLabel: string): string {
  const a = i.action;
  if (a.type === "skip") return a.reason;
  if (a.type === "update_plays") return "played more in the sending library";
  // the engine's detail names which file; it is the whole reason, so the list does not repeat it
  if (a.type === "repair") return i.detail || "a file of the track is missing in the receiving library";
  if (a.type === "remove") return `not in ${sourceLabel === "computer" ? "the library on this computer" : "the Pacemaker's library"}`;
  if (i.kind === "track") {
    if (a.type === "copy") return "not in the receiving library yet";
    return s.tracks_on_both === "source" ? "the setting always takes the sending library's details" : "the sending library's details are newer";
  }
  if (i.kind === "mix") {
    if (a.type === "copy") return "no mix with this name there";
    if (a.type === "copy_as_new") return "a mix with this name is there and the setting keeps both";
    return s.mixes_same_name === "replace" ? "a mix with this name is there, and the setting always replaces it" : "a mix with this name is there and the sending library's is newer";
  }
  if (a.type === "copy") return "no case with this name there";
  if (a.type === "merge") return "a case with this name is there; the missing tracks are added";
  return "a case with this name is there, and the setting replaces it";
}

// ---- left-out tracks and what needs them ------------------------------------------------------

export type Mark = { leaves: string[]; skipped: boolean };

/** For each mix or case that will be done: the tracks it needs that are being copied but were left
 *  out. A mix with any of them is skipped (the engine does not copy a mix with a hole in it); a case
 *  is copied without them. Also a removal of a track that a case or mix still uses: if the thing
 *  that uses it is left out of the removals, the track stays. */
export function dependencyMarks(items: SyncItem[], excluded: ReadonlySet<string>): Map<string, Mark> {
  const copying = new Set(items.filter((i) => i.kind === "track" && !isRemoval(i) && i.action.type === "copy").map((i) => i.key));
  const out = new Map<string, Mark>();
  for (const i of items) {
    // a play-count update copies nothing, so it needs nothing
    if (!isActionable(i) || excluded.has(i.key) || i.action.type === "update_plays") continue;
    if (isRemoval(i) && i.kind === "track") {
      const keep = i.blockers.filter((b) => excluded.has(b.key)).map((b) => b.label);
      if (keep.length) out.set(i.key, { leaves: keep, skipped: true });
      continue;
    }
    if (i.kind === "track" || isRemoval(i)) continue;
    const gone = i.needs.filter((n) => copying.has(`track:${n.location}`) && excluded.has(`track:${n.location}`)).map((n) => n.label);
    if (gone.length) out.set(i.key, { leaves: gone, skipped: i.kind === "mix" });
  }
  return out;
}

export function markText(kind: SyncItem["kind"], isRemove: boolean, m: Mark): string {
  const names = m.leaves.join(", ");
  if (isRemove) return `Will stay: it is still used by ${names}, which you left out of the removals.`;
  if (kind === "mix") return `Will be skipped: it needs ${names}, which you left out.`;
  return `Will be copied without ${names}, which you left out.`;
}

// ---- totals ------------------------------------------------------------------------------------

export type Tally = {
  copyTracks: number; updateTracks: number; removeTracks: number;
  /** Tracks that are there but lack a file: only the missing file is copied. */
  repairTracks: number;
  /** Items that only bring their play counts over. */
  plays: number;
  mixes: number; cases: number; removeOthers: number;
  /** Skipped mixes (by the plan or by a left-out track) are not counted in `mixes`. */
  skippedMixes: number;
  bytes: number;
};

/** What a run of the shown plan does with the boxes as they are: the items ticked, minus the mixes a
 *  left-out track holds up. */
export function tally(items: SyncItem[], excluded: ReadonlySet<string>): Tally {
  const marks = dependencyMarks(items, excluded);
  const t: Tally = { copyTracks: 0, updateTracks: 0, removeTracks: 0, repairTracks: 0, plays: 0, mixes: 0, cases: 0, removeOthers: 0, skippedMixes: 0, bytes: 0 };
  for (const i of items) {
    if (!isActionable(i) || excluded.has(i.key)) continue;
    const m = marks.get(i.key);
    if (m?.skipped) {
      if (i.kind === "mix") t.skippedMixes++;
      continue;
    }
    if (i.action.type === "update_plays") t.plays++;
    else if (isRemoval(i)) {
      if (i.kind === "track") t.removeTracks++;
      else t.removeOthers++;
    } else if (i.kind === "track") {
      if (i.action.type === "copy") t.copyTracks++;
      else if (i.action.type === "repair") t.repairTracks++;
      else t.updateTracks++;
      t.bytes += i.bytes;
    } else if (i.kind === "mix") t.mixes++;
    else t.cases++;
  }
  return t;
}

export const doesAnything = (t: Tally) => t.copyTracks + t.updateTracks + t.repairTracks + t.plays + t.removeTracks + t.mixes + t.cases + t.removeOthers > 0;

export function tallyText(t: Tally): string {
  const p: string[] = [];
  if (t.copyTracks) p.push(`copy ${plural(t.copyTracks, "track")}`);
  if (t.updateTracks) p.push(`update ${plural(t.updateTracks, "track")}`);
  if (t.repairTracks) p.push(`copy the missing files of ${plural(t.repairTracks, "track")}`);
  if (t.plays) p.push(`update the play counts of ${plural(t.plays, "item")}`);
  if (t.mixes) p.push(`bring ${plural(t.mixes, "mix", "mixes")}`);
  if (t.cases) p.push(`bring ${plural(t.cases, "case")}`);
  const rm = t.removeTracks + t.removeOthers;
  if (rm) p.push(`remove ${plural(rm, "item")}`);
  if (p.length === 0) return "Nothing will be changed.";
  const s = `Will ${p.join(", ")}.`;
  return t.skippedMixes ? `${s} ${plural(t.skippedMixes, "mix", "mixes")} will be skipped.` : s;
}

export type SpaceVerdict = { tone: "ok" | "bad" | "muted"; text: string; fits: boolean };

/** The space line: what the ticked tracks need against what the target has free. "Does not fit"
 *  blocks the apply. */
export function spaceLine(plan: SyncPlan, bytes: number): SpaceVerdict {
  const free = plan.free_bytes;
  if (bytes === 0) return { tone: "muted", text: "No audio to copy.", fits: true };
  if (free === null) return { tone: "muted", text: `Needs ${sizeText(bytes)}. The free space could not be read.`, fits: true };
  if (bytes + SPACE_MARGIN > free) return { tone: "bad", text: `Does not fit: needs ${sizeText(bytes)}, ${sizeText(free)} free.`, fits: false };
  return { tone: "ok", text: `Needs ${sizeText(bytes)}, ${sizeText(free)} free.`, fits: true };
}

/** Items that a run would remove, in words, for the confirmation: every one named. */
export function removalList(items: SyncItem[], excluded: ReadonlySet<string>): { kind: string; label: string }[] {
  const marks = dependencyMarks(items, excluded);
  return items
    .filter((i) => isRemoval(i) && isActionable(i) && !excluded.has(i.key) && !marks.get(i.key)?.skipped)
    .map((i) => ({ kind: i.kind === "track" ? "Track" : i.kind === "mix" ? "Mix" : "Case", label: i.label }));
}

/** Keys to send to the backend: only items that are in the plan and actionable. */
export function excludedKeys(items: SyncItem[], excluded: ReadonlySet<string>): string[] {
  return items.filter((i) => isActionable(i) && excluded.has(i.key)).map((i) => i.key);
}

// ---- the result --------------------------------------------------------------------------------

export type ResultPart = { heading: string; items: string[]; tone: "ok" | "muted" | "bad" };

const PART_START = /\. (?=(?:Copied|Updated|Repaired|Replaced|Merged|Removed|Stopped|Tags and labels|The tags and labels|\d+ items? (?:left out|could not be synced)))/;

/** Splits the finished line the backend wrote ("Copied 4 tracks (35.5 MB). 1 item left out: a: b; c:
 *  d") into parts; a "left out" or "could not be synced" part becomes a heading plus one entry per item. */
export function resultParts(summary: string): ResultPart[] {
  return summary
    .split(PART_START)
    .map((s) => s.trim().replace(/\.$/, ""))
    .filter(Boolean)
    .map((s): ResultPart => {
      const m = /^(\d+ items? (?:left out|could not be synced)):\s*(.*)$/s.exec(s);
      if (m) return { heading: m[1], items: m[2].split("; ").filter(Boolean), tone: m[1].includes("could not") ? "bad" : "muted" };
      return { heading: s, items: [], tone: /^(The tags and labels did not|Stopped)/.test(s) ? "muted" : "ok" };
    });
}

/** The refusal the backend gives when either library moved since the preview. */
export const isStale = (msg: string) => /changed since the preview|stale plan/i.test(msg);
export const STALE_TEXT = "The libraries changed since you checked; check again.";
