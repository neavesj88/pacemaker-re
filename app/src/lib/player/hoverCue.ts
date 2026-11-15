// "Hover the mouse, then press Space" moves the cue. Pure rules (no Svelte, no DOM), so they run
// under plain node: which surface the pointer is over, what Space should do about it, and the
// time maths from a pointer position to a time. The state that remembers the hover is
// hoverCue.svelte.ts; the one Space handler that acts on it is init.ts.

/** The two places Space can move the cue: the mix timeline and the player bar's waveform. */
export type CueSurface = "mix" | "player";

export type SpaceAction = "move-mix-cue" | "move-player-cue" | "default";

export type SpaceInputs = {
  /** Which surface the pointer is over, or null (over nothing that takes a cue). */
  surface: CueSurface | null;
  /** A text field has the focus: Space is for typing. */
  typing: boolean;
  /** A control, dialog or menu wants Space itself. */
  ownedByControl: boolean;
  /** A mix keeps playing on another tab, so the bar says "Space pauses it". */
  mixPlayingElsewhere: boolean;
};

/** What Space does now. Anything that is not a clear hover over a surface keeps the usual play /
 *  pause rules (spaceClaim.ts), which the caller applies on "default". */
export function decideSpace(i: SpaceInputs): SpaceAction {
  if (i.typing || i.ownedByControl || i.surface === null) return "default";
  if (i.surface === "mix") return "move-mix-cue";
  // the bar's own "a mix is playing, Space pauses it" rule keeps priority over its waveform
  return i.mixPlayingElsewhere ? "default" : "move-player-cue";
}

/** Seconds under the pointer on the mix timeline, or null over the lane headers (a sticky column
 *  that covers part of the scrolled content) or outside the box. `left` is the scroller's left edge. */
export function timelineSecs(clientX: number, left: number, headerW: number, scrollLeft: number, pps: number): number | null {
  if (!(pps > 0)) return null;
  const inView = clientX - left;
  if (inView < headerW) return null;
  return Math.max(0, (inView - headerW + scrollLeft) / pps);
}

/** Seconds under the pointer on the player's waveform: it spans `span` seconds starting at
 *  `fromSecs`. Null for a box with no width or no length. */
export function waveSecs(clientX: number, left: number, width: number, fromSecs: number, span: number): number | null {
  if (!(width > 0) || !(span > 0)) return null;
  const f = Math.min(1, Math.max(0, (clientX - left) / width));
  return fromSecs + f * span;
}

/** The cue time for a press: the nearest beat in `beats` (seconds) when snapping is on and Alt is
 *  not held and a grid exists, else exactly `secs`. */
export function cueTime(secs: number, beats: number[], snapOn: boolean, exact: boolean): number {
  if (!snapOn || exact || beats.length === 0) return secs;
  let best = beats[0];
  for (const b of beats) if (Math.abs(b - secs) < Math.abs(best - secs)) best = b;
  return best;
}

/** The hairline's label: m:ss.s (tenths, so a snapped beat and an exact time can be told apart). */
export function hoverLabel(secs: number): string {
  const t = Math.max(0, Math.round(secs * 10) / 10);
  const m = Math.floor(t / 60);
  const s = t - m * 60;
  return `${m}:${s < 10 ? "0" : ""}${s.toFixed(1)}`;
}
