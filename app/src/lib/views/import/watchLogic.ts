// Pure rules of the watch-folder import: what to do with the result of a scan, how a scan is
// worded, and the number fields of the settings. No Svelte and no Tauri here so node can test it
// (src/lib/views/import/watchLogic.test.mjs).
import type { WatchCandidate, WatchLogEntry, WatchScan, WatchSettings } from "$lib/api/import";

/** The settings before anything is saved; the same values as `Settings::default()` in watch.rs. */
export const DEFAULT_WATCH_SETTINGS: WatchSettings = {
  folders: [],
  mode: "ask",
  scan_on_open: true,
  interval_minutes: 0,
  subfolders: true,
  min_age_secs: 10,
  auto_max_files: 100,
  open_confirmation: false,
};

/** Why a scan ran: the library was opened, the timer fired, the user pressed Scan now, or the
 *  list is only being brought up to date (after an add, or a settings change). */
export type ScanReason = "open" | "timer" | "manual" | "refresh";

export type Action =
  | { kind: "none" }
  /** Tell the user; nothing is added. */
  | { kind: "toast"; text: string; tone: "info" | "ok" | "bad"; announce: true }
  /** Open the shared add confirmation for these files. */
  | { kind: "confirm"; paths: string[]; announce: true }
  /** Start the add straight away (still a job strip, a toast when it ends and a log entry). */
  | { kind: "auto"; paths: string[] };

export type DecideInput = {
  reason: ScanReason;
  settings: WatchSettings;
  scan: WatchScan;
  /** The add flow can start a write now (not busy, library writable). */
  canWrite: boolean;
  /** Paths an automatic add already tried this session: never retried automatically, so a file
   *  that fails to add cannot be retried on every scan. */
  tried: ReadonlySet<string>;
  /** The user was already told about exactly this set of files. */
  announced: boolean;
};

const NONE: Action = { kind: "none" };
const files = (n: number) => `${n} new audio file${n === 1 ? "" : "s"}`;

/** Identifies the set of files a scan offers, to tell the user about each set only once. */
export function scanSignature(scan: WatchScan): string {
  const c = scan.candidates;
  return c.length === 0 ? "" : `${scan.candidates_total}|${c[0].path}|${c[c.length - 1].path}`;
}

/** What to do with a finished scan. "ask" never adds anything by itself: it only tells the user
 *  or opens the confirmation. "auto" adds up to the user's limit and otherwise only tells. */
export function decide(i: DecideInput): Action {
  const { reason, settings: s, scan } = i;
  const found = scan.candidates;
  if (reason === "refresh" || found.length === 0) return NONE;
  const n = scan.candidates_total;
  // the same news twice is noise, unless the user asked for the scan
  const stale = i.announced && reason !== "manual";

  if (s.mode === "ask") {
    if (stale) return NONE;
    if (s.open_confirmation) return { kind: "confirm", paths: found.map((c) => c.path), announce: true };
    // a manual scan shows its result on screen already
    if (reason === "manual") return NONE;
    return { kind: "toast", tone: "info", announce: true, text: `${files(n)} found in the watched folders. Open Import to review ${n === 1 ? "it" : "them"}.` };
  }

  if (n > s.auto_max_files) {
    if (stale) return NONE;
    return {
      kind: "toast",
      tone: "bad",
      announce: true,
      text: `${files(n)} found in the watched folders, more than the ${s.auto_max_files} allowed to be added automatically, so nothing was added. Open Import to review them.`,
    };
  }
  const paths = found.filter((c) => !i.tried.has(c.path)).map((c) => c.path);
  if (paths.length === 0) return NONE;
  if (!i.canWrite) {
    if (stale) return NONE;
    return {
      kind: "toast",
      tone: "info",
      announce: true,
      text: `${files(n)} found in the watched folders, but nothing can be added right now (another job is running, or the library is read-only). Open Import to see them.`,
    };
  }
  return { kind: "auto", paths };
}

/** The finished-line toast of an automatic add. */
export function autoDoneText(summary: string, failed: number, error: string | null): string {
  if (error) return `Watch folder add failed: ${error}`;
  const tail = failed > 0 ? " Open Import to see what failed." : "";
  return `Watch folder: ${summary || "nothing to do"}.${tail}`;
}

/** The log entry of an automatic add, from the paths that were handed over and the job result. */
export function logEntryFor(
  mode: "auto" | "ask",
  names: string[],
  done: { ok: number; failed: number; summary: string; error: string | null },
): Omit<WatchLogEntry, "at"> {
  return {
    mode,
    requested: names.length,
    names,
    ok: done.ok,
    failed: done.failed,
    summary: done.error ? `failed: ${done.error}` : done.summary,
  };
}

/** The counters of a scan, in words, only those that are not zero (the order is the order the
 *  scan decided each file in). */
export function scanLines(scan: WatchScan): string[] {
  const n = (k: number, one: string, many: string) => `${k} ${k === 1 ? one : many}`;
  const out: string[] = [];
  if (scan.in_library) out.push(`${n(scan.in_library, "file is", "files are")} already in the library (same name and size)`);
  if (scan.matched_total) out.push(`${n(scan.matched_total, "file", "files")} left out because a library track matches by title (listed below)`);
  if (scan.ignored) out.push(`${n(scan.ignored, "file", "files")} dismissed by you`);
  if (scan.too_new) out.push(`${n(scan.too_new, "file", "files")} changed a moment ago, left for the next scan`);
  if (scan.duplicates) out.push(`${n(scan.duplicates, "extra copy", "extra copies")} with the same name and size`);
  if (scan.unusable_names) out.push(`${n(scan.unusable_names, "file", "files")} with a name that cannot be added`);
  if (scan.cut_short) out.push("the folders hold more than a scan can look at; the rest is not listed");
  return out;
}

export function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10240 ? 1 : 0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/** "just now", "5 minutes ago", "3 hours ago", "2 days ago". */
export function agoText(ms: number, now: number): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 45) return "just now";
  const unit = (v: number, name: string) => `${v} ${name}${v === 1 ? "" : "s"} ago`;
  if (s < 3600) return unit(Math.max(1, Math.round(s / 60)), "minute");
  if (s < 86_400) return unit(Math.round(s / 3600), "hour");
  return unit(Math.round(s / 86_400), "day");
}

/** "off", "every minute", "every 15 minutes", "every 2 hours", "every 90 minutes". */
export function intervalLabel(min: number): string {
  if (min <= 0) return "off";
  if (min === 1) return "every minute";
  if (min % 60 === 0) return min === 60 ? "every hour" : `every ${min / 60} hours`;
  return `every ${min} minutes`;
}

/** A whole number in [lo, hi] from a text field, or null. */
export function wholeIn(raw: string, lo: number, hi: number): number | null {
  const t = raw.trim();
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  return n >= lo && n <= hi ? n : null;
}

/** Is this path an absolute folder path on Windows or Unix? (The backend checks again.) */
export function looksAbsolute(p: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(p) || p.startsWith("\\\\") || p.startsWith("/");
}

/** The candidates whose paths are in `chosen`, in list order. */
export function chosenCandidates(all: WatchCandidate[], chosen: ReadonlySet<string>): WatchCandidate[] {
  return all.filter((c) => chosen.has(c.path));
}
