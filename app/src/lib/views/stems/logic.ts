// Pure helpers of the Stems view (no Svelte, no Tauri), tested by logic.test.mjs.
import type { Component, Plan, PlanAction, PlanItem, StemSettings, StemsProgress, StemsResult } from "$lib/api/stems";

/** "83 MB", "1.2 GB": decimal units, like the sizes in the download notices. */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "";
  if (n < 1_000) return `${Math.round(n)} B`;
  if (n < 1_000_000) return `${Math.round(n / 1_000)} kB`;
  if (n < 1_000_000_000) return `${n >= 100_000_000 ? Math.round(n / 1e6) : Math.round(n / 1e5) / 10} MB`;
  return `${Math.round(n / 1e8) / 10} GB`;
}

/** Time left, as "about 3 min" / "about 40 s" / "under 10 s"; "" when not known yet. */
export function formatEta(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs) || secs < 0) return "";
  if (secs < 10) return "under 10 s";
  if (secs < 90) return `about ${Math.round(secs / 5) * 5} s`;
  const m = Math.round(secs / 60);
  if (m < 90) return `about ${m} min`;
  const h = Math.floor(m / 60);
  return `about ${h} h ${m % 60} min`;
}

/** "3:05" from seconds. */
export function clock(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export const noun = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;

/** Progress of a run as 0..1 over the plan's items (a skipped item counts as done at once). */
export function runFraction(p: StemsProgress | null): number {
  if (!p || p.count <= 0) return 0;
  const inside = p.segments > 0 ? Math.min(1, p.segment / p.segments) : 0;
  return Math.max(0, Math.min(1, (p.index + inside) / p.count));
}

/** What the progress line says for the phase. */
export function phaseText(p: StemsProgress | null): string {
  if (!p) return "Starting…";
  if (p.phase === "checking-model") return "Checking the model file…";
  if (p.phase === "loading-model") return "Loading the model…";
  const where = p.count > 1 ? `track ${p.index + 1} of ${p.count}` : "";
  const seg = p.segments > 0 ? `part ${Math.min(p.segment + 1, p.segments)} of ${p.segments}` : "reading the file";
  return [where, seg].filter(Boolean).join(", ");
}

export type Step = "download" | "ready";

/** The one-line state of a downloadable part, for its row. */
export function componentState(c: Component): { text: string; tone: "ok" | "todo" | "na" } {
  if (!c.available_here) return { text: "No download for this system", tone: "na" };
  if (c.installed) return { text: `Installed (${formatBytes(c.installed_bytes)})`, tone: "ok" };
  return { text: `Not downloaded (${formatBytes(c.size_bytes)})`, tone: "todo" };
}

const VERB: Record<PlanAction, string> = { make: "Make", replace: "Replace", partial: "Make part", skip: "Keep", refuse: "Cannot" };
export const actionVerb = (a: PlanAction) => VERB[a];

/** Items that run the model. */
export const runnable = (plan: Plan): PlanItem[] => plan.items.filter((i) => i.action === "make" || i.action === "replace" || i.action === "partial");

/** Files this plan will write, in order (vocals first). */
export function filesToWrite(plan: Plan): string[] {
  return runnable(plan).flatMap((i) => [i.write_vocals ? i.vocals_path : null, i.write_instrumental ? i.instrumental_path : null].filter((p): p is string => !!p));
}

/** The headline of the plan: what happens, counted. */
export function planHeadline(plan: Plan): string {
  const parts: string[] = [];
  const n = plan.make + plan.replace + plan.partial;
  if (n > 0) parts.push(`${noun(n, "track")} to separate (${Math.round(plan.minutes)} min of audio)`);
  if (plan.replace > 0) parts.push(`${noun(plan.replaced_files.length, "existing file")} will be replaced`);
  if (plan.skip > 0) parts.push(`${noun(plan.skip, "track")} already done and kept`);
  if (plan.refuse > 0) parts.push(`${noun(plan.refuse, "track")} cannot be done`);
  return parts.join("; ") || "Nothing to do";
}

/** A run is allowed when something would run; replacing needs the explicit tick. */
export function canStart(plan: Plan | null, confirmedReplace: boolean): boolean {
  if (!plan || runnable(plan).length === 0) return false;
  return plan.replace === 0 || confirmedReplace;
}

/** Every file of a finished run that exists to be added to the card, vocals then instrumental per track. */
export function resultFiles(r: StemsResult | null): string[] {
  if (!r) return [];
  return r.written.flatMap((w) => [w.vocals, w.instrumental].filter((p): p is string => !!p));
}

const BAD = /[\\/:*?"<>|\u0000-\u001f]/;

/** The backend's checks for the two name suffixes, for feedback while typing (the backend decides). */
export function suffixProblem(label: string, v: string): string | null {
  const t = v.trim();
  if (t.length === 0 || [...t].length > 40) return `${label} must be 1 to 40 characters.`;
  if (BAD.test(t)) return `${label} cannot contain any of \\ / : * ? " < > | or control characters.`;
  return null;
}

/** Why these settings cannot be saved, or null. */
export function settingsProblem(s: StemSettings): string | null {
  return (
    suffixProblem("The vocals suffix", s.vocals_suffix) ??
    suffixProblem("The instrumental suffix", s.instrumental_suffix) ??
    (s.vocals_suffix.trim().toLowerCase() === s.instrumental_suffix.trim().toLowerCase() ? "The two suffixes must differ, or both results would get the same file name." : null) ??
    (s.overlap_percent < 10 || s.overlap_percent > 90 ? "Overlap must be between 10 and 90 percent." : null)
  );
}

export const sameSettings = (a: StemSettings, b: StemSettings) => JSON.stringify(strip(a)) === JSON.stringify(strip(b));
function strip(s: StemSettings): StemSettings {
  const { consents: _c, ...rest } = s as StemSettings & { consents?: unknown };
  return Object.fromEntries(Object.entries(rest).sort(([x], [y]) => x.localeCompare(y))) as StemSettings;
}

/** Names of the files for the add-to-card confirmation: the file name, not the folder. */
export const fileName = (p: string) => p.split(/[\\/]/).pop() || p;

/** Does the (display) path of the destination look like it is the device? The backend is the
 *  authority (it canonicalises); this only warns before a plan is asked for. */
export function looksLikeDevice(dest: string, root: string | undefined): boolean {
  if (!root) return false;
  const norm = (p: string) => p.replace(/[\\/]+$/, "").replace(/\\/g, "/").toLowerCase();
  const d = norm(dest);
  const r = norm(root);
  return d === r || d.startsWith(r + "/");
}
