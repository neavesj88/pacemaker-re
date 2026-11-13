// Wording for the gap map: names of bins, axes and tabs. Kept apart from gapLogic so that file
// stays free of runtime imports; this one may use the Tracks area's key names.
import { keyName, parseKey } from "../tracks/trackLogic";
import type { Bin, Matrix, MatrixId } from "./gapLogic";

export const TAB_LABEL: Record<MatrixId, string> = {
  "bpm-key": "BPM × Key",
  "bpm-genre": "BPM × Genre",
  "key-energy": "Key × Energy",
};

/** What the rows and columns of each grid are. */
export const AXES: Record<MatrixId, { row: string; col: string }> = {
  "bpm-key": { row: "BPM band", col: "Camelot key" },
  "bpm-genre": { row: "Genre", col: "BPM band" },
  "key-energy": { row: "Camelot key", col: "Energy" },
};

/** "120–130 BPM", "8A (A minor)", "House", "Energy 7": a bin as one would say it. */
export function binName(b: Bin): string {
  const v = b.value;
  switch (v.kind) {
    case "bpm":
      return `${v.lo}–${v.hi} BPM`;
    case "key": {
      const k = parseKey(v.code);
      return k ? `${v.code} (${keyName(k)})` : v.code;
    }
    case "genre":
      return v.name;
    case "otherGenres":
      return `Other genres (${v.count})`;
    case "energy":
      return `Energy ${v.level}`;
  }
}

/** Tooltip for a header: more than the label, e.g. the exact edges of a BPM band. */
export function binHint(b: Bin): string {
  const v = b.value;
  if (v.kind === "bpm") return `From ${v.lo} BPM up to, not including, ${v.hi} BPM`;
  if (v.kind === "otherGenres") return `${v.count} smaller genres folded into one row; raise "Genres shown" to list them`;
  return binName(b);
}

export const cellName = (m: Matrix, row: number, col: number): string => `${binName(m.rows[row])} × ${binName(m.cols[col])}`;

export const plural = (n: number, one: string, many = `${one}s`): string => `${n} ${n === 1 ? one : many}`;
