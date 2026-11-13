// Reordering the rows of a table with the pointer (set builder, an open case).
//
// Same reason as trackDrag.svelte.ts: HTML5 drag and drop is not delivered inside the page on
// Windows while Tauri's own drag-drop handler is on, so rows are moved with pointer events. The
// caller keeps its own `drag` state (which rows move, where the insert line is) and its own
// `.before` / `.after` / `.moving` classes; this file only runs the gesture:
//
//   onpointerdown={(e) => armReorder(e, { rows, scroller, begin, over, drop, end })}
//
// A press that never moves DRAG_THRESHOLD pixels stays a click. Once it does, `begin(i)` says which
// rows move (or null to refuse), `over` follows the pointer, release calls `drop` with the insert
// position, Escape, a cancelled pointer, the window losing focus or a move with no button down
// calls only `end`. Mouse only for rows without `touch-action: none`; the case grip has it. The scroller scrolls near its edges.

import { DRAG_THRESHOLD } from "./trackDrag.svelte";
import { edgeScroll, rowDropAt } from "./dragLogic";

export type ReorderOptions = {
  /** The rows in order, read on every move (rows come and go). */
  rows: () => HTMLElement[];
  /** The scrolling container, for auto-scroll near its top and bottom edge. */
  scroller?: () => HTMLElement | null | undefined;
  /** The drag starts: return false to refuse it. Make the selection the drag carries here. */
  begin: () => boolean | void;
  /** The insert position under the pointer changed. */
  over: (pos: { over: number; after: boolean }) => void;
  /** Released over a position. */
  drop: (pos: { over: number; after: boolean }) => void;
  /** The gesture is over, dropped or not. Clear the caller's drag state. */
  end: () => void;
};

let run: {
  startX: number;
  startY: number;
  pointerId: number;
  opts: ReorderOptions;
  started: boolean;
  y: number;
  raf: number;
  last: { over: number; after: boolean } | null;
} | null = null;

function locate() {
  const s = run;
  if (!s) return;
  const boxes = s.opts.rows().map((r) => r.getBoundingClientRect());
  const pos = rowDropAt(boxes, s.y);
  if (!pos) return;
  if (!s.last || s.last.over !== pos.over || s.last.after !== pos.after) {
    s.last = pos;
    s.opts.over(pos);
  }
}

function tick() {
  const s = run;
  if (!s || !s.started) return;
  const sc = s.opts.scroller?.();
  if (sc) {
    const b = sc.getBoundingClientRect();
    const d = edgeScroll(s.y, b.top, b.bottom);
    if (d !== 0) {
      sc.scrollTop += d;
      locate();
    }
  }
  s.raf = requestAnimationFrame(tick);
}

function onMove(e: PointerEvent) {
  const s = run;
  if (!s || e.pointerId !== s.pointerId) return;
  // the release was lost (focus left the window, a system dialog): no button is down any more
  if (e.buttons === 0) return finish(false);
  s.y = e.clientY;
  if (!s.started) {
    if (Math.hypot(e.clientX - s.startX, e.clientY - s.startY) < DRAG_THRESHOLD) return;
    if (s.opts.begin() === false) return finish(false);
    s.started = true;
    document.documentElement.style.userSelect = "none";
    document.documentElement.style.cursor = "grabbing";
    s.raf = requestAnimationFrame(tick);
  }
  locate();
}

const onUp = (e: PointerEvent) => run && e.pointerId === run.pointerId && finish(true);
const onCancel = (e: PointerEvent) => run && e.pointerId === run.pointerId && finish(false);
const onKey = (e: KeyboardEvent) => e.key === "Escape" && run && finish(false);
const onBlur = () => finish(false);
const onHidden = () => document.hidden && finish(false);

function swallowClick(e: Event) {
  e.stopPropagation();
  e.preventDefault();
}

function finish(drop: boolean) {
  const s = run;
  if (!s) return;
  run = null;
  cancelAnimationFrame(s.raf);
  window.removeEventListener("pointermove", onMove, true);
  window.removeEventListener("pointerup", onUp, true);
  window.removeEventListener("pointercancel", onCancel, true);
  window.removeEventListener("keydown", onKey, true);
  window.removeEventListener("blur", onBlur);
  document.removeEventListener("visibilitychange", onHidden);
  document.documentElement.style.userSelect = "";
  document.documentElement.style.cursor = "";
  if (s.started) {
    // the release would otherwise also click the row it started on
    window.addEventListener("click", swallowClick, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallowClick, true), 0);
    if (drop && s.last) s.opts.drop(s.last);
    s.opts.end();
  }
}

/** Call from a row's (or its grip's) `pointerdown`. Ignores presses on controls. */
export function armReorder(e: PointerEvent, opts: ReorderOptions) {
  if (e.button !== 0 || !e.isPrimary || run) return;
  if ((e.target as Element).closest("button, input, select, textarea, a")) return;
  run = { startX: e.clientX, startY: e.clientY, pointerId: e.pointerId, opts, started: false, y: e.clientY, raf: 0, last: null };
  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("pointerup", onUp, true);
  window.addEventListener("pointercancel", onCancel, true);
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("blur", onBlur);
  document.addEventListener("visibilitychange", onHidden);
}

/** Ends a reorder in progress without dropping (for a view that goes away mid-drag). */
export function endReorder() {
  finish(false);
}
