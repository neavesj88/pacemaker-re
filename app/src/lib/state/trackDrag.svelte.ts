// Dragging tracks out of a table so a case can take them. A table calls `startTrackDrag` from its
// row's `dragstart`; any case target (the sidebar strip, a case row, an open case) reads the ids
// with `readTrackDrag` on drop. Tracks and Cases are separate views, so the sidebar strip
// (views/cases/CaseDrops.svelte) is the target that is on screen whatever view the drag begins in;
// it shows itself while `trackDrag.active` is set.
//
// Payload: JSON number array under `TRACKS_MIME`. The mix editor's picker uses its own type
// (`application/x-pm-tracks`) for the mix timeline; the two do not overlap.
import { parseTrackIds } from "./casePlan";

export const TRACKS_MIME = "application/x-pacemaker-tracks";

class TrackDrag {
  /** A drag carrying tracks is in progress anywhere in the window. */
  active = $state(false);
  /** How many tracks it carries. */
  count = $state(0);
}

export const trackDrag = new TrackDrag();

/** Puts `ids` on the drag; `names` (title and artist, any number of them) is the plain-text
 * fallback so dropping on a text field still pastes something sensible. */
export function startTrackDrag(e: DragEvent, ids: number[], names: string[] = []) {
  const dt = e.dataTransfer;
  if (!dt || ids.length === 0) return;
  dt.setData(TRACKS_MIME, JSON.stringify(ids));
  dt.setData("text/plain", names.slice(0, 20).join("\n"));
  dt.effectAllowed = "copyMove";
  trackDrag.active = true;
  trackDrag.count = ids.length;
  // a small pill with the count instead of a screenshot of one row
  try {
    const pill = document.createElement("div");
    pill.textContent = `${ids.length} track${ids.length === 1 ? "" : "s"}`;
    Object.assign(pill.style, {
      position: "fixed",
      top: "-100px",
      left: "-100px",
      padding: "4px 10px",
      borderRadius: "10px",
      background: "var(--accent)",
      color: "var(--bg)",
      font: "600 12px system-ui, sans-serif",
    });
    document.body.appendChild(pill);
    dt.setDragImage(pill, 10, 10);
    setTimeout(() => pill.remove(), 0);
  } catch {
    // the default drag image is fine
  }
}

export function endTrackDrag() {
  trackDrag.active = false;
  trackDrag.count = 0;
}

export const hasTrackDrag = (e: DragEvent) => !!e.dataTransfer?.types.includes(TRACKS_MIME);

/** The track ids a drop carries (empty if it is not a tracks drop or the payload is malformed). */
export function readTrackDrag(e: DragEvent): number[] {
  const raw = e.dataTransfer?.getData(TRACKS_MIME);
  return raw ? parseTrackIds(raw) : [];
}
