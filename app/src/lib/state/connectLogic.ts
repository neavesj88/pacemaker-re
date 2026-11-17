// Pure rules of the "Pacemaker connected" prompt: which of the drives a check finds are new, and
// which ones the user already said no to. Plain TypeScript so plain node runs it
// (connectLogic.test.mjs); connect.svelte.ts owns the timer and the storage. Nothing here (or in
// the store) opens or reads a Pacemaker: a check is only the list of volumes.
import { rootKey } from "./place.ts";

/** Browser-storage keys (per computer, like the interface size). */
export const CONNECT_ASK_KEY = "pacemaker-revival.connectAsk";
export const CONNECT_SECS_KEY = "pacemaker-revival.connectSecs";

export const CONNECT_ASK_DEFAULT = true;
export const CONNECT_SECS_DEFAULT = 3;
export const CONNECT_SECS_MAX = 600;

/** Saved "ask when connected" (also the `normalise` for loadPref): only an explicit false turns it off. */
export const normaliseAsk = (raw: unknown): boolean => (typeof raw === "boolean" ? raw : CONNECT_ASK_DEFAULT);

/** Saved seconds between checks: whole seconds, 0 = never check, nothing usable gives the default. */
export function normaliseSecs(raw: unknown): number {
  if (typeof raw !== "number" || !Number.isFinite(raw)) return CONNECT_SECS_DEFAULT;
  return Math.min(CONNECT_SECS_MAX, Math.max(0, Math.round(raw)));
}

export type ConnectState = {
  /** The drives the previous check found; null before the first check (which only takes a baseline). */
  known: string[] | null;
  /** Drives the user said "Not now" to: not asked again until the drive has been unplugged. */
  dismissed: string[];
  /** Drives waiting for an answer, oldest first; the prompt shows the first. */
  pending: string[];
};

export const EMPTY_STATE: ConnectState = { known: null, dismissed: [], pending: [] };

const has = (list: readonly string[], v: string) => list.some((x) => rootKey(x) === rootKey(v));

/** The state after a check that found `now`. `openRoot` is the library that is open (never asked
 *  about again), `ask` the setting. The first check only remembers what is plugged in: a drive
 *  that was there before the app (or before checking was switched on) is not "connected". With
 *  `ask` off nothing is queued and an old question goes away. A drive that has gone leaves the
 *  queue and the dismissals, so plugging it in again asks again. */
export function step(s: ConnectState, now: readonly string[], openRoot: string | null, ask: boolean): ConnectState {
  const dismissed = s.dismissed.filter((d) => has(now, d));
  const pending = ask ? s.pending.filter((p) => has(now, p) && !has(dismissed, p) && !(openRoot && rootKey(p) === rootKey(openRoot))) : [];
  if (ask && s.known) {
    for (const v of now) {
      const seen = has(s.known, v) || has(pending, v) || has(dismissed, v);
      if (!seen && !(openRoot && rootKey(v) === rootKey(openRoot))) pending.push(v);
    }
  }
  return { known: [...now], dismissed, pending };
}

/** "Not now" for `v`: out of the queue, and not asked again until the drive has been unplugged. */
export function dismiss(s: ConnectState, v: string): ConnectState {
  return { ...s, pending: s.pending.filter((p) => rootKey(p) !== rootKey(v)), dismissed: has(s.dismissed, v) ? s.dismissed : [...s.dismissed, v] };
}

/** `v` was acted on (opened, or the open failed): out of the queue. It stays in `known`, so it is
 *  not asked about again until it has been unplugged and plugged in. */
export function answered(s: ConnectState, v: string): ConnectState {
  return { ...s, pending: s.pending.filter((p) => rootKey(p) !== rootKey(v)) };
}

/** The drive as the prompt shows it: "E:\" for a drive letter, else the path as it is. */
export function driveText(v: string): string {
  const m = /^([A-Za-z]:)[\\/]*$/.exec(v.trim());
  return m ? `${m[1]}\\` : v.trim();
}
