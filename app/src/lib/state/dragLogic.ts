// Pure arithmetic behind the pointer drags (trackDrag.svelte.ts, pointerReorder.svelte.ts), kept
// free of the DOM so `npm test` can run it.

export type RowBox = { top: number; height: number };

/** Where a row drag would drop, given the rows' boxes (in document order, client coordinates) and
 * the pointer's y. `over` is the row index the pointer is on and `after` whether it is in that
 * row's lower half (the insert line goes under it). Above the first row it is "before row 0",
 * below the last it is "after the last row", so the ends can always be reached. */
export function rowDropAt(rows: RowBox[], y: number): { over: number; after: boolean } | null {
  if (rows.length === 0) return null;
  if (y <= rows[0].top) return { over: 0, after: false };
  for (let i = 0; i < rows.length; i++) {
    const r = rows[i];
    // a pointer in the gap between two rows (a transition strip) belongs to the row above
    const bottom = i + 1 < rows.length ? rows[i + 1].top : r.top + r.height;
    if (y < bottom) return { over: i, after: y > r.top + r.height / 2 };
  }
  return { over: rows.length - 1, after: true };
}

/** The insert index (0..rows) a drop position means. */
export const insertIndex = (p: { over: number; after: boolean }) => p.over + (p.after ? 1 : 0);

/** Pixels from the left edge of a lane for a pointer at `clientX`; never negative. */
export const laneOffset = (clientX: number, laneLeft: number) => Math.max(0, clientX - laneLeft);

/** How many pixels to scroll a container per frame when the pointer is `y` and the container's
 * visible box is [top, bottom): zero in the middle, up to `max` at (or past) an edge within `zone`. */
export function edgeScroll(y: number, top: number, bottom: number, zone = 36, max = 18): number {
  if (bottom - top < zone * 2) return 0;
  if (y < top + zone) return -Math.ceil((max * Math.min(1, (top + zone - y) / zone)));
  if (y > bottom - zone) return Math.ceil(max * Math.min(1, (y - (bottom - zone)) / zone));
  return 0;
}

export type DragScope = "cases" | "mix" | "lib";

/** Whether a target declared with scope `target` (default cases) takes a drag of scope `drag`. */
export const scopeMatches = (target: DragScope | undefined, drag: DragScope | undefined): boolean =>
  (target ?? "cases") === (drag ?? "cases");

/** The scope a track drag named `from` has: the mix picker's drags ("mix:...") only reach the
 * timeline's lanes, the Libraries view's drags ("lib:<side>:<tab>") only reach its panes, every
 * other drag only reaches case targets. */
export const scopeOf = (from: string): DragScope => (from.startsWith("mix:") ? "mix" : from.startsWith("lib:") ? "lib" : "cases");
