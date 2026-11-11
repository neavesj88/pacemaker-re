// Pure helpers of the device area's trust pack (restore, verify, fixes, undo). No imports from
// the app, so `node --experimental-strip-types` runs them in trustLogic.test.mjs.

/** The counts of a snapshot-vs-card comparison (mirrors api/device-trust.ts DiffCounts). */
export type Counts = {
  tracks_returning: number;
  tracks_lost: number;
  tracks_changed: number;
  cases_returning: number;
  cases_lost: number;
  cases_changed: number;
  mixes_returning: number;
  mixes_lost: number;
  mixes_changed: number;
};

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "20261111T094512Z" as a Date, or null if the text is not in that shape. */
export function stampToDate(stamp: string): Date | null {
  const m = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/.exec(stamp);
  if (!m) return null;
  const [y, mo, d, h, mi, s] = m.slice(1).map(Number);
  if (mo < 1 || mo > 12 || d < 1 || d > 31 || h > 23 || mi > 59 || s > 59) return null;
  const date = new Date(Date.UTC(y, mo - 1, d, h, mi, s));
  // 31 February rolls over into March: not a real date
  return date.getUTCDate() === d ? date : null;
}

/** "11 Nov 2026, 09:45 UTC": fixed text, the same on every machine. */
export function stampLabel(stamp: string): string {
  const d = stampToDate(stamp);
  if (!d) return stamp || "unknown time";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}, ${p(d.getUTCHours())}:${p(d.getUTCMinutes())} UTC`;
}

/** "3 minutes ago", "2 days ago": how long before `now` (both in ms). */
export function ageText(thenMs: number, nowMs: number): string {
  const secs = Math.max(0, Math.round((nowMs - thenMs) / 1000));
  if (secs < 45) return "just now";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins} minute${mins === 1 ? "" : "s"} ago`;
  const hours = Math.round(mins / 60);
  if (hours < 36) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.round(hours / 24);
  return `${days} day${days === 1 ? "" : "s"} ago`;
}

const n = (count: number, one: string, many = `${one}s`) => `${count} ${count === 1 ? one : many}`;

/** What a restore changes, in plain sentences, most consequential first. Empty when the snapshot
 *  and the card hold the same library. */
export function diffSentences(c: Counts): string[] {
  const out: string[] = [];
  if (c.tracks_lost) out.push(`${n(c.tracks_lost, "track")} added since will leave the library (their files stay on the card)`);
  if (c.tracks_returning) out.push(`${n(c.tracks_returning, "track")} will come back`);
  if (c.tracks_changed) out.push(`${n(c.tracks_changed, "track")} will get their earlier tags, rating or key back`);
  if (c.cases_lost) out.push(`${n(c.cases_lost, "case")} made since will go`);
  if (c.cases_returning) out.push(`${n(c.cases_returning, "case")} will come back`);
  if (c.cases_changed) out.push(`${n(c.cases_changed, "case")} will go back to their earlier contents`);
  if (c.mixes_lost) out.push(`${n(c.mixes_lost, "mix", "mixes")} made since will go`);
  if (c.mixes_returning) out.push(`${n(c.mixes_returning, "mix", "mixes")} will come back`);
  if (c.mixes_changed) out.push(`${n(c.mixes_changed, "mix", "mixes")} will go back to their earlier version`);
  return out;
}

/** One line for a snapshot row: the first sentences, or "same as the card now". */
export function diffSummary(c: Counts | null): string {
  if (!c) return "";
  const s = diffSentences(c);
  if (s.length === 0) return "Same as the card now";
  return s.length > 2 ? `${s.slice(0, 2).join("; ")}; and more` : s.join("; ");
}

/** The words for a restore's names list heading. */
export const LIST_HEADINGS = {
  tracks_lost: "Tracks that will leave the library",
  tracks_returning: "Tracks that will come back",
  tracks_changed: "Tracks that will change back",
  cases_lost: "Cases that will go",
  cases_returning: "Cases that will come back",
  cases_changed: "Cases that will change back",
  mixes_lost: "Mixes that will go",
  mixes_returning: "Mixes that will come back",
  mixes_changed: "Mixes that will change back",
} as const;

export type ListKey = keyof typeof LIST_HEADINGS;
export const LIST_ORDER: ListKey[] = [
  "tracks_lost", "tracks_returning", "tracks_changed",
  "cases_lost", "cases_returning", "cases_changed",
  "mixes_lost", "mixes_returning", "mixes_changed",
];

/** "and 120 more" under a capped list. */
export function moreText(total: number, shown: number): string {
  return total > shown ? `and ${total - shown} more not listed here` : "";
}

// ---- verify a backup ----

export type Status = "ok" | "differs" | "missing" | "unreadable" | "unsafe" | "bad_line";

export const STATUS_TEXT: Record<Status, string> = {
  ok: "Matches",
  differs: "Changed or damaged",
  missing: "Missing",
  unreadable: "Cannot be read",
  unsafe: "Unsafe path, not opened",
  bad_line: "Not a manifest line",
};

/** How many rows have each status. */
export function statusCounts(files: { status: Status }[]): Record<Status, number> {
  const out: Record<Status, number> = { ok: 0, differs: 0, missing: 0, unreadable: 0, unsafe: 0, bad_line: 0 };
  for (const f of files) out[f.status]++;
  return out;
}

/** The rows to show: only the ones that are not fine, or all of them. */
export function visibleChecks<T extends { status: Status }>(files: T[], problemsOnly: boolean): T[] {
  return problemsOnly ? files.filter((f) => f.status !== "ok") : files;
}

/** The verdict line above the per-file list. */
export function verifyVerdict(r: { listed: number; ok: number; files: { status: Status }[]; extra: string[] }): string {
  const problems = r.files.filter((f) => f.status !== "ok").length;
  const extra = r.extra.length ? ` ${n(r.extra.length, "file")} in the folder ${r.extra.length === 1 ? "is" : "are"} not in the manifest.` : "";
  if (problems === 0) return `${r.listed === 1 ? "The file matches its checksum." : `All ${r.listed} files match their checksums.`}${extra}`;
  return `${problems} of ${n(r.listed, "file")} ${problems === 1 ? "does" : "do"} not match.${extra}`;
}

// ---- Health card fixes ----

type Row = { label: string; in_cases: string[]; in_mixes: string[] };

/** One dead row as the confirmation names it. */
export function deadRowLine(r: Row): string {
  return r.in_cases.length ? `${r.label} (also removed from ${r.in_cases.join(", ")})` : r.label;
}

/** The warning above a removal that looks like a half-mounted card, or "". */
export function massWarning(plan: { mass: boolean; dead_rows: unknown[]; kept_for_mixes: unknown[]; total_tracks: number; guard_percent: number }): string {
  if (!plan.mass) return "";
  const dead = plan.dead_rows.length + plan.kept_for_mixes.length;
  return `${dead} of ${plan.total_tracks} tracks look dead, more than ${plan.guard_percent}% of the library. That usually means the card is not fully mounted or the wrong drive is open, not that the library is full of dead rows. Check the drive before removing anything.`;
}

// ---- undo ----

/** "Rated X 4 stars (was unrated). Undo" pill text, trimmed to a sane length. */
export function pillText(summary: string): string {
  return summary.length > 90 ? `${summary.slice(0, 87)}…` : summary;
}

/** How a plan line reads in the preview: "Rating: 3 stars → 5 stars". */
export function planLine(i: { what: string; current: string; becomes: string; skip: string | null }): string {
  return i.skip ? `${i.what}: ${i.skip}` : `${i.what}: ${i.current} → ${i.becomes}`;
}

/** What a row removal did, in one sentence for the toast: what went, what was left alone and why. */
export function removalReport(r: { removed: string[]; skipped: string[]; cases_touched: number }): string {
  const rows = `${n(r.removed.length, "row")} removed from the library`;
  const cases = r.cases_touched ? ` and ${n(r.cases_touched, "case")} updated` : "";
  const skipped = r.skipped.length ? ` ${n(r.skipped.length, "row")} left alone: ${r.skipped.join("; ")}.` : "";
  return `${rows}${cases}. A copy of music.db was saved first; Restore from a snapshot brings them back.${skipped}`;
}
