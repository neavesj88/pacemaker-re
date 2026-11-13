// Dragging tracks out of a table so a case can take them, with pointer events.
//
// Why not HTML5 drag and drop: tauri.conf.json leaves Tauri's own drag-drop handler on (the OS file
// import in api/import.ts needs it), and Tauri's own config docs (tauri-utils `drag_drop_enabled`)
// say HTML5 drag and drop on the frontend is unavailable on Windows while that handler is on,
// "since we replace the drag drop handler of WebView2". So in-page `dragover` / `drop` never fire
// in the Windows app. A pointer drag is plain DOM events and works the same everywhere, and it
// leaves the OS file drop untouched.
//
// A table row calls `armTrackDrag` from `pointerdown`; once the pointer has moved a few pixels the
// drag starts (a small pill follows the cursor and, for case drags, the sidebar strip lists the
// cases). A target marks an element with the `trackDropTarget` action; on release over one that
// accepts, its `ondrop` gets the ids and the pointer position. Every drag action also has a click
// route (Add to case… in the Tracks context menu and inspector, Add tracks… / Copy to… / Move to…
// in an open case; Add at playhead / Add to end in the mix editor's picker).
//
// A drag has a scope, taken from its `from` name: "mix:..." drags (the mix editor's picker) are
// only offered to targets declared with scope "mix" (the deck lanes of the timeline); every other
// drag has scope "cases" and only reaches case targets. Reordering rows inside one table (set
// builder, open case) is not a track drag at all: see pointerReorder.svelte.ts, which shares this
// file's DRAG_THRESHOLD and the same listener pattern.

import { scopeMatches, scopeOf, type DragScope } from "./dragLogic";

/** Pixels the pointer must travel before a press becomes a drag (a plain click stays a click). */
export const DRAG_THRESHOLD = 6;

export type { DragScope };

class TrackDrag {
  /** A drag carrying tracks is in progress anywhere in the window. */
  active = $state(false);
  /** How many tracks it carries. */
  count = $state(0);
  /** Where it started: "tracks" (the Tracks table), "case:<id>" (an open case) or "mix:picker". */
  from = $state("");
  /** Which targets it can reach (see the header). Derived from `from` when the drag starts. */
  scope = $state<DragScope>("cases");
}

export const trackDrag = new TrackDrag();

/** Where the pointer was on release / hover: client coordinates and the target element. */
export type DropPoint = { x: number; y: number; node: HTMLElement };

export type DropOptions = {
  /** Which drags this target takes. Default "cases". */
  scope?: DragScope;
  /** Whether a drop at `hit` (the element under the pointer) would be taken. */
  accept: (hit: Element) => boolean;
  /** Called on release over an accepting target. */
  ondrop: (ids: number[], hit: Element, at: DropPoint) => void;
  /** Called with the element under the pointer while an accepted drag is over the target, and
   * with null when it leaves. Without it the target gets a `data-drop-over` attribute instead. */
  onhover?: (hit: Element | null) => void;
};

const targets = new WeakMap<Element, DropOptions>();
const ATTR = "data-track-drop";

/** Svelte action: make an element a place tracks can be dropped. */
export function trackDropTarget(node: HTMLElement, options: DropOptions) {
  node.setAttribute(ATTR, "");
  targets.set(node, options);
  return {
    update(next: DropOptions) {
      targets.set(node, next);
    },
    destroy() {
      targets.delete(node);
      node.removeAttribute(ATTR);
    },
  };
}

type Source = () => { ids: number[] };

let session: {
  startX: number;
  startY: number;
  pointerId: number;
  source: Source;
  from: string;
  scope: DragScope;
  ids: number[];
  ghost: HTMLElement | null;
  hover: HTMLElement | null;
  hit: Element | null;
} | null = null;

function targetAt(x: number, y: number): { node: HTMLElement; hit: Element; opts: DropOptions } | null {
  const hit = document.elementFromPoint(x, y);
  const node = hit?.closest(`[${ATTR}]`) as HTMLElement | null | undefined;
  const opts = node ? targets.get(node) : undefined;
  if (!node || !opts || !hit || !scopeMatches(opts.scope, session?.scope)) return null;
  return { node, hit, opts };
}

function setHover(node: HTMLElement | null, hit: Element | null) {
  if (session?.hover && session.hover !== node) {
    const prev = targets.get(session.hover);
    if (prev?.onhover) prev.onhover(null);
    else session.hover.removeAttribute("data-drop-over");
  }
  if (node) {
    const o = targets.get(node);
    if (o?.onhover) o.onhover(hit);
    else node.setAttribute("data-drop-over", "");
  }
  if (session) {
    session.hover = node;
    session.hit = hit;
  }
}

function makeGhost(n: number) {
  const g = document.createElement("div");
  g.textContent = `${n} track${n === 1 ? "" : "s"}`;
  Object.assign(g.style, {
    position: "fixed",
    left: "0",
    top: "0",
    zIndex: "1000",
    pointerEvents: "none",
    padding: "4px 10px",
    borderRadius: "10px",
    background: "var(--accent)",
    color: "var(--bg)",
    font: "600 12px system-ui, sans-serif",
    whiteSpace: "nowrap",
  });
  document.body.appendChild(g);
  return g;
}

function onMove(e: PointerEvent) {
  const s = session;
  if (!s || e.pointerId !== s.pointerId) return;
  // the release was lost (focus left the window, a system dialog): no button is down any more
  if (e.buttons === 0) return finish(false);
  if (!s.ghost) {
    if (Math.hypot(e.clientX - s.startX, e.clientY - s.startY) < DRAG_THRESHOLD) return;
    const { ids } = s.source();
    if (ids.length === 0) return finish(false, e);
    s.ids = ids;
    s.ghost = makeGhost(ids.length);
    trackDrag.from = s.from;
    trackDrag.scope = s.scope;
    trackDrag.count = ids.length;
    trackDrag.active = true;
    document.documentElement.style.userSelect = "none";
    document.documentElement.style.cursor = "grabbing";
  }
  s.ghost.style.transform = `translate(${e.clientX + 12}px, ${e.clientY + 12}px)`;
  const t = targetAt(e.clientX, e.clientY);
  if (t && t.opts.accept(t.hit)) setHover(t.node, t.hit);
  else setHover(null, null);
}

function onUp(e: PointerEvent) {
  if (session && e.pointerId === session.pointerId) finish(true, e);
}

function onCancel(e: PointerEvent) {
  if (session && e.pointerId === session.pointerId) finish(false, e);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && session) finish(false);
}

const onBlur = () => finish(false);
const onHidden = () => document.hidden && finish(false);

function swallowClick(e: Event) {
  e.stopPropagation();
  e.preventDefault();
}

function finish(drop: boolean, e?: PointerEvent) {
  const s = session;
  if (!s) return;
  const dragged = !!s.ghost;
  let t: ReturnType<typeof targetAt> = null;
  let at: DropPoint | null = null;
  if (drop && dragged && e) {
    t = targetAt(e.clientX, e.clientY);
    if (t && !t.opts.accept(t.hit)) t = null;
    if (t) at = { x: e.clientX, y: e.clientY, node: t.node };
  }
  setHover(null, null);
  session = null;
  s.ghost?.remove();
  window.removeEventListener("pointermove", onMove, true);
  window.removeEventListener("pointerup", onUp, true);
  window.removeEventListener("pointercancel", onCancel, true);
  window.removeEventListener("keydown", onKey, true);
  window.removeEventListener("blur", onBlur);
  document.removeEventListener("visibilitychange", onHidden);
  document.documentElement.style.userSelect = "";
  document.documentElement.style.cursor = "";
  if (dragged) {
    // the release would otherwise also click the row it started on
    window.addEventListener("click", swallowClick, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallowClick, true), 0);
    trackDrag.active = false;
    trackDrag.count = 0;
    trackDrag.from = "";
    trackDrag.scope = "cases";
    if (t && at) t.opts.ondrop(s.ids, t.hit, at);
  }
}

/** Call from a row's `pointerdown`. `source` is read when the drag actually starts, so the
 * selection a plain click just made is what gets dragged. `from` names the table (see
 * `trackDrag.from`) so it can refuse its own drops. */
export function armTrackDrag(e: PointerEvent, from: string, source: Source) {
  if (e.button !== 0 || !e.isPrimary || session) return;
  if ((e.target as Element).closest("button, input, select, textarea, a, [data-no-drag]")) return;
  session = { startX: e.clientX, startY: e.clientY, pointerId: e.pointerId, source, from, scope: scopeOf(from), ids: [], ghost: null, hover: null, hit: null };
  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("pointerup", onUp, true);
  window.addEventListener("pointercancel", onCancel, true);
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("blur", onBlur);
  document.addEventListener("visibilitychange", onHidden);
}

/** Ends any drag in progress without dropping (for a view that goes away mid-drag). */
export function endTrackDrag() {
  finish(false);
}
