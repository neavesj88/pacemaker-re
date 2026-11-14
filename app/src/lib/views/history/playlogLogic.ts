// Pure helpers of the Play log section: the period picker, grouping sessions under dates, and the
// wording of how sure a derived play is. No Svelte here so they can be tested with node.

export const DAY_MS = 86_400_000;

export type Period = "7" | "30" | "90" | "all" | "custom";

export const PERIODS: { id: Period; label: string }[] = [
  { id: "7", label: "Last 7 days" },
  { id: "30", label: "Last 30 days" },
  { id: "90", label: "Last 90 days" },
  { id: "all", label: "Everything logged" },
  { id: "custom", label: "Custom dates" },
];

/** "2023-11-08" for the computer's local calendar day of `ms`. */
export function localDayKey(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Local midnight at the start of a "YYYY-MM-DD" day, or null if it is not a real date. */
export function dayStart(key: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(key.trim());
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const date = new Date(y, mo - 1, d);
  return date.getFullYear() === y && date.getMonth() === mo - 1 && date.getDate() === d ? date.getTime() : null;
}

export type Range = { from: number | null; to: number | null; error: string };

/** The Unix ms bounds of a period. Rolling presets end now; custom dates are whole local days
 *  (the end date included). A bad or backwards custom range gives `error` and no bounds. */
export function periodRange(period: Period, now: number, customFrom = "", customTo = ""): Range {
  if (period === "all") return { from: null, to: null, error: "" };
  if (period !== "custom") return { from: now - Number(period) * DAY_MS, to: null, error: "" };
  const from = customFrom ? dayStart(customFrom) : null;
  const toStart = customTo ? dayStart(customTo) : null;
  if (customFrom && from === null) return { from: null, to: null, error: "The start date is not a valid date." };
  if (customTo && toStart === null) return { from: null, to: null, error: "The end date is not a valid date." };
  // End of the chosen local day, found as the start of the next one so a 23 or 25 hour day is right.
  let to: number | null = null;
  if (toStart !== null) {
    const next = new Date(toStart);
    next.setDate(next.getDate() + 1);
    to = next.getTime() - 1;
  }
  if (from !== null && to !== null && from > to) return { from: null, to: null, error: "The start date is after the end date." };
  return { from, to, error: "" };
}

/** Words for the period, for headings and the case source sentence. */
export function periodLabel(period: Period, customFrom = "", customTo = ""): string {
  if (period === "all") return "everything logged";
  if (period !== "custom") return `the last ${period} days`;
  if (customFrom && customTo) return `${customFrom} to ${customTo}`;
  return customFrom ? `since ${customFrom}` : customTo ? `up to ${customTo}` : "everything logged";
}

export type DayGroup<T> = { key: string; start: number; sessions: T[]; plays: number };

/** Groups sessions (kept in the order given) under the local day each one started on. A session
 *  that runs past midnight stays with the day it started, which is how a night out reads. */
export function groupByDay<T extends { start: number; plays: number }>(sessions: T[]): DayGroup<T>[] {
  const out: DayGroup<T>[] = [];
  for (const s of sessions) {
    const key = localDayKey(s.start);
    const last = out[out.length - 1];
    if (last && last.key === key) {
      last.sessions.push(s);
      last.plays += s.plays;
    } else out.push({ key, start: s.start, sessions: [s], plays: s.plays });
  }
  return out;
}

type HitLike = { plays: number; approx: boolean; time_known: boolean; reset: boolean };

/** Says in words why a derived play is not exact; "" when it is. */
export function hitNote(h: HitLike): string {
  const parts: string[] = [];
  if (!h.time_known) {
    parts.push("No newer last-played time came with the plays, so this is when the library was read");
  } else if (h.plays > 1) {
    parts.push(`${h.plays} plays were seen at once; only the latest time is known, the others were earlier`);
  } else if (h.approx) {
    parts.push("Inferred from the last-played time alone (the play count did not change)");
  }
  if (h.reset) parts.push("the play counter had been reset, so it counts from zero");
  const text = parts.join("; ");
  return text ? text + "." : "";
}

/** "12 KB" style size for the log file. */
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10240 ? 1 : 0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** "keeps 730 days" / "keeps everything". */
export function retentionLabel(days: number): string {
  return days <= 0 ? "keeps everything" : `keeps ${days} day${days === 1 ? "" : "s"}`;
}
