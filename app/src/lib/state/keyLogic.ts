// Pure logic of the key tools: the 24 keys of the picker, the stored spelling of a key, the
// detection settings and which tracks a "detect keys" run touches. No store or component imports
// (types only) so it runs under plain node: see keyLogic.test.mjs.
import type { Track } from "../api/types";

/** The part of a track the key tools read. */
export type KeyTrack = Pick<Track, "track_id" | "title" | "artist" | "key" | "location">;

// ---- the keys -------------------------------------------------------------------------------

const SHARP = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLAT = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

export type PickerKey = {
  /** What is stored in `tracks.key`: the detector's spelling, sharps only ("Am", "A#m", "C"). */
  name: string;
  /** The flat spelling when it differs ("Bbm"), else null. */
  flat: string | null;
  /** Camelot wheel code: "8A" for Am, "8B" for C. */
  camelot: string;
  camelotNumber: number;
  minor: boolean;
  /** Pitch class of the tonic, 0 = C. */
  tonic: number;
};

/** Camelot number of a major key by pitch class (C = 8, G = 9, ..., F = 7). */
const majorNumber = (pc: number) => (((pc * 7) % 12) + 7) % 12 + 1;

function makeKey(tonic: number, minor: boolean): PickerKey {
  const number = majorNumber(minor ? (tonic + 3) % 12 : tonic);
  const suffix = minor ? "m" : "";
  const name = SHARP[tonic] + suffix;
  const flat = FLAT[tonic] + suffix;
  return { name, flat: flat === name ? null : flat, camelot: `${number}${minor ? "A" : "B"}`, camelotNumber: number, minor, tonic };
}

/** All 24 keys, in Camelot wheel order: 1A, 1B, 2A, 2B ... 12B. */
export const PICKER_KEYS: PickerKey[] = Array.from({ length: 12 }, (_, i) => i + 1).flatMap((n) =>
  [true, false].map((minor) => {
    const tonic = [...Array(12).keys()].find((pc) => majorNumber(minor ? (pc + 3) % 12 : pc) === n)!;
    return makeKey(tonic, minor);
  }),
);

/** "A#m / Bbm" for a key that has two spellings, else just the name. */
export const spellings = (k: PickerKey) => (k.flat ? `${k.name} / ${k.flat}` : k.name);

const BASE: Record<string, number> = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };

/** The text stored for a typed or picked key, or null when the text is not a musical key.
 *  Same rule as `canonical_key` in commands/analysis.rs: "Bb", "bb minor" and "A#m" all give
 *  "A#m"; a Camelot code ("8A") is not a musical key and gives null. */
export function canonicalKeyName(text: string): string | null {
  // a bare capital M ("CM") is major in some books and minor in others: refuse it, as the backend does
  if (text.trim().endsWith("M")) return null;
  const m = /^([A-Ga-g])\s*([#b♯♭]?)\s*([A-Za-z]*)$/.exec(text.trim());
  if (!m) return null;
  const rest = m[3].toLowerCase();
  if (!["", "maj", "major", "m", "min", "minor"].includes(rest)) return null;
  const minor = rest === "m" || rest === "min" || rest === "minor";
  const shift = m[2] === "#" || m[2] === "♯" ? 1 : m[2] === "b" || m[2] === "♭" ? 11 : 0;
  return makeKey((BASE[m[1].toLowerCase()] + shift) % 12, minor).name;
}

/** The picker entry for a stored key, or null for an empty or unrecognised one. */
export function pickerKeyOf(stored: string): PickerKey | null {
  const name = canonicalKeyName(stored);
  return name ? (PICKER_KEYS.find((k) => k.name === name) ?? null) : null;
}

// ---- detection settings ----------------------------------------------------------------------

export type KeySettings = {
  /** Replace keys that are already set (including ones typed by hand). */
  overwrite: boolean;
  /** Results below this confidence (0 to 100 per cent) are skipped, not written. */
  minConfidencePct: number;
};

/** Safe by default: a key someone set is never replaced, and every result is written (0 per cent
 *  is how the backend behaved before the floor was a setting). */
export const DEFAULT_KEY_SETTINGS: KeySettings = { overwrite: false, minConfidencePct: 0 };

export const KEY_SETTINGS_STORAGE = "pacemaker.keyDetect";

export const clampPct = (n: number) => Math.min(100, Math.max(0, Math.round(n)));

/** Settings from whatever was stored (any JSON, or null): wrong types and out-of-range numbers
 *  fall back to the defaults, so a corrupt entry can never break the dialog. */
export function normaliseKeySettings(raw: unknown): KeySettings {
  const o = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  const pct =
    typeof o.minConfidencePct === "number" && Number.isFinite(o.minConfidencePct)
      ? o.minConfidencePct
      : DEFAULT_KEY_SETTINGS.minConfidencePct;
  return {
    overwrite: typeof o.overwrite === "boolean" ? o.overwrite : DEFAULT_KEY_SETTINGS.overwrite,
    minConfidencePct: clampPct(pct),
  };
}

/** The 0..1 floor the backend takes. */
export const confidenceFloor = (s: KeySettings) => clampPct(s.minConfidencePct) / 100;

/** Splits the asked-for tracks into those the run analyses and those it leaves alone. */
export function detectionPlan<T extends KeyTrack>(tracks: T[], s: KeySettings): { targets: T[]; kept: T[] } {
  const targets: T[] = [];
  const kept: T[] = [];
  for (const t of tracks) (s.overwrite || !t.key.trim() ? targets : kept).push(t);
  return { targets, kept };
}

export const trackLabel = (t: Pick<Track, "title" | "location">) => t.title || t.location.split("/").pop() || t.location;

/** One line of the confirmation for a track the run analyses, saying in words what happens. */
export function detectLine(t: KeyTrack, s: KeySettings): string {
  const name = trackLabel(t);
  const floor = clampPct(s.minConfidencePct);
  const only = floor > 0 ? ` if the result reaches ${floor}% confidence` : "";
  return t.key.trim()
    ? `${name}: key ${t.key.trim()} is replaced by the detected key${only}`
    : `${name}: no key yet, the detected key is written${only}`;
}
