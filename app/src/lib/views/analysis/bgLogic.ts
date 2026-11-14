// Pure logic of the background analysis UI: what the status line says, when the strip shows,
// how finished keys reach the table, and how the "couldn't analyse" list reads. Run by plain
// node (`npm test`), so no Svelte and no Tauri in here.
import type { BgFailure, BgKind, BgStatus } from "$lib/api/bganalysis";
import { sameRoot } from "../../state/place.ts";

export const KIND_WORD: Record<BgKind, string> = { peaks: "waveforms", keys: "keys", energy: "energy", sections: "sections" };

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** States in which the strip shows: something is being made, or is waiting to continue, or went wrong. */
export function stripVisible(s: BgStatus): boolean {
  switch (s.state) {
    case "running":
    case "blocked":
    case "error":
      return true;
    case "paused":
    case "playing":
    case "waiting":
    case "held":
      return s.left > 0;
    default:
      return false;
  }
}

/** The one line of the strip, for example "Analysing in the background: 12 of 40 - waveforms". */
export function statusLine(s: BgStatus): string {
  const more = s.left > 0 ? ` (${s.left} left in all)` : "";
  switch (s.state) {
    case "running":
      return s.word && s.total > 0 ? `Analysing in the background: ${s.done} of ${s.total} - ${s.word}` : "Analysing in the background: checking what is missing";
    case "paused":
      return `Background analysis paused${more}`;
    case "playing":
      return `Background analysis waits while a preview plays${more}`;
    case "waiting":
      return `Background analysis waits for the job you started${more}`;
    case "held":
      return `Background analysis waits while a Sync plan is on screen${more}`;
    case "blocked":
      return `Background analysis is held up: ${s.detail || "a result could not be saved"}. It tries again shortly.`;
    case "error":
      return `Background analysis could not read the library: ${s.detail || "unknown reason"}. It tries again shortly.`;
    case "idle":
      return s.failures > 0
        ? `Background analysis is up to date, except ${plural(s.failures, "result", "results")} it could not make.`
        : "Background analysis is up to date: nothing is missing.";
    case "manual":
      return "Background analysis never starts on a Pacemaker by itself. Use Run now to analyse this one.";
    case "off":
      return "Background analysis is switched off.";
    default:
      return "Background analysis starts when a library is open.";
  }
}

/** The Pause / Resume button: only while there is something to pause or resume. */
export function pauseAction(s: BgStatus): "pause" | "resume" | null {
  if (s.state === "paused") return "resume";
  if (s.state === "running" || s.state === "playing" || s.state === "waiting") return "pause";
  return null;
}

/** Keys that finished, grouped by the key text so the table is patched once per distinct key. */
export function groupKeys(keys: { track_id: number; key: string }[]): Map<string, number[]> {
  const out = new Map<string, number[]>();
  for (const k of keys) {
    const ids = out.get(k.key);
    if (ids) ids.push(k.track_id);
    else out.set(k.key, [k.track_id]);
  }
  return out;
}

/** Something went wrong with this one, as against a file that was read fine and has no result to
 *  give (a drum-only track has no key): that is not a failure and has nothing to try again. */
export const isFailure = (f: BgFailure): boolean => !f.no_result && !f.held_back;

/** What was read fine but gave no result: one row per track, with what is missing (`keys`). */
export function noResultRows(list: BgFailure[]): FailureRow[] {
  return rowsOf(list.filter((f) => f.no_result));
}

/** Keys found but under the confidence floor: not failures, and asked again when the floor changes. */
export function heldBackRows(list: BgFailure[]): FailureRow[] {
  return rowsOf(list.filter((f) => f.held_back));
}

/** The "couldn't analyse" list: one row per track, the kinds that failed and the reasons. */
export type FailureRow = { track_id: number; label: string; kinds: string; reasons: string };

export function failureRows(list: BgFailure[]): FailureRow[] {
  return rowsOf(list.filter(isFailure));
}

function rowsOf(list: BgFailure[]): FailureRow[] {
  const by = new Map<number, BgFailure[]>();
  for (const f of list) by.set(f.track_id, [...(by.get(f.track_id) ?? []), f]);
  return [...by.values()]
    .map((fs) => ({
      track_id: fs[0].track_id,
      label: fs[0].label,
      kinds: [...new Set(fs.map((f) => KIND_WORD[f.kind]))].join(", "),
      reasons: [...new Set(fs.map((f) => f.reason))].join("; "),
    }))
    .sort((a, b) => a.label.localeCompare(b.label));
}

/** The note under the strip: how many tracks could not be analysed, hidden once the user
 *  dismissed it for this many results (it comes back if more fail). */
export function failureNote(failures: BgFailure[], dismissedAt: number): string | null {
  const real = failures.filter(isFailure);
  const tracks = new Set(real.map((f) => f.track_id)).size;
  if (tracks === 0 || real.length <= dismissedAt) return null;
  return `${plural(tracks, "track", "tracks")} couldn't be analysed.`;
}

/** The status now belongs to another library than the last one (or none): the failure list read
 *  for the old one, and a "Hide" pressed on it, are not about this one. The worker's list is
 *  global, so the count alone cannot tell two libraries with the same number of failures apart. */
export function libraryChanged(prevRoot: string | null | undefined, nextRoot: string | null | undefined): boolean {
  if (!prevRoot || !nextRoot) return !!prevRoot !== !!nextRoot;
  return !sameRoot(prevRoot, nextRoot);
}

/** How many results count towards the hidden note (no-result ones never do). */
export const failureCount = (failures: BgFailure[]): number => failures.filter(isFailure).length;

/** The remembered "note hidden" counts, by library root, from storage. Anything unusable is dropped. */
export function parseDismissed(raw: unknown): Record<string, number> {
  const out: Record<string, number> = {};
  if (raw && typeof raw === "object" && !Array.isArray(raw)) {
    for (const [k, v] of Object.entries(raw)) if (typeof v === "number" && Number.isInteger(v) && v > 0) out[k] = v;
  }
  return out;
}

/** Clamps the key confidence floor the way the backend validates it (whole per cent, 0 to 100). */
export function clampPct(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return Math.min(100, Math.max(0, Math.round(v)));
}

/**
 * True while the worker is actually writing keys and other results into the library. Paused,
 * playing, waiting, held, blocked and error all keep a non-zero `left` but write nothing, so
 * they must not hold back the smart-case counts.
 */
export function writingNow(s: Pick<BgStatus, "state" | "left">): boolean {
  return s.state === "running" && s.left > 0;
}

/**
 * Whether the failure list must be read again after a status tick. The status count leaves out
 * held-back and no-result records, so a change in them shows only as the worker changing state
 * (a run starts or ends after the confidence floor moves or Try again is pressed).
 */
export function failuresStale(last: { failures: number; state: string }, s: Pick<BgStatus, "failures" | "state">): boolean {
  return s.failures !== last.failures || s.state !== last.state;
}
