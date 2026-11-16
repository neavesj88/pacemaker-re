// The maths of the whole-mix strip above the timeline: where the box for the part of the mix
// shown below sits, and where a click or a drag in the strip scrolls the view to. Pure (no Svelte,
// no DOM), so it runs under plain node (overview.test.mjs). Pixels of the strip are counted from
// its left edge; the scroll position is the timeline's (pixels, as mv.scrollLeft).

/** The strip's height (px), ruler excluded: the timeline reserves it above its scrolling lanes. */
export const OVERVIEW_H = 44;

/** The strip's box is never narrower than this (px), so it can be grabbed when the whole mix is on screen below. */
export const BOX_MIN_W = 6;

/** The time the strip spans (seconds): the whole mix, at least 10 s so an empty or tiny mix still draws. */
export const overviewSpan = (mixSecs: number): number => Math.max(10, mixSecs);

export type OverviewBox = { x0: number; x1: number };

/** The box over the strip for the part of the timeline below it: from the left edge of the lanes
 * (`scroll` px) to their right edge (`laneW` px later), at `pps` pixels per second, on a strip
 * `stripW` wide that spans `span` seconds. It stays inside the strip and keeps a minimum width. */
export function visibleBox(scroll: number, laneW: number, pps: number, span: number, stripW: number): OverviewBox {
  if (!(stripW > 0) || !(span > 0) || !(pps > 0)) return { x0: 0, x1: 0 };
  const k = stripW / span;
  const w = Math.min(stripW, Math.max(BOX_MIN_W, Math.max(0, laneW / pps) * k));
  const x0 = Math.min(stripW - w, Math.max(0, (scroll / pps) * k));
  return { x0, x1: x0 + w };
}

/** The strip pixel of a time (seconds), clamped to the strip. */
export const stripX = (secs: number, span: number, stripW: number): number => Math.min(stripW, Math.max(0, (secs / span) * stripW));

/** The scroll position that puts the time under strip pixel `x` in the middle of the lanes, within
 * what the lanes can scroll (`maxScroll`). */
export function centreScroll(x: number, stripW: number, span: number, pps: number, laneW: number, maxScroll: number): number {
  if (!(stripW > 0)) return 0;
  const secs = (Math.min(stripW, Math.max(0, x)) / stripW) * span;
  return Math.min(Math.max(0, maxScroll), Math.max(0, secs * pps - laneW / 2));
}

/** The scroll position after the box was dragged `dx` strip pixels from where the drag began
 * (`startScroll`), within what the lanes can scroll. */
export function dragScroll(startScroll: number, dx: number, stripW: number, span: number, pps: number, maxScroll: number): number {
  if (!(stripW > 0)) return startScroll;
  return Math.min(Math.max(0, maxScroll), Math.max(0, startScroll + (dx / stripW) * span * pps));
}

/** Whether strip pixel `x` is on the box (a press there drags it; elsewhere it centres the view first). */
export const onBox = (x: number, box: OverviewBox): boolean => x >= box.x0 && x <= box.x1;
