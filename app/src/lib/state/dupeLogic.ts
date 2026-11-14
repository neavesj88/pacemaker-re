// Pure logic of the duplicate finder's screens: how a group's tracks are compared field by field,
// the sentences that say why they are a group and what a merge will do, and the request the
// review builds from the user's choices. Types only are imported, so `dupeLogic.test.mjs` runs
// it with plain node.
import type { FindResult, Group, GroupPlan, Link, Member, MergePlan, MergeRequest } from "$lib/api/duplicates";
import type { DupSettings } from "$lib/api/duplicates";

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export function fmtBytes(b: number): string {
  if (b >= 1 << 30) return `${(b / (1 << 30)).toFixed(1)} GB`;
  if (b >= 1 << 20) return `${(b / (1 << 20)).toFixed(1)} MB`;
  if (b >= 1024) return `${Math.round(b / 1024)} KB`;
  return `${b} B`;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** Unix milliseconds as a UTC date (the same everywhere), or `never` for the "no play" marker. */
export function fmtDate(ms: number, never = "never"): string {
  if (ms <= 0) return never;
  const d = new Date(ms);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
}

export const fmtLength = (secs: number) => (secs > 0 ? `${Math.floor(secs / 60)}:${pad(Math.floor(secs % 60))}` : "–");

// ---- the field-by-field comparison --------------------------------------------------------------

export type Field = { key: string; label: string; get: (m: Member) => string };

const text = (v: string) => v.trim() || "–";
const num = (n: number, unit = "") => (n > 0 ? `${n}${unit}` : "–");

export const FIELDS: Field[] = [
  { key: "title", label: "Title", get: (m) => text(m.title) },
  { key: "artist", label: "Artist", get: (m) => text(m.artist) },
  { key: "album_artist", label: "Album artist", get: (m) => text(m.album_artist) },
  { key: "album", label: "Album", get: (m) => text(m.album) },
  { key: "genre", label: "Genre", get: (m) => text(m.genre) },
  { key: "label", label: "Label", get: (m) => text(m.label) },
  { key: "year", label: "Year", get: (m) => num(m.year) },
  { key: "key", label: "Key", get: (m) => text(m.key) },
  { key: "bpm", label: "BPM", get: (m) => (m.bpm > 0 ? m.bpm.toFixed(2) : "–") },
  { key: "length", label: "Length", get: (m) => fmtLength(m.play_time_secs) },
  { key: "format", label: "Format", get: (m) => text(m.format) },
  { key: "bitrate", label: "Bitrate", get: (m) => num(m.bit_rate_kbps, " kbps") },
  { key: "sample_rate", label: "Sample rate", get: (m) => (m.sample_rate > 0 ? `${(m.sample_rate / 1000).toFixed(1)} kHz` : "–") },
  { key: "size", label: "File size", get: (m) => (m.file_size > 0 ? fmtBytes(m.file_size) : "–") },
  { key: "rating", label: "Rating", get: (m) => (m.rating > 0 ? plural(m.rating, "star") : "unrated") },
  { key: "plays", label: "Plays", get: (m) => String(m.times_played) },
  { key: "last_played", label: "Last played", get: (m) => fmtDate(m.last_played) },
  { key: "added", label: "Added", get: (m) => fmtDate(m.date_added, "–") },
  { key: "composer", label: "Composer", get: (m) => text(m.composer) },
  { key: "producer", label: "Producer", get: (m) => text(m.producer) },
  { key: "remixer", label: "Remixer", get: (m) => text(m.remixer) },
  { key: "comments", label: "Comments", get: (m) => text(m.comments) },
  { key: "track_number", label: "Track no.", get: (m) => num(m.track_number) },
  { key: "disc_number", label: "Disc no.", get: (m) => num(m.disc_number) },
  { key: "analysed", label: "Beat grid", get: (m) => (m.analysed ? "analysed" : "not analysed") },
  { key: "cases", label: "In cases", get: (m) => (m.cases.length ? m.cases.map((c) => `${c.name} (${c.position} of ${c.of})`).join(", ") : "none") },
  { key: "mixes", label: "In mixes", get: (m) => (m.mixes.length ? m.mixes.join(", ") : "none") },
  { key: "file", label: "File", get: (m) => FILE_TEXT[m.file] },
  { key: "location", label: "Location", get: (m) => text(m.location) },
];

const FILE_TEXT: Record<Member["file"], string> = {
  present: "present",
  missing: "missing",
  outside: "outside the music folder",
};

export type FieldRow = { key: string; label: string; values: string[] };

/** Every field split into those that differ between the members and those that read the same in
 *  all of them (shown once, so nothing is hidden, only folded). */
export function compareMembers(members: Member[]): { differ: FieldRow[]; same: { label: string; value: string }[] } {
  const differ: FieldRow[] = [];
  const same: { label: string; value: string }[] = [];
  for (const f of FIELDS) {
    const values = members.map(f.get);
    if (values.every((v) => v === values[0])) same.push({ label: f.label, value: values[0] ?? "–" });
    else differ.push({ key: f.key, label: f.label, values });
  }
  return { differ, same };
}

// ---- why they are a group ------------------------------------------------------------------------

export function reasonLines(group: Pick<Group, "links">): string[] {
  const out: string[] = [];
  if (group.links.some((l) => l.mode === "tags")) out.push("same artist and title");
  if (group.links.some((l) => l.mode === "size")) out.push("same file size and length");
  const audio = group.links.filter((l) => l.mode === "audio");
  if (audio.length) {
    const best = Math.max(...audio.map((l) => l.score ?? 0));
    const lo = Math.min(...audio.map((l) => l.score ?? 0));
    out.push(audio.length > 1 && lo !== best ? `sound ${pct(lo)}–${pct(best)} alike` : `sound ${pct(best)} alike`);
  }
  return out;
}

const pct = (s: number) => `${Math.floor(s * 100 + 1e-6)}%`;

/** How a member is tied to the rest, for its column: the strongest reason first. */
export function memberLinks(group: Pick<Group, "links">, id: number): Link[] {
  return group.links.filter((l) => l.a === id || l.b === id);
}

// ---- the user's choices ---------------------------------------------------------------------------

export type Choice = {
  /** The copy to keep. */
  keeper: number;
  /** Tracks the user says are not copies of this group: left out of the merge. */
  excluded: number[];
  /** Whether the group is part of the merge. */
  picked: boolean;
};

export function defaultChoices(result: FindResult | null): Record<number, Choice> {
  const out: Record<number, Choice> = {};
  for (const g of result?.groups ?? []) out[g.id] = { keeper: g.suggested_keeper, excluded: [], picked: true };
  return out;
}

/** The tracks a merge of this group would fold into the keeper. */
export function copiesOf(group: Group, choice: Choice): number[] {
  return group.members.map((m) => m.track_id).filter((id) => id !== choice.keeper && !choice.excluded.includes(id));
}

/** A group can be merged when it has a keeper among its tracks and at least one copy left. */
export function mergeable(group: Group, choice: Choice | undefined): boolean {
  return !!choice && !choice.excluded.includes(choice.keeper) && group.members.some((m) => m.track_id === choice.keeper) && copiesOf(group, choice).length > 0;
}

export function requestFor(result: FindResult, choices: Record<number, Choice>, removeRows: boolean, deleteFiles: boolean, settings: DupSettings): MergeRequest {
  const groups = result.groups
    .filter((g) => choices[g.id]?.picked && mergeable(g, choices[g.id]))
    .map((g) => ({ keeper: choices[g.id].keeper, others: copiesOf(g, choices[g.id]) }));
  return { groups, remove_rows: removeRows, delete_files: removeRows && deleteFiles, settings };
}

/** Picked groups that can actually be merged. */
export function pickedCount(result: FindResult | null, choices: Record<number, Choice>): number {
  return (result?.groups ?? []).filter((g) => choices[g.id]?.picked && mergeable(g, choices[g.id])).length;
}

/** Choosing a different keeper: it must not stay excluded, and the old keeper is a copy again. */
export function withKeeper(c: Choice, id: number): Choice {
  return { ...c, keeper: id, excluded: c.excluded.filter((x) => x !== id) };
}

/** Marking a track as "not a copy" (or putting it back). The keeper cannot be marked. */
export function withExcluded(c: Choice, id: number, excluded: boolean): Choice {
  if (id === c.keeper) return c;
  const rest = c.excluded.filter((x) => x !== id);
  return { ...c, excluded: excluded ? [...rest, id] : rest };
}

// ---- what a merge will do, in words ----------------------------------------------------------------

export type PlanCopy = { track_id: number; label: string; parts: string[] };
export type PlanGroup = { keeper: string; stats: string | null; copies: PlanCopy[] };

const CASE_TEXT = {
  replace: (n: string) => `its place in “${n}” goes to the kept track`,
  already_there: (n: string) => `leaves “${n}” (the kept track is there already)`,
  drop: (n: string) => `leaves “${n}”`,
} as const;

function statsLine(p: GroupPlan): string | null {
  const parts: string[] = [];
  const { rating, times_played, last_played } = p.stats;
  if (rating.before !== rating.after) parts.push(`rating ${rating.before} → ${rating.after}`);
  if (times_played.before !== times_played.after) parts.push(`plays ${times_played.before} → ${times_played.after}`);
  if (last_played.before !== last_played.after) parts.push(`last played ${fmtDate(last_played.before)} → ${fmtDate(last_played.after)}`);
  return parts.length ? `The kept track gets: ${parts.join(", ")}.` : null;
}

/** One entry per group: the kept track, what its statistics become, and for each copy every
 *  change to it (cases, the library row, each file), so the confirmation names all of them. */
export function planSentences(plan: MergePlan): PlanGroup[] {
  const label = new Map(plan.labels.map((l) => [l.track_id, l.label] as const));
  const name = (id: number) => (label.has(id) ? `${label.get(id)} (#${id})` : `track ${id}`);
  return plan.groups.map((g) => ({
    keeper: name(g.keeper),
    stats: statsLine(g),
    copies: g.others.map((id) => {
      const parts: string[] = [];
      for (const e of g.case_edits) if (e.others.includes(id)) parts.push(CASE_TEXT[e.kind](e.case_name));
      const blocked = g.blocked.find((b) => b.track_id === id);
      if (blocked) parts.push(`stays in the library: ${blocked.mixes.length === 1 ? "the mix" : "the mixes"} ${blocked.mixes.map((m) => `“${m}”`).join(", ")} ${blocked.mixes.length === 1 ? "uses" : "use"} it`);
      else if (g.remove.some((r) => r.track_id === id)) parts.push("removed from the library");
      else parts.push("stays in the library");
      for (const f of plan.files.filter((x) => x.track_id === id)) {
        const what = f.kind === "audio" ? "audio file" : "beat grid file";
        if (f.kept_because) parts.push(`keeps ${what} ${f.path} (${f.kept_because})`);
        else if (!f.exists) parts.push(`${what} ${f.path} is missing`);
        else parts.push(`deletes ${what} ${f.path} (${fmtBytes(f.size)})`);
      }
      return { track_id: id, label: name(id), parts };
    }),
  }));
}

/** The totals line of the confirmation. */
export function totalsLine(plan: MergePlan): string {
  const t = plan.totals;
  const parts = [plural(t.groups, "group"), `${plural(t.case_entries, "case place")} moved`];
  if (t.rows_removed) parts.push(`${plural(t.rows_removed, "copy", "copies")} removed from the library`);
  if (t.files_deleted) parts.push(`${plural(t.files_deleted, "file")} deleted (${fmtBytes(t.bytes)})`);
  return parts.join(" · ");
}

/** The text of the confirm button: it says what will be done to files. */
export function mergeButtonLabel(plan: MergePlan | null, removeRows: boolean, deleteFiles: boolean): string {
  if (!plan) return "Merge";
  const n = plan.totals.groups;
  if (removeRows && deleteFiles) return `Merge ${plural(n, "group")} and delete ${plural(plan.totals.files_deleted, "file")}`;
  if (removeRows) return `Merge ${plural(n, "group")} and remove ${plural(plan.totals.rows_removed, "copy", "copies")}`;
  return `Merge ${plural(n, "group")}`;
}

/** The settings summary under the toolbar: which ways are on. */
export function waysLine(s: DupSettings): string {
  const on = [s.by_tags && "artist and title", s.by_size && "file size and length", s.by_audio && "sound"].filter(Boolean);
  return on.length ? `Looking by ${on.join(", ")}` : "No way of finding copies is switched on";
}
