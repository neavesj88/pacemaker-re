// Which rows of a long, fixed-height list need to exist while it scrolls. Pure arithmetic so
// `npm test` can run it (views/cases/CaseDetail.svelte is the user; the Tracks table keeps its own
// copy of the same idea).

/** The rows `[start, end)` to render: those in view plus `overscan` either side. The first row
 *  starts `headH` px below the top of the scrolled content (a sticky header sits there). */
export function rowWindow(scrollTop: number, viewH: number, rowH: number, headH: number, total: number, overscan: number): { start: number; end: number } {
  if (total <= 0 || rowH <= 0) return { start: 0, end: 0 };
  // a scroll position left over from a longer list still shows the end of this one
  const first = Math.min(total - 1, Math.floor(Math.max(0, scrollTop - headH) / rowH));
  const last = Math.ceil(Math.max(0, scrollTop + viewH - headH) / rowH);
  const start = Math.min(total, Math.max(0, first - overscan));
  const end = Math.min(total, Math.max(start, last + overscan));
  return { start, end };
}

/** The scrollTop that brings row `i` fully into view with the least movement: unchanged when it
 *  already is, never below 0. The sticky header covers the top `headH` px of the view. */
export function revealTop(i: number, scrollTop: number, viewH: number, rowH: number, headH: number): number {
  const top = headH + i * rowH; // where the row starts in the scrolled content
  const bottom = top + rowH;
  if (top < scrollTop + headH) return Math.max(0, top - headH);
  if (bottom > scrollTop + viewH) return Math.max(0, bottom - viewH);
  return scrollTop;
}
