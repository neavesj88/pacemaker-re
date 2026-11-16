// Keeping the playhead in view while a mix (or, in the beat grid editor, a track) plays. Pure (no
// Svelte, no DOM), so it runs under plain node (followLogic.test.mjs). The numbers can be pixels or
// seconds: only their ratios matter. The remembered setting is followPrefs.svelte.ts.

/** "smooth": the view scrolls a little at a time so the playhead stays where it is on the screen.
 * "page": the view stays put until the playhead reaches the edge, then jumps. */
export type FollowMode = "smooth" | "page";

export type FollowPrefs = {
  mode: FollowMode;
  /** Smooth mode: how far from the left edge of the lanes (per cent of their width) the playhead
   * is held once it has got there. */
  anchorPct: number;
};

export const ANCHOR_MIN = 20;
export const ANCHOR_MAX = 80;
export const DEFAULT_FOLLOW_PREFS: FollowPrefs = { mode: "smooth", anchorPct: 40 };

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergeFollowPrefs(raw: unknown): FollowPrefs {
  const out = { ...DEFAULT_FOLLOW_PREFS };
  if (!raw || typeof raw !== "object") return out;
  const r = raw as Record<string, unknown>;
  if (r.mode === "smooth" || r.mode === "page") out.mode = r.mode;
  if (typeof r.anchorPct === "number" && Number.isFinite(r.anchorPct)) out.anchorPct = Math.min(ANCHOR_MAX, Math.max(ANCHOR_MIN, Math.round(r.anchorPct)));
  return out;
}

export type FollowInput = {
  /** The playhead, on the same scale as the scroll position. */
  playhead: number;
  /** The scroll position now (the content position at the left edge of the lanes). */
  scroll: number;
  /** Width of the lanes' viewport. */
  view: number;
  /** The furthest the lanes can scroll at all. */
  maxScroll: number;
  /** The furthest the following itself scrolls: the end of the content at the right edge of the view. */
  stop: number;
  prefs: FollowPrefs;
  /** The smallest step the scroll position can take (1 for pixels, one pixel's worth for seconds). Default 1. */
  unit?: number;
};

/** The scroll position to move to so the playhead stays in view, or null to leave the view alone.
 *
 * Smooth: nothing happens until the playhead reaches the anchor; from then on the view moves with
 * it, forwards only (it never scrolls back while the playhead is still on screen), and stops
 * where the end of the content meets the right edge, after which the playhead runs on to the edge.
 * A playhead off screen (a seek, or the view scrolled away) is brought to the anchor in one step.
 * Page: the view jumps when the playhead reaches the right edge, putting it near the left edge. */
export function followScroll(i: FollowInput): number | null {
  const { playhead, scroll, view, maxScroll, stop, prefs } = i;
  const unit = i.unit && i.unit > 0 ? i.unit : 1;
  if (!(view > 0) || !Number.isFinite(playhead)) return null;
  const off = playhead < scroll || playhead > scroll + view;
  let target: number;
  if (prefs.mode === "page") {
    if (!off && playhead <= scroll + view - Math.min(24, view * 0.05)) return null;
    target = Math.min(playhead - view * 0.15, maxScroll);
  } else {
    const want = playhead - (view * prefs.anchorPct) / 100;
    if (off) target = Math.min(want, maxScroll);
    else if (want <= scroll) return null;
    else target = Math.min(want, Math.max(stop, scroll));
  }
  // whole pixels: the browser snaps the scroll position to them anyway, and a canvas redraw is not worth less
  target = Math.round(Math.max(0, target) / unit) * unit;
  return Math.abs(target - scroll) < unit / 2 ? null : target;
}
