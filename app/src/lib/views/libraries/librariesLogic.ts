// The pure rules of the Libraries view: what each pane lists and how it is searched, the "on both"
// markers, how clicks build a selection, what a drag carries, where a drop lands, and the words of
// the confirm dialog. Run by plain node in `npm test` (librariesLogic.test.mjs).
//
// The backend is the authority: `sync_plan_selection` decides what a copy does and the apply runs
// that plan or refuses it. What is worked out here only chooses the selection that goes to it and
// says what came back.
import type { Collection, LibraryKind, Track } from "../../api/types.ts";
import type { SyncItem, SyncPlan, SyncSelection } from "../../api/sync.ts";
import { libraryKind, libraryLabel, sameRoot } from "../../state/place.ts";
import { plural, reasonText, SAME, sizeText, spaceLine, type LibOption, type SpaceVerdict } from "../sync/syncLogic.ts";

export type Side = "left" | "right";
export type Tab = "tracks" | "mixes" | "cases";
export const TABS: { id: Tab; label: string }[] = [
  { id: "tracks", label: "Tracks" },
  { id: "mixes", label: "Mixes" },
  { id: "cases", label: "Cases" },
];

export const otherSide = (s: Side): Side => (s === "left" ? "right" : "left");

/** What a pane lists of one library. */
export type PaneData = {
  root: string;
  kind: LibraryKind;
  tracks: readonly Track[];
  mixes: readonly Collection[];
  cases: readonly Collection[];
};

export type Row = Track | Collection;

/** The id of a row inside its library: `track_id` for a track, `id` for a mix or case. */
export const rowId = (tab: Tab, row: Row): number => (tab === "tracks" ? (row as Track).track_id : (row as Collection).id);

/** Names compare the way the sync engine compares them: trimmed and case-folded. */
export const norm = (name: string) => name.trim().toLowerCase();

/** 30421 -> "30,421". */
export const num = (n: number) => String(Math.trunc(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");

const WORD: Record<Tab, [string, string]> = { tracks: ["track", "tracks"], mixes: ["mix", "mixes"], cases: ["case", "cases"] };
export const wordFor = (tab: Tab, n: number) => WORD[tab][n === 1 ? 0 : 1];
/** "3 tracks", "1 mix", "2 cases". */
export const countText = (tab: Tab, n: number) => `${num(n)} ${wordFor(tab, n)}`;

/** 215 -> "3:35", 3725 -> "1:02:05", a length that is not there -> "". */
export function clock(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "";
  const s = Math.round(secs);
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(r)}` : `${m}:${two(r)}`;
}

// ---- listing and search ------------------------------------------------------------------------

const hay = new WeakMap<object, string>();
function trackHay(t: Track): string {
  let h = hay.get(t);
  if (h === undefined) {
    h = `${t.title} ${t.artist} ${t.album} ${t.genre} ${t.key} ${t.label}`.toLowerCase();
    hay.set(t, h);
  }
  return h;
}

/** The rows a pane shows for `query`: every word must be somewhere in the track (title, artist,
 *  album, genre, key, label) or in the mix or case name. An empty query is the list itself. */
export function rowsOf(data: PaneData, tab: Tab, query: string): readonly Row[] {
  const list: readonly Row[] = data[tab];
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return list;
  if (tab === "tracks") return (list as readonly Track[]).filter((t) => words.every((w) => trackHay(t).includes(w)));
  return (list as readonly Collection[]).filter((c) => words.every((w) => c.name.toLowerCase().includes(w)));
}

// ---- "on both" ---------------------------------------------------------------------------------

/** What a library holds, to tell the other pane which of its rows are also here: track locations as
 *  they are, mix and case names normalised. */
export type Presence = { tracks: ReadonlySet<string>; mixes: ReadonlySet<string>; cases: ReadonlySet<string> };

export function presenceOf(data: PaneData | null): Presence {
  return {
    tracks: new Set((data?.tracks ?? []).map((t) => t.location)),
    mixes: new Set((data?.mixes ?? []).map((m) => norm(m.name))),
    cases: new Set((data?.cases ?? []).map((c) => norm(c.name))),
  };
}

/** Whether the other library has this row too (the same place for a track, the same name for a mix or case). */
export function onBoth(tab: Tab, row: Row, there: Presence | null): boolean {
  if (!there) return false;
  if (tab === "tracks") return there.tracks.has((row as Track).location);
  return there[tab].has(norm((row as Collection).name));
}

// ---- selection ---------------------------------------------------------------------------------

export type Selected = Record<Tab, ReadonlySet<number>>;
export const emptySelected = (): Selected => ({ tracks: new Set(), mixes: new Set(), cases: new Set() });

/** A click on the row at `index` of the visible `ids`: plain selects just it, Ctrl/Cmd toggles it,
 *  Shift selects from the anchor (added to the rest when Ctrl is also held). */
export function clickSelect(
  current: ReadonlySet<number>,
  anchor: number | null,
  ids: readonly number[],
  index: number,
  mods: { shift: boolean; ctrl: boolean },
): { selected: Set<number>; anchor: number | null } {
  const id = ids[index];
  if (id === undefined) return { selected: new Set(current), anchor };
  if (mods.shift) {
    const a = anchor === null ? -1 : ids.indexOf(anchor);
    const from = a < 0 ? index : a;
    const [lo, hi] = from <= index ? [from, index] : [index, from];
    const range = ids.slice(lo, hi + 1);
    return { selected: new Set(mods.ctrl ? [...current, ...range] : range), anchor: anchor !== null && a >= 0 ? anchor : id };
  }
  if (mods.ctrl) {
    const next = new Set(current);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return { selected: next, anchor: id };
  }
  return { selected: new Set([id]), anchor: id };
}

/** The selection with the rows that are not in the list any more (a search, a reload) taken out. */
export function pruneTo(selected: ReadonlySet<number>, visible: readonly number[]): Set<number> {
  const here = new Set(visible);
  return new Set([...selected].filter((id) => here.has(id)));
}

/** What a drag that starts on `pressed` carries: the whole selection when the pressed row is in it,
 *  else just that row (the press selects it). Ids are in list order. */
export function dragIds(selected: ReadonlySet<number>, pressed: number, visible: readonly number[]): number[] {
  if (!selected.has(pressed)) return [pressed];
  return visible.filter((id) => selected.has(id));
}

/** The words on the drag pill: "3 tracks", "1 mix", "2 cases". */
export const dragLabel = (tab: Tab, n: number) => countText(tab, n);

/** The selection the backend plans: track locations, mix names or case names of the chosen rows, in
 *  the library's order. `into` names a case on the other side the dragged tracks should end up in. */
export function buildSelection(data: PaneData, tab: Tab, ids: Iterable<number>, into: { name: string; create: boolean } | null = null): SyncSelection {
  const want = new Set(ids);
  const sel: SyncSelection = { tracks: [], mixes: [], cases: [], into_case: null };
  if (tab === "tracks") {
    for (const t of data.tracks) if (want.has(t.track_id)) sel.tracks.push(t.location);
    if (into && sel.tracks.length > 0) sel.into_case = { name: into.name, create: into.create };
  } else {
    for (const c of data[tab]) if (want.has(c.id)) sel[tab].push(c.name);
  }
  return sel;
}

export const selectionSize = (s: SyncSelection) => s.tracks.length + s.mixes.length + s.cases.length;

// ---- drop --------------------------------------------------------------------------------------

export type DropInput = {
  fromSide: Side;
  fromTab: Tab;
  /** The pane the pointer was released over. */
  overSide: Side;
  /** The tab that pane is showing. */
  overTab: Tab;
  /** The name of the case row under the pointer (only a row in a Cases tab has one), else null. */
  overCase: string | null;
};

/** Where a drop lands: nothing for the pane the drag came from; the library of the pane it was
 *  released over; and, for tracks released on a case row of a Cases tab, that case too. */
export function resolveDrop(d: DropInput): { into: { name: string; create: boolean } | null } | null {
  if (d.fromSide === d.overSide) return null;
  if (d.fromTab === "tracks" && d.overTab === "cases" && d.overCase) return { into: { name: d.overCase, create: false } };
  return { into: null };
}

/** The `from` name of a drag: `lib:<side>:<tab>`. */
export const dragFrom = (side: Side, tab: Tab) => `lib:${side}:${tab}`;

/** Reads a `from` name back; null for any other drag. */
export function parseDragFrom(from: string): { side: Side; tab: Tab } | null {
  const m = /^lib:(left|right):(tracks|mixes|cases)$/.exec(from);
  return m ? { side: m[1] as Side, tab: m[2] as Tab } : null;
}

// ---- the click route into a case --------------------------------------------------------------------

/** The longest case name the library takes (the engine refuses a longer one). */
export const MAX_CASE_NAME = 255;
/** The choice in the case picker that means "make a new case". */
export const NEW_CASE = "\u0000new";

/** "Copy into a case on Pacemaker (G:)…": the row menu item and the button for tracks. */
export const intoCaseLabel = (targetName: string) => `Copy into a case on ${targetName}…`;

/** What the case picker starts on: the other library's first case, or a new one when it has none. */
export const defaultCasePick = (cases: readonly Collection[]): string => cases[0]?.name ?? NEW_CASE;

/** What the picker's choice means for the plan: an existing case of the other library, or a new
 *  case with the typed name. A typed name that is already a case there (compared as the engine
 *  does) is that case, not a second one. The plan and the dialog that follow are the ones a drop on a
 *  case row gives. */
export function resolveIntoCase(
  cases: readonly Collection[],
  pick: string,
  typed: string,
): { into: { name: string; create: boolean } } | { error: string } {
  if (pick !== NEW_CASE) {
    const c = cases.find((x) => norm(x.name) === norm(pick));
    return c ? { into: { name: c.name, create: false } } : { error: "That case is not on the other library any more. Pick another." };
  }
  const name = typed.trim();
  if (!name) return { error: "Type a name for the new case." };
  if (name.length > MAX_CASE_NAME) return { error: `A case name can have at most ${MAX_CASE_NAME} characters.` };
  const same = cases.find((x) => norm(x.name) === norm(name));
  return { into: same ? { name: same.name, create: false } : { name, create: true } };
}

// ---- which library on the right ------------------------------------------------------------------

/** The libraries the right pane can show: all on offer except the open one. */
export const rightOptions = (options: LibOption[], open: { root: string } | null) => options.filter((o) => !open || !sameRoot(o.value, open.root));

/** The library the right pane starts with, or keeps: `current` while it is still on offer, else the
 *  detected Pacemaker when a computer library is open, the computer library when a Pacemaker is. */
export function defaultRight(options: LibOption[], open: { root: string; kind: LibraryKind } | null, current: string): string {
  const cand = rightOptions(options, open);
  if (current && cand.some((o) => sameRoot(o.value, current))) return current;
  // nothing open: the library on this computer is safe to show; a Pacemaker waits for the user
  if (!open) return cand.find((o) => o.kind === "computer")?.value ?? "";
  const want: LibraryKind = open.kind === "pacemaker" ? "computer" : "pacemaker";
  return (cand.find((o) => o.kind === want) ?? cand[0])?.value ?? "";
}

/** Whether choosing `root` for the right pane reads it at once. A detected Pacemaker (a volume that was
 *  not added by hand with "Open folder…") is read only when the user presses the button (`readNow`),
 *  because its drive may be ageing; anything on this computer, and a folder chosen by hand, is read
 *  as soon as it is chosen. */
export function readsAtOnce(root: string, volumes: readonly string[], folders: readonly string[], readNow: boolean): boolean {
  if (!root) return false;
  if (readNow) return true;
  const detected = volumes.some((v) => sameRoot(v, root)) && !folders.some((f) => sameRoot(f, root));
  return !detected;
}

/** "Read the Pacemaker (G:)" for the Pacemaker whose root is given. */
export const readButtonText = (root: string) => `Read the ${libraryLabel("pacemaker", root)}`;

/** The label of a copy button or menu item: "Copy to Pacemaker (G:) →" on the left pane (the other
 *  side is to the right), "← Copy to This computer" on the right. */
export const copyLabel = (side: Side, targetName: string) => (side === "left" ? `Copy to ${targetName} →` : `← Copy to ${targetName}`);

// ---- the status line -----------------------------------------------------------------------------

/** "3 selected · 120 of 30,421 tracks" (the filter part only while a search narrows the list). */
export function statusText(tab: Tab, selected: number, shown: number, total: number): string {
  const what = shown === total ? countText(tab, total) : `${num(shown)} of ${countText(tab, total)}`;
  return selected > 0 ? `${num(selected)} selected · ${what}` : what;
}

// ---- the confirm dialog ----------------------------------------------------------------------------

export type Line = {
  key: string;
  kind: SyncItem["kind"];
  name: string;
  /** "Copy", "Update", "Skip", "Add to case Crate", ... */
  action: string;
  /** The tracks an "Add to case" row puts in the case (their names), else empty. */
  names: string[];
  /** The reason: "Needed by mix "Set"", "already there", ... */
  why: string;
  /** Something happens to the target. */
  acts: boolean;
  /** Skipped because it is already there. */
  same: boolean;
  bytes: number;
};

const ACTION: Record<string, string> = {
  copy: "Copy",
  update: "Update",
  update_plays: "Update play counts",
  repair: "Repair",
  replace: "Replace",
  merge: "Merge",
  remove: "Remove",
};

/** One row of the dialog: the item's name, what is done and why, each in words. */
export function lineOf(i: SyncItem, plan: SyncPlan): Line {
  const a = i.action;
  const base = { key: i.key, kind: i.kind, name: i.label, bytes: i.bytes, names: [] as string[] };
  if (a.type === "skip") {
    const same = a.reason === SAME;
    return { ...base, action: "Skip", why: same ? "already there" : a.reason, acts: false, same };
  }
  if (i.into_case !== null) {
    // the label says it all ("Add 2 tracks to case X"); the row says the action first and the rest after it
    const rest = a.type === "copy" ? i.label.replace(/^New case /, "") : i.label.replace(/^Add (.*) to case .*$/s, "$1");
    return { ...base, name: rest, names: i.needs.map((n) => n.label), action: a.type === "copy" ? "New case" : `Add to case ${i.into_case}`, why: i.detail, acts: true, same: false };
  }
  const action = a.type === "copy_as_new" ? `Copy as "${a.name}"` : (ACTION[a.type] ?? a.type);
  let why = "";
  if (i.needed_by.length > 0) why = `Needed by ${i.needed_by.join(", ")}`;
  else if (a.type !== "copy" || i.kind !== "track") why = reasonText(i, plan.settings, plan.source_label);
  if (i.detail && a.type !== "update_plays" && a.type !== "repair") why = why ? `${why} (${i.detail})` : i.detail;
  return { ...base, action, why, acts: true, same: false };
}

export type Lines = { acting: Line[]; skipped: Line[]; same: Line[] };

/** The plan's items as dialog rows: what happens first, then what is skipped for a reason, then what
 *  is already there. The plan's order is kept within each. */
export function linesOf(plan: SyncPlan): Lines {
  const out: Lines = { acting: [], skipped: [], same: [] };
  for (const i of plan.items) {
    const l = lineOf(i, plan);
    (l.acts ? out.acting : l.same ? out.same : out.skipped).push(l);
  }
  return out;
}

/** A library's name inside a sentence: "this computer", "Pacemaker (G:)". */
export const sentenceName = (name: string) => name.replace(/^This computer/, "this computer");

const lowerFirst = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);

/** What the copy will do, in one sentence: "Will copy 3 tracks, bring 1 mix, add 3 tracks to case X." */
export function copySummary(plan: SyncPlan): string {
  let copy = 0, update = 0, repair = 0, plays = 0, mixes = 0, cases = 0;
  const into: string[] = [];
  for (const i of plan.items) {
    if (i.action.type === "skip") continue;
    if (i.into_case !== null) {
      into.push(i.action.type === "copy" ? `make ${lowerFirst(i.label)}` : lowerFirst(i.label));
    } else if (i.action.type === "update_plays") plays++;
    else if (i.action.type === "repair") repair++;
    else if (i.kind === "track") i.action.type === "copy" ? copy++ : update++;
    else if (i.kind === "mix") mixes++;
    else cases++;
  }
  const p: string[] = [];
  if (copy) p.push(`copy ${plural(copy, "track")}`);
  if (update) p.push(`update ${plural(update, "track")}`);
  if (repair) p.push(`copy the missing files of ${plural(repair, "track")}`);
  if (plays) p.push(`update the play counts of ${plural(plays, "item")}`);
  if (mixes) p.push(`bring ${plural(mixes, "mix", "mixes")}`);
  if (cases) p.push(`bring ${plural(cases, "case")}`);
  p.push(...into);
  if (p.length > 0) return `Will ${p.join(", ")}.`;
  // only say "already there" when that is the reason for every row; the rows give the others
  const allThere = plan.items.every((i) => i.action.type === "skip" && i.action.reason === SAME);
  return allThere ? "Nothing will be copied: all of it is there already." : "Nothing will be copied.";
}

/** The dialog's closing line: what a copy leaves alone on the target, said from the plan. A drag only
 *  adds, so this is the usual line; if a plan ever did replace or update something it names that
 *  instead of claiming nothing changes. */
export function footerText(plan: SyncPlan, targetName: string): string {
  const where = sentenceName(targetName);
  const changes = plan.items.some((i) => i.action.type === "replace" || i.action.type === "update");
  const plays = plan.items.some((i) => i.action.type === "update_plays");
  const head = changes
    ? `This changes mixes, cases or track details that are already on ${where}.`
    : `Nothing on ${where} is removed or overwritten.${plays ? " Play counts only go up." : ""}`;
  return `${head} A copy of the database on ${where} is saved before the first change.`;
}

/** Whether there is anything to run and it fits. */
export const canCopy = (plan: SyncPlan) => plan.items.some((i) => i.action.type !== "skip") && spaceLine(plan, plan.bytes_to_copy).fits;

export const dialogSpace = (plan: SyncPlan): SpaceVerdict => spaceLine(plan, plan.bytes_to_copy);

/** How a library root is named in a sentence: "This computer", "Pacemaker (G:)". */
export const libraryNameFor = (root: string, computerRoot?: string | null) => libraryLabel(libraryKind(root), root, computerRoot);

/** "Copy to Pacemaker (G:)?" */
export const dialogTitle = (targetName: string) => `Copy to ${targetName}?`;

/** The size of the audio to copy, for the dialog's heading line. */
export const totalSize = (plan: SyncPlan) => (plan.bytes_to_copy > 0 ? sizeText(plan.bytes_to_copy) : "no audio");

/** The refusal to say in words when the run was refused because a picked item moved. */
export const STALE_COPY_TEXT = "Something you picked changed while the list was open, so nothing was copied. Copy it again to see the new list.";
