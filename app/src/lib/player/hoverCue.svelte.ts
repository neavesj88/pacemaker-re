// Where the pointer is hovering, for "hover, then Space moves the cue". The mix timeline and the
// player bar's waveform each report their surface and the time under the pointer (already snapped
// when it will be); the single Space handler in init.ts reads it. Cleared on pointer leave, on a
// window blur and when the surface goes away.
import type { CueSurface } from "./hoverCue";

class HoverCue {
  surface = $state<CueSurface | null>(null);
  /** The time Space would move the cue to, seconds on the surface's own scale. */
  secs = $state(0);
  /** Alt is held: Space puts the cue exactly under the pointer, with no snapping. */
  exact = $state(false);

  set(surface: CueSurface, secs: number) {
    this.surface = surface;
    this.secs = secs;
  }

  /** `surface` only: a surface leaving must not clear another one's hover. */
  clear(surface?: CueSurface) {
    if (surface === undefined || this.surface === surface) this.surface = null;
  }
}

export const hoverCue = new HoverCue();
