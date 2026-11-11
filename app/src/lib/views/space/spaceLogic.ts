// Pure helpers of the free-space planner (device area). No imports from the app (type imports
// only, which are erased), so `node --experimental-strip-types` runs them in spaceLogic.test.mjs.
import type { SpaceHidden, SpaceItem, SpacePlan, SpaceRunItem, SpaceRunPlan } from "../../api/device-space";

/** The wording the Storage card uses: binary units, "GB". */
export function bytesText(n: number): string {
  if (n >= 2 ** 30) return `${(n / 2 ** 30).toFixed(1)} GB`;
  if (n >= 2 ** 20) return `${(n / 2 ** 20).toFixed(1)} MB`;
  if (n >= 2 ** 10) return `${(n / 2 ** 10).toFixed(1)} KB`;
  return `${n} B`;
}

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

const MONTH_MS = 2_629_800_000;
const DAY_MS = 86_400_000;

/** "never", "today", "3 days ago", "7 months ago", "2 years ago". */
export function lastPlayedText(ms: number | null, now: number): string {
  if (ms === null) return "never";
  const age = Math.max(0, now - ms);
  if (age < DAY_MS) return "today";
  if (age < 60 * DAY_MS) return `${plural(Math.floor(age / DAY_MS), "day")} ago`;
  if (age < 24 * MONTH_MS) return `${plural(Math.floor(age / MONTH_MS), "month")} ago`;
  return `${plural(Math.floor(age / (12 * MONTH_MS)), "year")} ago`;
}

export type Totals = {
  count: number;
  bytes: number;
  /** Ticked tracks that leave the card if the user removes (a mix does not use them). */
  removeCount: number;
  removeBytes: number;
  blockedCount: number;
};

export function tickTotals(items: readonly SpaceItem[], ticked: ReadonlySet<number>): Totals {
  const t: Totals = { count: 0, bytes: 0, removeCount: 0, removeBytes: 0, blockedCount: 0 };
  for (const i of items) {
    if (!ticked.has(i.track_id)) continue;
    t.count++;
    t.bytes += i.bytes;
    if (i.blocked) t.blockedCount++;
    else {
      t.removeCount++;
      t.removeBytes += i.bytes;
    }
  }
  return t;
}

/** The ids the planner suggests to reach the target. */
export const suggestedIds = (items: readonly SpaceItem[]): Set<number> => new Set(items.filter((i) => i.suggested).map((i) => i.track_id));

/** The ticks that are still in a new list (a re-plan after a setting changed keeps the user's choices). */
export function keepTicks(ticked: ReadonlySet<number>, items: readonly SpaceItem[]): Set<number> {
  const ids = new Set(items.map((i) => i.track_id));
  return new Set([...ticked].filter((id) => ids.has(id)));
}

export type Verdict = { tone: "ok" | "warn" | "bad"; text: string };

/** One line on whether the ticked tracks reach the target. */
export function verdict(plan: SpacePlan, totals: Totals): Verdict {
  if (!plan.rules_on) return { tone: "warn", text: "No rule is on, so nothing is suggested. Turn on at least one reason." };
  if (plan.need_bytes === 0) return { tone: "ok", text: "The target is already met. You can still archive or remove tracks." };
  const short = plan.need_bytes - totals.removeBytes;
  if (short <= 0) return { tone: "ok", text: `Enough: removing the ticked tracks reaches the target (${bytesText(totals.removeBytes - plan.need_bytes)} to spare).` };
  if (!plan.reachable) {
    return {
      tone: "bad",
      text: `The rules cannot reach the target: all ${bytesText(plan.removable_bytes)} of removable candidates still leave ${bytesText(plan.need_bytes - plan.removable_bytes)} short. Loosen a rule or lower the target.`,
    };
  }
  return { tone: "warn", text: `${bytesText(short)} more to tick to reach the target.` };
}

/** The target in words: what is wanted and how far off the card is. */
export function goalLine(plan: SpacePlan, kind: "free" | "card"): string {
  if (kind === "card") {
    return `The library takes ${bytesText(plan.library_bytes)}; the card allows ${bytesText(plan.usable_bytes ?? 0)}. ${
      plan.need_bytes > 0 ? `${bytesText(plan.need_bytes)} must go.` : "It already fits."
    }`;
  }
  return `${bytesText(plan.free_bytes)} free of ${bytesText(plan.total_bytes)}. ${
    plan.need_bytes > 0 ? `${bytesText(plan.need_bytes)} more must be freed.` : "The target is met."
  }`;
}

/** What the free space (free target) or the library size (card target) becomes after removing `removeBytes`. */
export function afterLine(plan: SpacePlan, kind: "free" | "card", removeBytes: number): string {
  if (kind === "card") return `Library after removal: ${bytesText(Math.max(0, plan.library_bytes - removeBytes))}`;
  return `Free after removal: ${bytesText(plan.free_bytes + removeBytes)}`;
}

/** Tracks that matched the rules but are held back by a protection setting. */
export function hiddenLine(h: SpaceHidden): string {
  const parts: string[] = [];
  if (h.favourites) parts.push(`${h.favourites} in Favourites`);
  if (h.high_rated) parts.push(`${h.high_rated} rated highly`);
  if (h.new_tracks) parts.push(`${h.new_tracks} added recently`);
  if (parts.length === 0) return "";
  const n = h.favourites + h.high_rated + h.new_tracks;
  return `${plural(n, "track")} matched your rules but ${n === 1 ? "is" : "are"} protected: ${parts.join(", ")}.`;
}

export type DupConflict = { keeper: string; copies: string[] };

/** Ticked copies whose kept track is ticked as well: no copy of that audio would be left. */
export function duplicateConflicts(items: readonly SpaceItem[], ticked: ReadonlySet<number>): DupConflict[] {
  const byId = new Map(items.map((i) => [i.track_id, i]));
  const groups = new Map<number, DupConflict>();
  for (const i of items) {
    if (!ticked.has(i.track_id) || i.duplicate_of === null || !ticked.has(i.duplicate_of)) continue;
    const g = groups.get(i.duplicate_of) ?? { keeper: byId.get(i.duplicate_of)?.label ?? i.duplicate_of_label ?? "its original", copies: [] };
    g.copies.push(i.label);
    groups.set(i.duplicate_of, g);
  }
  return [...groups.values()];
}

export type SortKey = "label" | "bytes" | "last_played" | "rating" | "reasons";

export function sortItems(items: readonly SpaceItem[], key: SortKey | null, asc: boolean): SpaceItem[] {
  if (!key) return [...items];
  const dir = asc ? 1 : -1;
  const val = (i: SpaceItem): string | number => {
    switch (key) {
      case "label":
        return i.label.toLowerCase();
      case "bytes":
        return i.bytes;
      // never played sorts as the oldest
      case "last_played":
        return i.last_played_ms ?? -1;
      case "rating":
        return i.rating;
      case "reasons":
        return i.reasons.length;
    }
  };
  return [...items].sort((a, b) => {
    const x = val(a);
    const y = val(b);
    // ties keep the planner's id order in both directions
    return x < y ? -dir : x > y ? dir : a.track_id - b.track_id;
  });
}

/** Text filter over the track name, its reasons, its cases and its format. */
export function filterItems(items: readonly SpaceItem[], text: string): SpaceItem[] {
  const q = text.trim().toLowerCase();
  if (!q) return [...items];
  return items.filter((i) => `${i.label} ${i.reasons.map((r) => r.text).join(" ")} ${i.in_cases.join(" ")} ${i.format}`.toLowerCase().includes(q));
}

export type RunMode = "archive" | "remove" | "both";

export const runFlags = (mode: RunMode): { archive: boolean; remove: boolean } => ({ archive: mode !== "remove", remove: mode !== "archive" });

const counts = (plan: SpaceRunPlan) => ({
  archived: plan.items.filter((i) => i.archive_name).length,
  removed: plan.items.filter((i) => i.remove).length,
});

/** The confirmation's heading, from the same preview the run repeats. */
export function runTitle(plan: SpaceRunPlan, mode: RunMode): string {
  const { archived, removed } = counts(plan);
  if (mode === "archive") return `Archive ${plural(archived, "track")}?`;
  if (mode === "remove") return `Remove ${plural(removed, "track")} from the card?`;
  return `Archive ${plural(archived, "track")} and remove ${removed} from the card?`;
}

/** The confirm button's label. */
export function runButton(plan: SpaceRunPlan, mode: RunMode): string {
  const { archived, removed } = counts(plan);
  if (mode === "archive") return `Archive ${plural(archived, "track")} (${bytesText(plan.archive_bytes)})`;
  if (mode === "remove") return `Remove ${plural(removed, "track")} (${bytesText(plan.remove_bytes)})`;
  return `Archive ${archived} and remove ${removed}`;
}

/** What one line of the confirmation says will happen to the track. */
export function runLine(i: SpaceRunItem): string {
  const parts: string[] = [];
  if (i.archive_name) parts.push(`archived as ${i.archive_name}`);
  if (i.remove) parts.push("removed from the card");
  else if (i.note) parts.push(i.note);
  return parts.join("; ") || "nothing";
}

/** A removal with no archive copy is the one thing that cannot be taken back: it asks for a tick. */
export const needsAcknowledge = (plan: SpaceRunPlan, mode: RunMode): boolean => mode === "remove" && plan.items.some((i) => i.remove);

/** Turns a 0-means-off number setting into a checkbox state and back. */
export const isOn = (n: number) => n > 0;
export const toggled = (current: number, on: boolean, fallback: number) => (on ? (current > 0 ? current : fallback) : 0);

/** A number typed in a settings field, kept in range; junk keeps the old value. */
export function clampInt(text: string | number, lo: number, hi: number, old: number): number {
  const n = typeof text === "number" ? text : Number.parseInt(text, 10);
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, Math.trunc(n))) : old;
}

/** Like clampInt for a size in GB with one decimal. */
export function clampGb(text: string | number, lo: number, hi: number, old: number): number {
  const n = typeof text === "number" ? text : Number.parseFloat(text);
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, Math.round(n * 10) / 10)) : old;
}

export type MeterPart = { key: "used" | "freed" | "free" | "over"; bytes: number };

/** The segments of the target meter: what stays used, what the ticks free, what is free (or, for
 *  a card target, what is still over the card's limit). The widths are shares of the sum. */
export function meterParts(plan: SpacePlan, kind: "free" | "card", removeBytes: number): { parts: MeterPart[]; total: number; label: string } {
  let parts: MeterPart[];
  let label: string;
  if (kind === "card") {
    const lib = plan.library_bytes;
    const usable = plan.usable_bytes ?? 0;
    const removed = Math.min(removeBytes, lib);
    const kept = lib - removed;
    const fits = Math.min(kept, usable);
    parts = [
      { key: "used", bytes: fits },
      { key: "over", bytes: kept - fits },
      { key: "freed", bytes: removed },
      { key: "free", bytes: Math.max(0, usable - kept) },
    ];
    label = `Library: ${bytesText(fits)} fits, ${bytesText(kept - fits)} is over the card's limit, ${bytesText(removed)} ticked for removal, ${bytesText(Math.max(0, usable - kept))} room left`;
  } else {
    if (plan.total_bytes <= 0) return { parts: [], total: 0, label: "Free space unknown" };
    const used = Math.max(0, plan.total_bytes - plan.free_bytes);
    const removed = Math.min(removeBytes, used);
    parts = [
      { key: "used", bytes: used - removed },
      { key: "freed", bytes: removed },
      { key: "free", bytes: plan.free_bytes },
    ];
    label = `${bytesText(used - removed)} used, ${bytesText(removed)} ticked for removal, ${bytesText(plan.free_bytes)} free`;
  }
  parts = parts.filter((p) => p.bytes > 0);
  return { parts, total: parts.reduce((n, p) => n + p.bytes, 0), label };
}
