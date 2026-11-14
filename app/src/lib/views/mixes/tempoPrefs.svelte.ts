// Settings of the mix tab that are policy choices rather than document content: how tempo
// matching treats a big stretch, whether new clips are matched at all, and the width of the track
// table beside the timeline. Kept in the browser storage like the other mix settings
// (try/catch-guarded: everything works without storage).
import { DEFAULT_STRETCH_WARN_PCT } from "$lib/state/tempoMatch";
import { loadPref, savePref } from "$lib/state/prefStore";

export type OnAddOverStretch = "keep" | "match";

export type TempoPrefs = {
  /** Stretch (per cent, either way) above which a tempo match is not applied silently. */
  warnPct: number;
  /** A clip added to a mix with a tempo is matched to it (half / double time included). */
  matchOnAdd: boolean;
  /** What happens to a newly added clip that would need more than `warnPct`: keep its own tempo, or match anyway. */
  overOnAdd: OnAddOverStretch;
  /** Memory (MB) the live player may use for decoded audio; a mix that needs more reads the nearest tracks first. */
  decodeMb: number;
};

export const DEFAULT_TEMPO_PREFS: TempoPrefs = { warnPct: DEFAULT_STRETCH_WARN_PCT, matchOnAdd: true, overOnAdd: "keep", decodeMb: 1024 };

export function mergeTempoPrefs(raw: unknown): TempoPrefs {
  const d = { ...DEFAULT_TEMPO_PREFS };
  if (!raw || typeof raw !== "object") return d;
  const r = raw as Partial<TempoPrefs>;
  if (typeof r.warnPct === "number" && Number.isFinite(r.warnPct)) d.warnPct = Math.min(100, Math.max(0, r.warnPct));
  if (typeof r.matchOnAdd === "boolean") d.matchOnAdd = r.matchOnAdd;
  if (r.overOnAdd === "keep" || r.overOnAdd === "match") d.overOnAdd = r.overOnAdd;
  if (typeof r.decodeMb === "number" && Number.isFinite(r.decodeMb)) d.decodeMb = Math.min(65536, Math.max(256, Math.round(r.decodeMb)));
  return d;
}

const STORAGE_KEY = "pm.mixes.tempo.v1";

class TempoSettings {
  value = $state<TempoPrefs>({ ...DEFAULT_TEMPO_PREFS });

  constructor() {
    this.value = loadPref(STORAGE_KEY, mergeTempoPrefs, () => ({ ...DEFAULT_TEMPO_PREFS }));
  }

  set(patch: Partial<TempoPrefs>) {
    this.value = mergeTempoPrefs({ ...this.value, ...patch });
    savePref(STORAGE_KEY, this.value);
  }
}

export const tempoPrefs = new TempoSettings();
