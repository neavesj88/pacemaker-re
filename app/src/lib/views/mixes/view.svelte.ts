// View state of the mix editor that is not part of the document: zoom, scroll, playhead, which
// panels and lanes are open, tracks waiting to be added. The document and its undo history are in
// state/mixes.svelte.ts; this file never edits a document.
import { session } from "$lib/state/session.svelte";
import { viewPrefs } from "./viewPrefs.svelte";
import { HEADER_W, PPS_MAX, PPS_MIN, SR, clamp, fromPx, maxScroll, toPx, zoomScroll } from "./layout";

/** Which envelope is drawn over the clips of the bar (one at a time, so the lines never fight). */
export type EnvOverlay = "none" | "gain" | "tempo" | "key";

const PICKER_KEY = "pm.mixes.pickerHeight.v1";
export const PICKER_MIN = 160;

/** The remembered height of the track table under the mix (px); null = not set yet. */
function loadPickerHeight(): number | null {
  try {
    const v = Number(localStorage.getItem(PICKER_KEY));
    return Number.isFinite(v) && v >= PICKER_MIN ? Math.min(2000, v) : null;
  } catch {
    return null;
  }
}

const SECTION_MARKERS_KEY = "pm.mixes.sectionMarkers.v1";

/** The remembered "draw section markers on the clips" choice (default on). Browser storage can be
 * unavailable or corrupt: then the default is used and the choice is not kept. */
function loadSectionMarkers(): boolean {
  try {
    return localStorage.getItem(SECTION_MARKERS_KEY) !== "0";
  } catch {
    return true;
  }
}

class MixView {
  /** Zoom: pixels per second of the timeline. */
  pps = $state(24);
  /** Scroll position and size of the lanes' viewport, in pixels (set by the timeline). */
  scrollLeft = $state(0);
  viewW = $state(900);
  /** Where the content ends (samples: the mix end or the playhead, plus the empty room after it).
   * The timeline keeps it current; zooming needs it to know how far the lanes can scroll. */
  endSamples = $state(0);
  /** Playhead, timeline samples. */
  playhead = $state(0);
  /** Keep the playhead in view while playing (a View option, remembered). */
  get follow() {
    return viewPrefs.value.follow;
  }
  set follow(on: boolean) {
    viewPrefs.set({ follow: on });
  }
  /** Following is paused because the user scrolled or zoomed the timeline themselves; Play, a seek
   * or "Follow again" brings it back. */
  followPaused = $state(false);
  /** The ruler is being pressed or dragged: following waits, so the view does not move under the pointer. */
  scrubbing = $state(false);
  pickerOpen = $state(true);
  /** Height of the track table under the mix, px (dragged by its top edge, remembered). */
  pickerH = $state(loadPickerHeight() ?? 380);
  /** The height the timeline pane has zoomed in (px, strip excluded): the editor column minus the transport and the
   * track table. Set by the editor; the timeline sizes its bar from it, not from its own (shrinking) pane. */
  paneFullH = $state(0);
  /** How much shorter the bar is than zoomed in (px): the pane gives this much to the track table. Set by the timeline. */
  barExtra = $state(0);
  /** The least the timeline pane can be (px) with the bar at its current height. Set by the timeline. */
  paneNeed = $state(0);
  /** An audio export is rendering. */
  exporting = $state(false);
  /** Progress of the running audio export, 0..1. */
  exportPct = $state(0);
  /** What the export is doing: measuring loudness first (about the first half), then rendering. */
  exportPhase = $state<"measuring" | "rendering">("rendering");
  /** The part of the timeline the export dialog has chosen (samples), shown as a band; null = none. */
  rangeBand = $state<{ from: number; to: number } | null>(null);
  /** A stretch marked by Shift-dragging the ruler (samples, from < to); the export dialog can take it as its time range. */
  marked = $state<{ from: number; to: number } | null>(null);
  /** Draw detected sections on the clips (also needs "show markers on waveforms" in the Sections
   * panel's settings). Clips of tracks without sections draw nothing either way. */
  sectionMarkers = $state(loadSectionMarkers());
  /** The envelope drawn over the clips (toolbar buttons Gain / Tempo / Key). */
  overlay = $state<EnvOverlay>("none");
  /** Library tracks the tracks view asked to add; consumed when a mix is open. */
  pending = $state.raw<number[]>([]);

  setSectionMarkers(on: boolean) {
    this.sectionMarkers = on;
    try {
      localStorage.setItem(SECTION_MARKERS_KEY, on ? "1" : "0");
    } catch {
      // not remembered this time
    }
  }

  setPickerHeight(h: number, save = false) {
    this.pickerH = Math.max(PICKER_MIN, Math.min(2000, Math.round(h)));
    if (!save) return;
    try {
      localStorage.setItem(PICKER_KEY, String(this.pickerH));
    } catch {
      // not remembered this time
    }
  }

  x(samples: number) {
    return toPx(samples, this.pps);
  }

  samples(px: number) {
    return fromPx(px, this.pps);
  }

  /** `playing`: the mix is playing. While the view follows it, zoom holds the playhead where it is
   * and following carries on (pausing it let the playhead run off the screen mid-mix); otherwise
   * zooming is the user looking around, and following would pull the view back. */
  setZoom(pps: number, anchorPx?: number, playing = false) {
    const next = clamp(pps, PPS_MIN, PPS_MAX);
    if (next === this.pps) return;
    // keep the sample under `anchorPx` (a pixel inside the lanes) where it is. The lanes cannot
    // scroll past their new end, so the position is clamped here: a position the element refuses
    // left this state far from the screen, and clips and waveforms were drawn for the wrong part
    const lane = Math.max(0, this.viewW - HEADER_W);
    const head = this.x(this.playhead) - this.scrollLeft;
    const holdHead = playing && this.follow && !this.followPaused && head >= 0 && head <= lane;
    if (!holdHead) this.followPaused = true;
    const a = clamp(holdHead ? head : (anchorPx ?? lane / 2), 0, lane);
    this.scrollLeft = zoomScroll(this.scrollLeft, this.pps, next, a, this.endSamples, this.viewW);
    this.pps = next;
  }

  pauseFollow() {
    this.followPaused = true;
  }

  resumeFollow() {
    this.followPaused = false;
  }

  /** Scrolls the lanes to `px`, within what they can scroll. */
  scrollTo(px: number) {
    this.scrollLeft = clamp(px, 0, maxScroll(this.endSamples, this.pps, this.viewW));
  }

  /** Zooms so that `samples` fills the viewport. */
  fit(samples: number) {
    const sec = Math.max(10, samples / SR);
    this.pps = clamp((this.viewW - HEADER_W - 40) / sec, PPS_MIN, PPS_MAX);
    this.scrollLeft = 0;
    this.followPaused = true;
  }

  reset() {
    this.scrollLeft = 0;
    this.playhead = 0;
    this.scrubbing = false;
    this.marked = null;
  }
}

export const mv = new MixView();
session.onReset(() => {
  mv.reset();
  mv.pending = [];
});
