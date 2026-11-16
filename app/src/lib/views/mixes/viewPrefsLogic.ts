// The mix editor's View options: how tall the clip lane is while zooming, the whole-mix strip, and
// whether the view follows the playhead. Pure (no Svelte, no storage), so it runs under plain node
// (viewPrefsLogic.test.mjs); the remembered copy is viewPrefs.svelte.ts.

/** "shrink": the clip lane gets shorter as the timeline is zoomed out (see clipBarHeight in
 * layout.ts). "full": the lane keeps the height it has zoomed in, at every zoom. */
export type ClipHeightMode = "shrink" | "full";

export type ViewPrefs = {
  clipHeight: ClipHeightMode;
  /** The thin strip above the ruler that shows the whole mix. */
  overview: boolean;
  /** Scroll the timeline with the playhead while the mix plays (how: followPrefs). */
  follow: boolean;
};

export const DEFAULT_VIEW_PREFS: ViewPrefs = { clipHeight: "shrink", overview: true, follow: true };

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergeViewPrefs(raw: unknown): ViewPrefs {
  const out = { ...DEFAULT_VIEW_PREFS };
  if (!raw || typeof raw !== "object") return out;
  const r = raw as Record<string, unknown>;
  if (r.clipHeight === "shrink" || r.clipHeight === "full") out.clipHeight = r.clipHeight;
  if (typeof r.overview === "boolean") out.overview = r.overview;
  if (typeof r.follow === "boolean") out.follow = r.follow;
  return out;
}
