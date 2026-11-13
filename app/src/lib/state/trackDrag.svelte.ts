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
// drag starts (a small pill follows the cursor and the sidebar strip lists the cases). A target
// marks an element with the `trackDropTarget` action; on release over one that accepts, its
// `ondrop` gets the ids. Every drag action also has a click route (Add to case… in the Tracks
// context menu and inspector, Add tracks… / Copy to… / Move to… in an open case).
//
// The mix editor's picker still uses its own HTML5 type (`application/x-pm-tracks`) for the mix
// timeline; nothing here touches it.

/** Pixels the pointer must travel before a press becomes a drag (a plain click stays a click). */
export const DRAG_THRESHOLD = 6;

class TrackDrag {
  /** A drag carrying tracks is in progress anywhere in the window. */
  active = $state(false);
  /** How many tracks it carries. */
  count = $state(0);
  /** Where it started: "tracks" (the Tracks table) or "case:<id>" (an open case). */
  from = $state("");
}

export const trackDrag = new TrackDrag();

export type DropOptions = {
  /** Whether a drop at `hit` (the element under the pointer) would be taken. */
  accept: (hit: Element) => boolean;
  /** Called on release over an accepting target. */
  ondrop: (ids: number[], hit: Element) => void;
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
  ids: number[];
  ghost: HTMLElement | null;
  hover: HTMLElement | null;
  hit: Element | null;
} | null = null;

function targetAt(x: number, y: number): { node: HTMLElement; hit: Element; opts: DropOptions } | null {
  const hit = document.elementFromPoint(x, y);
  const node = hit?.closest(`[${ATTR}]`) as HTMLElement | null | undefined;
  const opts = node ? targets.get(node) : undefined;
  return node && opts && hit ? { node, hit, opts } : null;
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
  if (!s.ghost) {
    if (Math.hypot(e.clientX - s.startX, e.clientY - s.startY) < DRAG_THRESHOLD) return;
    const { ids } = s.source();
    if (ids.length === 0) return finish(false, e);
    s.ids = ids;
    s.ghost = makeGhost(ids.length);
    trackDrag.from = s.from;
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

function swallowClick(e: Event) {
  e.stopPropagation();
  e.preventDefault();
}

function finish(drop: boolean, e?: PointerEvent) {
  const s = session;
  if (!s) return;
  const dragged = !!s.ghost;
  let t: ReturnType<typeof targetAt> = null;
  if (drop && dragged && e) {
    t = targetAt(e.clientX, e.clientY);
    if (t && !t.opts.accept(t.hit)) t = null;
  }
  setHover(null, null);
  session = null;
  s.ghost?.remove();
  window.removeEventListener("pointermove", onMove, true);
  window.removeEventListener("pointerup", onUp, true);
  window.removeEventListener("pointercancel", onCancel, true);
  window.removeEventListener("keydown", onKey, true);
  document.documentElement.style.userSelect = "";
  document.documentElement.style.cursor = "";
  if (dragged) {
    // the release would otherwise also click the row it started on
    window.addEventListener("click", swallowClick, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallowClick, true), 0);
    trackDrag.active = false;
    trackDrag.count = 0;
    trackDrag.from = "";
    if (t) t.opts.ondrop(s.ids, t.hit);
  }
}

/** Call from a row's `pointerdown`. `source` is read when the drag actually starts, so the
 * selection a plain click just made is what gets dragged. `from` names the table (see
 * `trackDrag.from`) so it can refuse its own drops. */
export function armTrackDrag(e: PointerEvent, from: string, source: Source) {
  if (e.button !== 0 || !e.isPrimary || session) return;
  if ((e.target as Element).closest("button, input, select, textarea, a, [data-no-drag]")) return;
  session = { startX: e.clientX, startY: e.clientY, pointerId: e.pointerId, source, from, ids: [], ghost: null, hover: null, hit: null };
  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("pointerup", onUp, true);
  window.addEventListener("pointercancel", onCancel, true);
  window.addEventListener("keydown", onKey, true);
}

/** Ends any drag in progress without dropping (for a view that goes away mid-drag). */
export function endTrackDrag() {
  finish(false);
}
