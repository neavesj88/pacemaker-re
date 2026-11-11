// View state of the mix editor that is not part of the document: zoom, scroll, playhead, which
// panels and lanes are open, tracks waiting to be added. The document and its undo history are in
// state/mixes.svelte.ts; this file never edits a document.
import { session } from "$lib/state/session.svelte";
import { HEADER_W, PPS_MAX, PPS_MIN, SR, clamp, fromPx, toPx } from "./layout";

export type EnvLaneId = "tempo" | "key" | "gain";

class MixView {
  /** Zoom: pixels per second of the timeline. */
  pps = $state(24);
  /** Scroll position and size of the lanes' viewport, in pixels (set by the timeline). */
  scrollLeft = $state(0);
  viewW = $state(900);
  /** Playhead, timeline samples. */
  playhead = $state(0);
  /** Keep the playhead in view while playing. */
  follow = $state(true);
  pickerOpen = $state(true);
  /** An audio export is rendering. */
  exporting = $state(false);
  lanes = $state<Record<EnvLaneId, boolean>>({ tempo: false, key: false, gain: true });
  /** Library tracks the tracks view asked to add; consumed when a mix is open. */
  pending = $state.raw<number[]>([]);

  x(samples: number) {
    return toPx(samples, this.pps);
  }

  samples(px: number) {
    return fromPx(px, this.pps);
  }

  setZoom(pps: number, anchorPx?: number) {
    const next = clamp(pps, PPS_MIN, PPS_MAX);
    if (next === this.pps) return;
    // keep the sample under `anchorPx` (a pixel inside the viewport) where it is
    const a = anchorPx ?? this.viewW / 2;
    const at = (this.scrollLeft + a) / this.pps;
    this.pps = next;
    this.scrollLeft = Math.max(0, at * next - a);
  }

  /** Zooms so that `samples` fills the viewport. */
  fit(samples: number) {
    const sec = Math.max(10, samples / SR);
    this.pps = clamp((this.viewW - HEADER_W - 40) / sec, PPS_MIN, PPS_MAX);
    this.scrollLeft = 0;
  }

  reset() {
    this.scrollLeft = 0;
    this.playhead = 0;
  }
}

export const mv = new MixView();
session.onReset(() => {
  mv.reset();
  mv.pending = [];
});
