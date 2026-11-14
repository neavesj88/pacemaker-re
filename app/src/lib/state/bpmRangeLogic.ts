// The "BPM range" analysis setting, as plain logic: the windows on offer and which tracks of a
// library were analysed outside the window in force. Mirrors `BpmRange` in crates/pm-analysis
// (`PRESETS`); the backend refuses any other value.

export type BpmRangeOption = {
  /** Low end; the window is [lo, 2 * lo). */
  lo: number;
  label: string;
  hint: string;
};

export const DEFAULT_RANGE_LO = 88;

/** The windows on offer, widest first. */
export const BPM_RANGES: readonly BpmRangeOption[] = [
  { lo: 98, label: "98-196", hint: "Fast drum & bass and hardcore at full time (180 reads as 180). 90 BPM hip-hop reads as 180." },
  { lo: 88, label: "88-176", hint: "Drum & bass at full time (about 174). House, techno and everything from 88 to 175 keeps its own number. Drum & bass at 176 or faster reads at half time (180 reads as 90): use 98-196 for those." },
  { lo: 78, label: "78-156", hint: "Same as the original Editor: drum & bass reads at half time (about 87), 158 reads as 79." },
  { lo: 68, label: "68-136", hint: "Slow music at its own tempo; anything from 136 up reads at half time (140 reads as 70)." },
];

export const rangeOf = (lo: number): BpmRangeOption => BPM_RANGES.find((r) => r.lo === lo) ?? BPM_RANGES.find((r) => r.lo === DEFAULT_RANGE_LO)!;

export const hiOf = (lo: number) => 2 * lo;

export const rangeLabel = (lo: number) => `${lo}-${hiOf(lo)}`;

/** A saved or typed low end if it is one on offer, else the default. */
export function normaliseLo(v: unknown): number {
  const n = typeof v === "number" ? v : Number.NaN;
  return BPM_RANGES.some((r) => r.lo === n) ? n : DEFAULT_RANGE_LO;
}

type Row = { track_id: number; title: string; artist: string; location: string; bpm: number; analysed: boolean };

/** The tracks whose stored BPM lies outside the window `lo`: they were analysed under another range
 *  (or by the original Editor, which reads drum & bass at ~87), or the BPM was typed in by hand.
 *  Tracks with no BPM, never analysed, or in `unreadable` (no readable grid) are left out: they
 *  have nothing to compare. Nothing is stored per track; the window itself is the test, so
 *  tracks the Editor analysed are found too, and a track re-analysed under the current window
 *  is never found again. */
export function outsideRange<T extends Row>(tracks: readonly T[], lo: number, unreadable?: ReadonlySet<number> | Map<number, unknown>): T[] {
  const hi = hiOf(lo);
  return tracks.filter((t) => t.analysed && t.bpm > 0 && !unreadable?.has(t.track_id) && (t.bpm < lo - 0.005 || t.bpm >= hi - 0.005));
}

export const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** One line per track for the offer: "Artist - Title  (87.00 BPM)". */
export function offerLine(t: Row): string {
  const name = t.artist && t.title ? `${t.artist} - ${t.title}` : t.title || t.location.split("/").pop() || String(t.track_id);
  return `${name}  (${t.bpm.toFixed(2)} BPM)`;
}

export function offerTitle(n: number, lo: number): string {
  return `${plural(n, "track")} read outside ${rangeLabel(lo)}`;
}

export function offerText(lo: number): string {
  return (
    `These tracks have a BPM outside the current range of ${rangeLabel(lo)}: they were analysed with another range, ` +
    `by the original Editor, or their BPM was typed in. Re-analysing replaces only the beat grid and BPM of the tracks you tick, ` +
    `from the audio already in the library. A beat grid or BPM you set by hand for them is replaced too. ` +
    `Music that really is slow or fast moves into the window: a true 80 BPM track would be read as 160, so untick those. "Leave these alone" stops listing them until the library is opened again.`
  );
}
