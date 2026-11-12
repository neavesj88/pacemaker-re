// Pure logic of tags, colour labels and energy ratings: no store, component or `$lib` imports, so
// it runs under plain node (`npm test`) and any area can reuse it. The labels themselves are
// app-side data (the Pacemaker has no place for them); see state/labels.svelte.ts for the store
// that holds them and commands/tags.rs for the file they are kept in.
//
// `applyOps` is a mirror of the Rust `apply_ops` (commands/tags/store.rs). The UI uses it to
// word a change before it is made; the same vectors in labels.parity.json are run against both,
// so the preview and the action cannot drift apart. The backend stays the authority: the UI
// always shows what it returns.

// ---- colours and limits ------------------------------------------------------------------

/** The eight colour labels, in palette order (the CSS tokens are --label-<name>). */
export const LABEL_COLOURS = ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"] as const;
export type LabelColour = (typeof LABEL_COLOURS)[number];

export const isColour = (c: unknown): c is LabelColour => typeof c === "string" && (LABEL_COLOURS as readonly string[]).includes(c);
export const colourName = (c: LabelColour): string => c[0].toUpperCase() + c.slice(1);
/** The CSS value for a colour label; use as `background` or `color`. */
export const colourVar = (c: LabelColour): string => `var(--label-${c})`;

export const MIN_ENERGY = 1;
export const MAX_ENERGY = 10;
/** Longest tag, in characters. */
export const MAX_TAG_CHARS = 40;
/** Most tags one track can carry. */
export const MAX_TAGS = 32;

/** The 1 to 10 scale, for buttons and selects. */
export const ENERGY_LEVELS: readonly number[] = Array.from({ length: MAX_ENERGY }, (_, i) => i + 1);

// ---- the data ----------------------------------------------------------------------------

export type TrackLabels = { tags: string[]; colour: LabelColour | null; energy: number | null };

export const NO_LABELS: Readonly<TrackLabels> = Object.freeze({ tags: [], colour: null, energy: null });

export const labelsEmpty = (l: TrackLabels | undefined): boolean => !l || (l.tags.length === 0 && l.colour === null && l.energy === null);

/** One change, in the shape the backend takes. */
export type LabelOp =
  | { op: "add_tags"; tags: string[] }
  | { op: "remove_tags"; tags: string[] }
  | { op: "set_colour"; colour: LabelColour | null }
  | { op: "set_energy"; energy: number | null };

// ---- tags --------------------------------------------------------------------------------

/** The key two tags are compared by: case does not matter. */
export const tagKey = (tag: string): string => tag.toLowerCase();

export type TagResult = { ok: true; tag: string } | { ok: false; error: string };

// Unicode White_Space, the set Rust's split_whitespace uses (JS \s differs at U+0085 and U+FEFF).
const WHITESPACE = /[\t\n\u000b\f\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+/;

/** A tag as it is stored: trimmed, inner whitespace collapsed to single spaces. Commas are the
 *  separator when tags are typed, so a tag cannot contain one. */
export function normaliseTag(raw: string): TagResult {
  if (/[,\u0000-\u0008\u000e-\u001f\u007f-\u0084\u0086-\u009f]/.test(raw)) {
    return { ok: false, error: `The tag "${raw.trim()}" contains a comma or a control character.` };
  }
  const tag = raw.split(WHITESPACE).filter(Boolean).join(" ");
  if (!tag) return { ok: false, error: "A tag cannot be empty." };
  if ([...tag].length > MAX_TAG_CHARS) return { ok: false, error: `The tag "${tag}" is longer than ${MAX_TAG_CHARS} characters.` };
  return { ok: true, tag };
}

/** Splits typed or pasted text ("a, b\nc") into tags; `errors` lists what was refused. Empty
 *  pieces are ignored, repeats (ignoring case) are kept once. */
export function parseTagInput(text: string): { tags: string[]; errors: string[] } {
  const tags: string[] = [];
  const errors: string[] = [];
  const seen = new Set<string>();
  for (const piece of text.split(/[,\n;]/)) {
    if (!piece.trim()) continue;
    const r = normaliseTag(piece);
    if (!r.ok) errors.push(r.error);
    else if (!seen.has(tagKey(r.tag))) {
      seen.add(tagKey(r.tag));
      tags.push(r.tag);
    }
  }
  return { tags, errors };
}

const byKey = (a: string, b: string): number => {
  const x = tagKey(a);
  const y = tagKey(b);
  return x < y ? -1 : x > y ? 1 : a < b ? -1 : a > b ? 1 : 0;
};

const hasTag = (tags: readonly string[], key: string): boolean => tags.some((t) => tagKey(t) === key);

// ---- applying changes (mirror of Rust apply_ops) -----------------------------------------

export type ApplyResult = { ok: true; labels: TrackLabels } | { ok: false; error: string };

/** The labels after `ops`, or why they cannot be applied. Same rules, same order, as the backend. */
export function applyOps(before: TrackLabels, ops: readonly LabelOp[]): ApplyResult {
  const l: TrackLabels = { tags: [...before.tags], colour: before.colour, energy: before.energy };
  for (const op of ops) {
    switch (op.op) {
      case "add_tags":
        for (const raw of op.tags) {
          const r = normaliseTag(raw);
          if (!r.ok) return r;
          if (!hasTag(l.tags, tagKey(r.tag))) {
            l.tags.push(r.tag);
            l.tags.sort(byKey);
          }
        }
        if (l.tags.length > MAX_TAGS) return { ok: false, error: `A track can have at most ${MAX_TAGS} tags.` };
        break;
      case "remove_tags": {
        const gone = new Set<string>();
        for (const raw of op.tags) {
          const r = normaliseTag(raw);
          if (r.ok) gone.add(tagKey(r.tag));
        }
        l.tags = l.tags.filter((t) => !gone.has(tagKey(t)));
        break;
      }
      case "set_colour":
        if (op.colour !== null && !isColour(op.colour)) return { ok: false, error: `"${op.colour}" is not one of the colour labels.` };
        l.colour = op.colour;
        break;
      case "set_energy":
        if (op.energy !== null && !(Number.isInteger(op.energy) && op.energy >= MIN_ENERGY && op.energy <= MAX_ENERGY)) {
          return { ok: false, error: `Energy is ${MIN_ENERGY} to ${MAX_ENERGY}.` };
        }
        l.energy = op.energy;
        break;
    }
  }
  return { ok: true, labels: l };
}

// ---- vocabulary --------------------------------------------------------------------------

export type TagCount = { tag: string; count: number };

/** Every tag in use with the number of tracks that carry it, sorted by name. When one tag is
 *  spelled with different casings (only a hand-edited file can do that) the most used casing is
 *  shown, then the alphabetically first. */
export function tagVocabulary(all: Iterable<TrackLabels>): TagCount[] {
  const byLower = new Map<string, { count: number; spellings: Map<string, number> }>();
  for (const l of all) {
    for (const t of l.tags) {
      const k = tagKey(t);
      const e = byLower.get(k) ?? { count: 0, spellings: new Map() };
      e.count++;
      e.spellings.set(t, (e.spellings.get(t) ?? 0) + 1);
      byLower.set(k, e);
    }
  }
  return [...byLower.entries()]
    .map(([, e]) => ({
      tag: [...e.spellings.entries()].sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))[0][0],
      count: e.count,
    }))
    .sort((a, b) => byKey(a.tag, b.tag));
}

/** The spelling the library already uses for `tag`, or `tag` itself. A new tag typed as "house"
 *  becomes "House" when the library says House. */
export function canonicalTag(vocabulary: readonly TagCount[], tag: string): string {
  const k = tagKey(tag);
  return vocabulary.find((v) => tagKey(v.tag) === k)?.tag ?? tag;
}

/** Tags to offer while typing: those starting with the text first, then those containing it;
 *  tags in `exclude` (by key) are left out. */
export function suggestTags(vocabulary: readonly TagCount[], typed: string, exclude: ReadonlySet<string> = new Set(), limit = 8): TagCount[] {
  const q = tagKey(typed.trim());
  const open = vocabulary.filter((v) => !exclude.has(tagKey(v.tag)));
  if (!q) return open.slice(0, limit);
  const starts = open.filter((v) => tagKey(v.tag).startsWith(q));
  const contains = open.filter((v) => !tagKey(v.tag).startsWith(q) && tagKey(v.tag).includes(q));
  return [...starts, ...contains].slice(0, limit);
}

/** Ids of the tracks carrying `tag` (ignoring case). The tag manager lists exactly these before
 *  a delete or merge, and the counts next to each tag are their number. */
export function tracksWithTag(entries: Iterable<readonly [number, TrackLabels]>, tag: string): number[] {
  const k = tagKey(tag);
  const out: number[] = [];
  for (const [id, l] of entries) if (hasTag(l.tags, k)) out.push(id);
  return out;
}

/** The labels of removed tracks that carry `tag` (ignoring case). Delete and merge change these
 *  too, so the tag manager's confirmation counts and lists them next to the tracks it names. */
export function orphansWithTag(orphans: readonly TrackLabels[], tag: string): TrackLabels[] {
  const k = tagKey(tag);
  return orphans.filter((l) => hasTag(l.tags, k));
}

/** One line naming a removed track's labels, for the "forget" confirmation (the track itself is
 *  gone, so its labels are all that identify it): `opener, old crate · Teal · energy 4`. */
export function describeOrphan(l: TrackLabels): string {
  const parts: string[] = [];
  if (l.tags.length) parts.push(l.tags.join(", "));
  if (l.colour) parts.push(colourName(l.colour));
  if (l.energy !== null) parts.push(`energy ${l.energy}`);
  return parts.join(" · ") || "no labels";
}

// ---- filtering ---------------------------------------------------------------------------

export type LabelFilter = {
  tags: string[];
  /** "any": the track has at least one of the tags; "all": it has every one. */
  tagMode: "any" | "all";
  /** The track's colour is one of these. Empty = no colour condition. */
  colours: LabelColour[];
  /** Energy within [min, max], either end optional; a track with no energy never matches. */
  energyMin: number | null;
  energyMax: number | null;
};

export const NO_LABEL_FILTER: Readonly<LabelFilter> = Object.freeze({ tags: [], tagMode: "any", colours: [], energyMin: null, energyMax: null });

export const freshLabelFilter = (): LabelFilter => ({ tags: [], tagMode: "any", colours: [], energyMin: null, energyMax: null });

export const labelFilterActive = (f: LabelFilter): boolean => f.tags.length > 0 || f.colours.length > 0 || f.energyMin !== null || f.energyMax !== null;

/** Does a track with these labels (undefined = none) pass the filter? */
export function matchesLabels(l: TrackLabels | undefined, f: LabelFilter): boolean {
  const labels = l ?? NO_LABELS;
  if (f.tags.length > 0) {
    const mine = new Set(labels.tags.map(tagKey));
    const want = f.tags.map(tagKey);
    if (!(f.tagMode === "all" ? want.every((k) => mine.has(k)) : want.some((k) => mine.has(k)))) return false;
  }
  if (f.colours.length > 0 && !(labels.colour !== null && f.colours.includes(labels.colour))) return false;
  if (f.energyMin !== null || f.energyMax !== null) {
    if (labels.energy === null) return false;
    if (f.energyMin !== null && labels.energy < f.energyMin) return false;
    if (f.energyMax !== null && labels.energy > f.energyMax) return false;
  }
  return true;
}

/** `rows` that pass the filter, in their order. Returns `rows` itself when no filter is set. */
export function filterByLabels<T extends { track_id: number }>(rows: T[], f: LabelFilter, get: (id: number) => TrackLabels | undefined): T[] {
  return labelFilterActive(f) ? rows.filter((t) => matchesLabels(get(t.track_id), f)) : rows;
}

/** A readable one-line description of the filter, for the "no tracks match" hint and tooltips. */
export function describeLabelFilter(f: LabelFilter): string {
  const parts: string[] = [];
  if (f.tags.length) parts.push(`${f.tagMode === "all" ? "all of" : "any of"} tags ${f.tags.map((t) => `“${t}”`).join(", ")}`);
  if (f.colours.length) parts.push(`colour ${f.colours.map(colourName).join(" or ")}`);
  if (f.energyMin !== null || f.energyMax !== null) parts.push(`energy ${f.energyMin ?? MIN_ENERGY} to ${f.energyMax ?? MAX_ENERGY}`);
  return parts.join("; ");
}

// ---- sorting -----------------------------------------------------------------------------

/** Sort position of a colour: palette order; none last. */
export const colourRank = (c: LabelColour | null): number => (c === null ? 999 : LABEL_COLOURS.indexOf(c));

// ---- editing drafts ----------------------------------------------------------------------

/** What the user has chosen in an editor so far, before it is saved. `undefined` = leave alone;
 *  `null` = clear. Add / remove hold tags to add to and remove from every target. */
export type LabelDraft = { add: string[]; remove: string[]; colour: LabelColour | null | undefined; energy: number | null | undefined };

export const emptyDraft = (): LabelDraft => ({ add: [], remove: [], colour: undefined, energy: undefined });

export const draftIsEmpty = (d: LabelDraft): boolean => d.add.length === 0 && d.remove.length === 0 && d.colour === undefined && d.energy === undefined;

/** Queue a tag to be added. A tag queued for removal is un-queued instead. */
export function draftAddTag(d: LabelDraft, tag: string): void {
  const k = tagKey(tag);
  d.remove = d.remove.filter((t) => tagKey(t) !== k);
  if (!hasTag(d.add, k)) d.add.push(tag);
}

/** Queue a tag to be removed. A tag queued for adding is un-queued instead. */
export function draftRemoveTag(d: LabelDraft, tag: string): void {
  const k = tagKey(tag);
  d.add = d.add.filter((t) => tagKey(t) !== k);
  if (!hasTag(d.remove, k)) d.remove.push(tag);
}

/** Take a queued tag back (add or remove) so the tag is left as it was. */
export function draftForgetTag(d: LabelDraft, tag: string): void {
  const k = tagKey(tag);
  d.add = d.add.filter((t) => tagKey(t) !== k);
  d.remove = d.remove.filter((t) => tagKey(t) !== k);
}

/** The ops a draft stands for, in the order the backend applies them: removals, additions,
 *  colour, energy. Empty when nothing is queued. */
export function draftOps(d: LabelDraft): LabelOp[] {
  const ops: LabelOp[] = [];
  if (d.remove.length) ops.push({ op: "remove_tags", tags: [...d.remove] });
  if (d.add.length) ops.push({ op: "add_tags", tags: [...d.add] });
  if (d.colour !== undefined) ops.push({ op: "set_colour", colour: d.colour });
  if (d.energy !== undefined) ops.push({ op: "set_energy", energy: d.energy });
  return ops;
}

/** How many of the targets carry each tag, sorted by name: the chips of an editor. */
export function tagPresence(targets: readonly TrackLabels[]): { tag: string; have: number }[] {
  const m = new Map<string, { tag: string; have: number }>();
  for (const l of targets) {
    for (const t of l.tags) {
      const e = m.get(tagKey(t)) ?? { tag: t, have: 0 };
      e.have++;
      m.set(tagKey(t), e);
    }
  }
  return [...m.values()].sort((a, b) => byKey(a.tag, b.tag));
}

/** The value all targets share, or "mixed". */
export function commonValue<T>(values: readonly T[]): { kind: "same"; value: T } | { kind: "mixed" } {
  if (values.length === 0) return { kind: "mixed" };
  return values.every((v) => v === values[0]) ? { kind: "same", value: values[0] } : { kind: "mixed" };
}

const noun = (n: number) => `${n} track${n === 1 ? "" : "s"}`;

/** The draft in words, one line per change, with exact counts for these targets. Counts come
 *  from `applyOps`, the same rules the backend runs. */
export function describeChanges(d: LabelDraft, targets: readonly TrackLabels[]): string[] {
  const n = targets.length;
  const changes = (op: LabelOp) => targets.filter((l) => {
    const r = applyOps(l, [op]);
    return r.ok && (r.labels.colour !== l.colour || r.labels.energy !== l.energy || r.labels.tags.join("\u0000") !== l.tags.join("\u0000"));
  }).length;
  const lines: string[] = [];
  for (const t of d.remove) {
    const c = changes({ op: "remove_tags", tags: [t] });
    lines.push(c === 0 ? `Remove tag “${t}”: none of the ${noun(n)} has it` : `Remove tag “${t}” from ${c} of ${noun(n)}`);
  }
  for (const t of d.add) {
    const c = changes({ op: "add_tags", tags: [t] });
    lines.push(c === 0 ? `Add tag “${t}”: ${n === 1 ? "the track has" : `all ${n} tracks have`} it already` : `Add tag “${t}” to ${c} of ${noun(n)}`);
  }
  if (d.colour !== undefined) {
    const c = changes({ op: "set_colour", colour: d.colour });
    lines.push(d.colour === null ? `Clear the colour on ${c} of ${noun(n)}` : `Set colour ${colourName(d.colour)} on ${c} of ${noun(n)}`);
  }
  if (d.energy !== undefined) {
    const c = changes({ op: "set_energy", energy: d.energy });
    lines.push(d.energy === null ? `Clear the energy on ${c} of ${noun(n)}` : `Set energy ${d.energy} on ${c} of ${noun(n)}`);
  }
  return lines;
}

/** Does the draft change anything for these targets? A draft that only repeats what they have
 *  is not worth a write. */
export function draftChangesAnything(d: LabelDraft, targets: readonly TrackLabels[]): boolean {
  const ops = draftOps(d);
  return ops.length > 0 && targets.some((l) => {
    const r = applyOps(l, ops);
    return r.ok && (r.labels.colour !== l.colour || r.labels.energy !== l.energy || r.labels.tags.join("\u0000") !== l.tags.join("\u0000"));
  });
}
