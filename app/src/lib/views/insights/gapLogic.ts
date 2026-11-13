// Pure logic of the Library gap map: bins the library into BPM-band x key, BPM-band x genre and
// key x energy grids, finds the thin areas, and turns a cell back into the Tracks view's own
// filter. No store, component or runtime imports (types only) so it runs under plain node:
//   node --experimental-strip-types src/lib/views/insights/gapLogic.test.mjs
// The key parser is passed in (`KeyCode`) for the same reason; the view hands it
// `(k) => parseKey(k)?.code ?? null` from the Tracks area.
import type { Track } from "$lib/api/types";
import type { TrackFilter } from "../tracks/trackLogic";

/** A track as the gap map sees it. `energy` is the optional rating another analysis chain adds
 *  to the wire type later; until then it is simply absent. */
/** What the matrices read of a track. Only these fields are copied per track (a spread of the
 *  whole row for every track of a big library cost over a second on each recompute). */
export type GapTrack = Pick<Track, "track_id" | "bpm" | "key" | "genre"> & { energy?: number | null };

/** Camelot code ("8A") of a stored key text, or null when empty or not recognised. */
export type KeyCode = (key: string) => string | null;

// ---- settings -----------------------------------------------------------------------------

export type GapSettings = {
  /** Width of a BPM band, in BPM. */
  bandWidth: number;
  /** Tracks below / at or above these are left out of the BPM grids (counted in the summary). */
  bpmMin: number;
  bpmMax: number;
  /** Genre rows shown; the rest are folded into one "Other genres" row. */
  maxGenres: number;
  /** A cell with fewer tracks than this is thin. */
  thinBelow: number;
  /** A row is only checked for thin cells once it holds at least this many tracks. */
  minRowTracks: number;
};

export const BAND_WIDTHS = [2, 5, 10, 20] as const;

export const DEFAULT_SETTINGS: GapSettings = {
  bandWidth: 10,
  bpmMin: 60,
  bpmMax: 200,
  maxGenres: 12,
  thinBelow: 2,
  minRowTracks: 10,
};

const int = (v: unknown, lo: number, hi: number, fallback: number): number =>
  typeof v === "number" && Number.isFinite(v) ? Math.max(lo, Math.min(hi, Math.round(v))) : fallback;

/** Allowed range of each numeric setting, as mergeSettings clamps it (and the form shows it). */
export const SETTING_LIMITS = {
  bpmMin: [20, 300],
  bpmMax: [30, 400],
  maxGenres: [3, 40],
  thinBelow: [1, 20],
  minRowTracks: [1, 500],
} as const;

/** Settings from anything (saved JSON, a half-typed form): unknown or out-of-range values fall
 *  back to the defaults, and the BPM range is trimmed so it holds a whole number of bands.
 *  A range narrower than one band keeps the bound the user did not just change: `edited` names
 *  the one that was (`bpmMin` is pulled down below the max, otherwise the max is pushed up above
 *  the min, which is also what happens for a saved range or a changed band width). */
export function mergeSettings(raw: unknown, edited?: keyof GapSettings): GapSettings {
  const s = raw && typeof raw === "object" ? (raw as Partial<GapSettings>) : {};
  const d = DEFAULT_SETTINGS;
  const L = SETTING_LIMITS;
  const bandWidth = (BAND_WIDTHS as readonly number[]).includes(s.bandWidth as number) ? (s.bandWidth as number) : d.bandWidth;
  let bpmMin = int(s.bpmMin, L.bpmMin[0], L.bpmMin[1], d.bpmMin);
  let bpmMax = int(s.bpmMax, L.bpmMax[0], L.bpmMax[1], d.bpmMax);
  if (bpmMax - bpmMin < bandWidth) {
    if (edited === "bpmMin") {
      bpmMin = Math.max(L.bpmMin[0], bpmMax - bandWidth);
      bpmMax = Math.max(bpmMax, bpmMin + bandWidth);
    } else {
      bpmMax = bpmMin + bandWidth;
    }
  }
  bpmMax = bpmMin + Math.floor((bpmMax - bpmMin) / bandWidth) * bandWidth;
  return {
    bandWidth,
    bpmMin,
    bpmMax,
    maxGenres: int(s.maxGenres, L.maxGenres[0], L.maxGenres[1], d.maxGenres),
    thinBelow: int(s.thinBelow, L.thinBelow[0], L.thinBelow[1], d.thinBelow),
    minRowTracks: int(s.minRowTracks, L.minRowTracks[0], L.minRowTracks[1], d.minRowTracks),
  };
}

const SETTING_LABEL: Record<keyof GapSettings, string> = {
  bandWidth: "BPM band",
  bpmMin: "BPM range from",
  bpmMax: "BPM range up to",
  maxGenres: "Genres shown",
  thinBelow: "Thin limit",
  minRowTracks: "Judge rows from",
};

/** Words for a setting change that was not used exactly as typed ("" when it was): a value
 *  outside its allowed range, or a BPM range that had to move to stay at least one band wide and
 *  a whole number of bands. `requested` is what was typed, `before` the settings it replaced. */
export function settingNotice(edited: keyof GapSettings, requested: number, next: GapSettings, before: GapSettings): string {
  if (edited !== "bandWidth") {
    const [lo, hi] = SETTING_LIMITS[edited];
    if (!Number.isFinite(requested) || Math.round(requested) < lo || Math.round(requested) > hi) {
      return `${SETTING_LABEL[edited]} must be ${lo} to ${hi}; ${next[edited]} is used.`;
    }
  }
  const wantMin = edited === "bpmMin" ? Math.round(requested) : before.bpmMin;
  const wantMax = edited === "bpmMax" ? Math.round(requested) : before.bpmMax;
  const touchesRange = edited === "bpmMin" || edited === "bpmMax" || edited === "bandWidth";
  if (touchesRange && (next.bpmMin !== wantMin || next.bpmMax !== wantMax)) {
    return `The BPM range is now ${next.bpmMin} to ${next.bpmMax}: it holds whole ${next.bandWidth} BPM bands, and at least one.`;
  }
  return "";
}

// ---- bins and matrices --------------------------------------------------------------------

export type BinValue =
  | { kind: "bpm"; lo: number; hi: number }
  | { kind: "key"; code: string }
  | { kind: "genre"; name: string }
  | { kind: "otherGenres"; count: number }
  | { kind: "energy"; level: number };

export type Bin = { id: string; label: string; value: BinValue };

export type MatrixId = "bpm-key" | "bpm-genre" | "key-energy";

export const MATRIX_IDS: MatrixId[] = ["bpm-key", "bpm-genre", "key-energy"];

export type Skipped = { reason: string; count: number };

export type Matrix = {
  id: MatrixId;
  rows: Bin[];
  cols: Bin[];
  /** Track ids per cell: cells[row][col]. */
  cells: number[][][];
  rowTotals: number[];
  colTotals: number[];
  /** Tracks placed in the grid. */
  total: number;
  /** Largest cell count, for the colour scale. */
  max: number;
  /** Whether neighbouring rows / columns are neighbours in value (BPM, energy) rather than a list. */
  rowsOrdered: boolean;
  colsOrdered: boolean;
  /** Tracks left out of this grid, by the first reason that applies. */
  skipped: Skipped[];
  /** Tracks considered (the whole library). */
  considered: number;
};

/** 1A, 2A ... 12A, then 1B ... 12B: neighbours on the wheel sit next to each other in a letter. */
export const KEY_ORDER: string[] = [
  ...Array.from({ length: 12 }, (_, i) => `${i + 1}A`),
  ...Array.from({ length: 12 }, (_, i) => `${i + 1}B`),
];

/** A track has a usable BPM when the Tracks table would show one (positive, and not flagged as
 *  needing analysis, whose stored BPM is not trusted). Same rule as the Tracks filter. */
export const hasBpm = (t: Pick<Track, "bpm" | "track_id">, needs: ReadonlyMap<number, string>): boolean =>
  t.bpm > 0 && Number.isFinite(t.bpm) && !needs.has(t.track_id);

/** Energy level of a track, or null when it has none. Rounded to a whole level, 1 or more. */
export function energyOf(t: GapTrack): number | null {
  const e = t.energy;
  if (typeof e !== "number" || !Number.isFinite(e)) return null;
  const r = Math.round(e);
  return r >= 1 ? Math.min(r, 10) : null;
}

export const anyEnergy = (tracks: readonly GapTrack[]): boolean => tracks.some((t) => energyOf(t) !== null);

/** Index of the BPM band that holds `bpm`: band i covers [min + i*width, min + (i+1)*width). */
export function bandIndex(bpm: number, min: number, width: number): number {
  let i = Math.floor((bpm - min) / width);
  // guard the division against rounding right at a band edge
  if (bpm < min + i * width) i--;
  else if (bpm >= min + (i + 1) * width) i++;
  return i;
}

const bandBin = (min: number, width: number, i: number): Bin => {
  const lo = min + i * width;
  const hi = lo + width;
  return { id: `bpm:${lo}`, label: `${lo}–${hi}`, value: { kind: "bpm", lo, hi } };
};
const keyBin = (code: string): Bin => ({ id: `key:${code}`, label: code, value: { kind: "key", code } });

type Placed = { id: number; row: string; col: string };

function assemble(
  id: MatrixId,
  rows: Bin[],
  cols: Bin[],
  placed: Placed[],
  opts: { rowsOrdered: boolean; colsOrdered: boolean; skipped: Skipped[]; considered: number },
): Matrix {
  const rowAt = new Map(rows.map((b, i) => [b.id, i]));
  const colAt = new Map(cols.map((b, i) => [b.id, i]));
  const cells: number[][][] = rows.map(() => cols.map(() => []));
  for (const p of placed) cells[rowAt.get(p.row)!][colAt.get(p.col)!].push(p.id);
  const rowTotals = cells.map((r) => r.reduce((n, c) => n + c.length, 0));
  const colTotals = cols.map((_, c) => cells.reduce((n, r) => n + r[c].length, 0));
  const max = cells.reduce((m, r) => r.reduce((mm, c) => Math.max(mm, c.length), m), 0);
  return {
    id,
    rows,
    cols,
    cells,
    rowTotals,
    colTotals,
    total: placed.length,
    max,
    rowsOrdered: opts.rowsOrdered,
    colsOrdered: opts.colsOrdered,
    skipped: opts.skipped.filter((s) => s.count > 0),
    considered: opts.considered,
  };
}

/** Bands from the lowest to the highest occupied one, empty bands in between included (those
 *  are the gaps). */
function bandsFor(indices: number[], s: GapSettings): Bin[] {
  if (indices.length === 0) return [];
  const lo = Math.min(...indices);
  const hi = Math.max(...indices);
  return Array.from({ length: hi - lo + 1 }, (_, k) => bandBin(s.bpmMin, s.bandWidth, lo + k));
}

/** Splits the library into those with a BPM inside the shown range (with their band) and the
 *  two groups left out. */
function bpmSplit(tracks: readonly GapTrack[], needs: ReadonlyMap<number, string>, s: GapSettings) {
  const inRange: { t: GapTrack; band: number }[] = [];
  let noBpm = 0;
  let outside = 0;
  for (const t of tracks) {
    if (!hasBpm(t, needs)) noBpm++;
    else if (t.bpm < s.bpmMin || t.bpm >= s.bpmMax) outside++;
    else inRange.push({ t, band: bandIndex(t.bpm, s.bpmMin, s.bandWidth) });
  }
  return { inRange, noBpm, outside };
}

export function buildBpmKey(tracks: readonly GapTrack[], needs: ReadonlyMap<number, string>, s: GapSettings, keyCode: KeyCode): Matrix {
  const { inRange, noBpm, outside } = bpmSplit(tracks, needs, s);
  const placed: { id: number; band: number; key: string }[] = [];
  let noKey = 0;
  for (const { t, band } of inRange) {
    const code = keyCode(t.key);
    if (code) placed.push({ id: t.track_id, band, key: code });
    else noKey++;
  }
  const rows = bandsFor(placed.map((p) => p.band), s);
  const rowId = (band: number) => bandBin(s.bpmMin, s.bandWidth, band).id;
  return assemble(
    "bpm-key",
    rows,
    KEY_ORDER.map(keyBin),
    placed.map((p) => ({ id: p.id, row: rowId(p.band), col: `key:${p.key}` })),
    {
      rowsOrdered: true,
      colsOrdered: false,
      considered: tracks.length,
      skipped: [
        { reason: "no BPM (not analysed yet)", count: noBpm },
        { reason: `BPM outside ${s.bpmMin}–${s.bpmMax}`, count: outside },
        { reason: "no key", count: noKey },
      ],
    },
  );
}

export function buildBpmGenre(tracks: readonly GapTrack[], needs: ReadonlyMap<number, string>, s: GapSettings): Matrix {
  const { inRange, noBpm, outside } = bpmSplit(tracks, needs, s);
  const withGenre = inRange.filter(({ t }) => t.genre !== "");
  const noGenre = inRange.length - withGenre.length;

  // the biggest genres get a row each (exact text, the way the Tracks filter compares it)
  const perGenre = new Map<string, number>();
  for (const { t } of withGenre) perGenre.set(t.genre, (perGenre.get(t.genre) ?? 0) + 1);
  const ranked = [...perGenre.entries()].sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  const named = ranked.slice(0, s.maxGenres).map(([g]) => g);
  const nameSet = new Set(named);
  const folded = ranked.length - named.length;
  const rows: Bin[] = named.map((g) => ({ id: `genre:${g}`, label: g, value: { kind: "genre", name: g } }));
  const OTHER = "genre-other";
  if (folded > 0) rows.push({ id: OTHER, label: "Other genres", value: { kind: "otherGenres", count: folded } });

  const cols = bandsFor(withGenre.map((p) => p.band), s);
  return assemble(
    "bpm-genre",
    rows,
    cols,
    withGenre.map(({ t, band }) => ({
      id: t.track_id,
      row: nameSet.has(t.genre) ? `genre:${t.genre}` : OTHER,
      col: bandBin(s.bpmMin, s.bandWidth, band).id,
    })),
    {
      rowsOrdered: false,
      colsOrdered: true,
      considered: tracks.length,
      skipped: [
        { reason: "no BPM (not analysed yet)", count: noBpm },
        { reason: `BPM outside ${s.bpmMin}–${s.bpmMax}`, count: outside },
        { reason: "no genre", count: noGenre },
      ],
    },
  );
}

export function buildKeyEnergy(tracks: readonly GapTrack[], keyCode: KeyCode): Matrix {
  const placed: { id: number; key: string; level: number }[] = [];
  let noKey = 0;
  let noEnergy = 0;
  for (const t of tracks) {
    const level = energyOf(t);
    const code = keyCode(t.key);
    if (level === null) noEnergy++;
    else if (!code) noKey++;
    else placed.push({ id: t.track_id, key: code, level });
  }
  // ratings of 5 or less read as a 1-5 scale, anything above as 1-10
  const top = placed.every((p) => p.level <= 5) ? 5 : 10;
  const cols: Bin[] = Array.from({ length: top }, (_, i) => ({ id: `energy:${i + 1}`, label: String(i + 1), value: { kind: "energy", level: i + 1 } }));
  return assemble(
    "key-energy",
    KEY_ORDER.map(keyBin),
    cols,
    placed.map((p) => ({ id: p.id, row: `key:${p.key}`, col: `energy:${p.level}` })),
    {
      rowsOrdered: false,
      colsOrdered: true,
      considered: tracks.length,
      skipped: [
        { reason: "no energy", count: noEnergy },
        { reason: "no key", count: noKey },
      ],
    },
  );
}

export function buildMatrix(
  id: MatrixId,
  tracks: readonly GapTrack[],
  needs: ReadonlyMap<number, string>,
  s: GapSettings,
  keyCode: KeyCode,
): Matrix {
  if (id === "bpm-genre") return buildBpmGenre(tracks, needs, s);
  if (id === "key-energy") return buildKeyEnergy(tracks, keyCode);
  return buildBpmKey(tracks, needs, s, keyCode);
}

// ---- thin areas ---------------------------------------------------------------------------

export type ThinCell = { col: number; count: number };
export type ThinRow = {
  row: number;
  /** Tracks in the whole row. */
  total: number;
  /** Cells with no tracks, and cells with some but fewer than the limit. */
  empty: number[];
  few: ThinCell[];
};

export type Summary = {
  /** Rows that hold enough tracks to judge and have thin cells, biggest first. */
  thinRows: ThinRow[];
  /** Rows / columns with no track at all in the middle of an ordered axis. */
  emptyRows: number[];
  emptyCols: number[];
  /** Columns of an unordered axis (keys) with fewer tracks than the limit in the whole grid. */
  thinCols: { col: number; total: number }[];
  busiest: { row: number; col: number; count: number } | null;
};

/** The first and last index with a non-zero total, or null when all are zero. */
const span = (totals: number[]): [number, number] | null => {
  const first = totals.findIndex((n) => n > 0);
  if (first < 0) return null;
  return [first, totals.length - 1 - [...totals].reverse().findIndex((n) => n > 0)];
};

/** Where the library is thin. A row is judged once it holds `minRowTracks`. In it, a cell below
 *  `thinBelow` counts as thin; on an ordered column axis (BPM, energy) only cells between the
 *  row's own lowest and highest occupied column are looked at, because a genre that never goes
 *  below 120 BPM has no gap at 80. */
export function summarise(m: Matrix, s: Pick<GapSettings, "thinBelow" | "minRowTracks">): Summary {
  const gaps = (totals: number[]): number[] => {
    const range = span(totals);
    if (!range) return [];
    const out: number[] = [];
    for (let i = range[0]; i <= range[1]; i++) if (totals[i] === 0) out.push(i);
    return out;
  };
  const emptyRows = m.rowsOrdered ? gaps(m.rowTotals) : [];
  const emptyCols = m.colsOrdered ? gaps(m.colTotals) : [];
  // a band that is empty for every row is reported once, not under each row
  const everyRowEmpty = new Set(emptyCols);

  const thinRows: ThinRow[] = [];
  m.rows.forEach((_, r) => {
    const total = m.rowTotals[r];
    if (total < s.minRowTracks) return;
    const counts = m.cells[r].map((c) => c.length);
    const range = m.colsOrdered ? span(counts) : ([0, counts.length - 1] as [number, number]);
    if (!range) return;
    const empty: number[] = [];
    const few: ThinCell[] = [];
    for (let c = range[0]; c <= range[1]; c++) {
      if (counts[c] === 0) {
        if (!everyRowEmpty.has(c)) empty.push(c);
      } else if (counts[c] < s.thinBelow) few.push({ col: c, count: counts[c] });
    }
    if (empty.length + few.length > 0) thinRows.push({ row: r, total, empty, few });
  });
  thinRows.sort((a, b) => b.total - a.total || a.row - b.row);

  const thinCols = m.colsOrdered
    ? []
    : m.colTotals.flatMap((total, col) => (total < s.thinBelow && m.total > 0 ? [{ col, total }] : []));

  let busiest: Summary["busiest"] = null;
  m.cells.forEach((row, r) =>
    row.forEach((cell, c) => {
      if (cell.length > 0 && (!busiest || cell.length > busiest.count)) busiest = { row: r, col: c, count: cell.length };
    }),
  );

  return { thinRows, emptyRows, emptyCols, thinCols, busiest };
}

/** Cells to mark as thin on the grid: the empty and few cells of the rows `summarise` reports. */
export function thinCellSet(sum: Summary): Set<string> {
  const out = new Set<string>();
  for (const t of sum.thinRows) {
    for (const c of t.empty) out.add(`${t.row}:${c}`);
    for (const c of t.few) out.add(`${t.row}:${c.col}`);
  }
  return out;
}

// ---- cell -> Tracks view ------------------------------------------------------------------

/** The Tracks view filter that shows exactly the tracks of one cell, or null when that view
 *  cannot express it (an energy axis, the folded "Other genres" row, an empty cell). The BPM
 *  upper bound is the highest BPM actually inside the band, so the inclusive max of the Tracks
 *  filter agrees with the band's open upper edge. */
export function cellFilter(
  m: Matrix,
  row: number,
  col: number,
  tracks: readonly GapTrack[],
  needs: ReadonlyMap<number, string>,
): TrackFilter | null {
  const cell = m.cells[row]?.[col];
  if (!cell || cell.length === 0) return null;
  const f: TrackFilter = { genre: "", bpmMin: null, bpmMax: null, key: "", onlyAnalysed: false };
  for (const bin of [m.rows[row], m.cols[col]]) {
    const v = bin.value;
    if (v.kind === "energy" || v.kind === "otherGenres") return null;
    if (v.kind === "genre") f.genre = v.name;
    else if (v.kind === "key") f.key = `is:${v.code}`;
    else {
      let top = -Infinity;
      for (const t of tracks) if (hasBpm(t, needs) && t.bpm >= v.lo && t.bpm < v.hi && t.bpm > top) top = t.bpm;
      if (!Number.isFinite(top)) return null;
      f.bpmMin = v.lo;
      f.bpmMax = top;
    }
  }
  return f;
}
