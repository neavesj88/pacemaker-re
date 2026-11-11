// Pure helpers of the clean-up tool: the text forms of the editable lists and maps, which
// preview rows are ticked, and how ticked changes become the requests the backend takes.
// No $lib imports (the node test loads this file directly); types only come from the api module.
import type { AcceptedChange, CleanupPreview, MapEntry, TagItem, TagPlan } from "../../api/metacleanup";

/** One entry per non-empty line, trimmed, without repeats (compared ignoring case). */
export function parseList(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const line of text.split(/\r?\n/)) {
    const t = line.trim();
    if (t && !seen.has(t.toLowerCase())) {
      seen.add(t.toLowerCase());
      out.push(t);
    }
  }
  return out;
}

export const formatList = (list: string[]): string => list.join("\n");

const ARROWS = ["->", "=>", "→"];

/** "from -> to" per line; `=>` and the arrow character work too. Returns the entries and how many
 *  non-empty lines had no arrow (they are ignored, and the editor says so). */
export function parseMap(text: string): { entries: MapEntry[]; ignored: number } {
  const entries: MapEntry[] = [];
  let ignored = 0;
  const seen = new Set<string>();
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let at = -1;
    let len = 0;
    for (const a of ARROWS) {
      const i = line.indexOf(a);
      if (i >= 0 && (at < 0 || i < at)) {
        at = i;
        len = a.length;
      }
    }
    if (at < 0) {
      ignored++;
      continue;
    }
    const from = line.slice(0, at).trim();
    const to = line.slice(at + len).trim();
    if (!from || !to) {
      ignored++;
      continue;
    }
    // the first line for a name wins, like the backend's lookup
    if (seen.has(from.toLowerCase())) continue;
    seen.add(from.toLowerCase());
    entries.push({ from, to });
  }
  return { entries, ignored };
}

export const formatMap = (map: MapEntry[]): string => map.map((e) => `${e.from} -> ${e.to}`).join("\n");

/** Key of one previewed change; ticks are kept by it across recomputes. */
export const tickKey = (trackId: number, field: string) => `${trackId}:${field}`;

/** The previewed changes the user left ticked (and that can be applied at all). */
export function acceptedOf(preview: CleanupPreview | null, unticked: Record<string, boolean>): AcceptedChange[] {
  if (!preview) return [];
  const out: AcceptedChange[] = [];
  for (const r of preview.rows) {
    for (const c of r.changes) {
      if (c.problem || unticked[tickKey(r.track_id, c.field)]) continue;
      out.push({ track_id: r.track_id, field: c.field, old: c.old, new: c.new });
    }
  }
  return out;
}

/** Accepted changes as per-track values to write into the files' tags. */
export function itemsOf(accepted: { track_id: number; field: string; new: string }[]): TagItem[] {
  const by = new Map<number, TagItem>();
  for (const a of accepted) {
    let item = by.get(a.track_id);
    if (!item) {
      item = { track_id: a.track_id };
      by.set(a.track_id, item);
    }
    (item as Record<string, unknown>)[a.field] = a.new;
  }
  return [...by.values()];
}

/** The tag-write plans the user left ticked, as items (only plans that would write something,
 *  only their ticked fields). */
export function itemsFromPlans(plans: TagPlan[], unticked: Record<string, boolean>, numeric: ReadonlySet<string>): TagItem[] {
  const out: TagItem[] = [];
  for (const p of plans) {
    if (p.status !== "write") continue;
    const item: TagItem = { track_id: p.track_id };
    let any = false;
    for (const c of p.changes) {
      if (unticked[tickKey(p.track_id, c.field)]) continue;
      (item as Record<string, unknown>)[c.field] = numeric.has(c.field) ? Number(c.new) : c.new;
      any = true;
    }
    if (any) out.push(item);
  }
  return out;
}

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** Counts of a preview with some changes unticked. */
export function tally(preview: CleanupPreview | null, unticked: Record<string, boolean>) {
  let tracks = 0;
  let fields = 0;
  let blocked = 0;
  for (const r of preview?.rows ?? []) {
    let any = false;
    for (const c of r.changes) {
      if (c.problem) {
        blocked++;
        continue;
      }
      if (unticked[tickKey(r.track_id, c.field)]) continue;
      fields++;
      any = true;
    }
    if (any) tracks++;
  }
  return { tracks, fields, blocked };
}

/** The tag fields that are numbers (the others are text). */
export const NUMERIC_TAG_FIELDS: ReadonlySet<string> = new Set(["year", "track_number", "number_of_tracks", "disc_number", "number_of_discs"]);

/** Where two strings differ: the shared start and end, and what is left in the middle of each.
 *  The table highlights the middle parts, so a change of white space is visible. */
export function diffParts(a: string, b: string): { pre: string; del: string; add: string; post: string } {
  const x = Array.from(a);
  const y = Array.from(b);
  let i = 0;
  while (i < x.length && i < y.length && x[i] === y[i]) i++;
  let j = 0;
  while (j < x.length - i && j < y.length - i && x[x.length - 1 - j] === y[y.length - 1 - j]) j++;
  return {
    pre: x.slice(0, i).join(""),
    del: x.slice(i, x.length - j).join(""),
    add: y.slice(i, y.length - j).join(""),
    post: x.slice(x.length - j).join(""),
  };
}

/** Values of the library to write into the files' tags, from the library's own rows: the chosen
 *  fields only; an empty text or a 0 is left out (a blank never erases a tag). */
export function itemsFromDetails(details: Record<string, unknown>[], fields: string[]): TagItem[] {
  const out: TagItem[] = [];
  for (const d of details) {
    const item: Record<string, unknown> = { track_id: d.track_id };
    let any = false;
    for (const f of fields) {
      const v = d[f];
      if (NUMERIC_TAG_FIELDS.has(f) ? typeof v === "number" && v > 0 : typeof v === "string" && v !== "") {
        item[f] = v;
        any = true;
      }
    }
    if (any) out.push(item as TagItem);
  }
  return out;
}

/** The track ids the preview rows are about, in order, once each. */
export const idsOf = (list: { track_id: number }[]): number[] => [...new Set(list.map((x) => x.track_id))];

/** Fields that were accepted for each track: `{id: ["title", ...]}`. */
export function fieldsByTrack(accepted: { track_id: number; field: string }[]): Map<number, string[]> {
  const m = new Map<number, string[]>();
  for (const a of accepted) {
    const l = m.get(a.track_id) ?? [];
    if (!l.includes(a.field)) l.push(a.field);
    m.set(a.track_id, l);
  }
  return m;
}
