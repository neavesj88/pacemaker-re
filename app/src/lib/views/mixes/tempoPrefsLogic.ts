// The settings of the mix tab that are policy choices (see tempoPrefs.svelte.ts), apart from the
// reactive store so the rules for reading saved settings can be tested without Svelte.
import { DEFAULT_STRETCH_WARN_PCT } from "../../state/tempoMatch.ts";

export type OnAddOverStretch = "keep" | "match";
/** What an added track is matched to: the track it follows on the timeline, or the mix's reference tempo. */
export type MatchTo = "previous" | "mix";

export type TempoPrefs = {
  /** Stretch (per cent, either way) above which a tempo match is not applied silently. */
  warnPct: number;
  /** A clip added to a mix with a tempo is matched to it (half / double time included). */
  matchOnAdd: boolean;
  /** What an added track is matched to. */
  matchTo: MatchTo;
  /** What happens to a newly added clip that would need more than `warnPct`: keep its own tempo, or match anyway. */
  overOnAdd: OnAddOverStretch;
  /** Memory (MB) the live player may use for decoded audio; a mix that needs more reads the nearest tracks first. */
  decodeMb: number;
  /** Let the computer decide the memory (an eighth of its memory, 256 to 1024 MB) instead of `decodeMb`. */
  decodeAuto: boolean;
};

export const DEFAULT_TEMPO_PREFS: TempoPrefs = { warnPct: DEFAULT_STRETCH_WARN_PCT, matchOnAdd: true, matchTo: "previous", overOnAdd: "keep", decodeMb: 1024, decodeAuto: true };

export function mergeTempoPrefs(raw: unknown): TempoPrefs {
  const d = { ...DEFAULT_TEMPO_PREFS };
  if (!raw || typeof raw !== "object") return d;
  const r = raw as Partial<TempoPrefs>;
  if (typeof r.warnPct === "number" && Number.isFinite(r.warnPct)) d.warnPct = Math.min(100, Math.max(0, r.warnPct));
  if (typeof r.matchOnAdd === "boolean") d.matchOnAdd = r.matchOnAdd;
  if (r.matchTo === "previous" || r.matchTo === "mix") d.matchTo = r.matchTo;
  if (r.overOnAdd === "keep" || r.overOnAdd === "match") d.overOnAdd = r.overOnAdd;
  if (typeof r.decodeMb === "number" && Number.isFinite(r.decodeMb)) d.decodeMb = Math.min(65536, Math.max(256, Math.round(r.decodeMb)));
  if (typeof r.decodeAuto === "boolean") d.decodeAuto = r.decodeAuto;
  // settings saved before the choice existed: 1024 was the fixed default (and is what a computer with
  // 8 GB or more gets automatically), any other number was typed by the user and stays theirs
  else if (typeof r.decodeMb === "number" && r.decodeMb !== 1024) d.decodeAuto = false;
  return d;
}
