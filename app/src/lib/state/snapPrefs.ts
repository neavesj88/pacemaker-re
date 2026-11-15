// The mix editor's Snap setting: on or off, and what it lines clips up on. Pure (no Svelte, no
// storage), so it runs under plain node (scripts/test-mixedit.mjs); the remembered copy is
// views/mixes/snapPrefs.svelte.ts.

/** What a dragged clip is lined up on: beats, bars (the downbeat), 8-bar phrases, or where
 * detected sections begin. */
export type SnapGrain = "beat" | "bar" | "phrase" | "section";
export const SNAP_GRAINS: SnapGrain[] = ["beat", "bar", "phrase", "section"];

export type SnapPrefs = {
  /** Snapping on. Alt held while dragging moves freely whatever this says. */
  on: boolean;
  grain: SnapGrain;
};

export const DEFAULT_SNAP_PREFS: SnapPrefs = { on: true, grain: "bar" };

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergeSnapPrefs(saved: unknown): SnapPrefs {
  const out: SnapPrefs = { ...DEFAULT_SNAP_PREFS };
  if (!saved || typeof saved !== "object") return out;
  const s = saved as Record<string, unknown>;
  if (typeof s.on === "boolean") out.on = s.on;
  if (typeof s.grain === "string" && (SNAP_GRAINS as string[]).includes(s.grain)) out.grain = s.grain as SnapGrain;
  return out;
}

/** The editor's snap mode for these settings. */
export const snapModeOf = (p: SnapPrefs): SnapGrain | "off" => (p.on ? p.grain : "off");

/** The grain the Beat-align button uses: the setting's, with "section" (which has no beat grid of
 * its own) meaning bars. */
export const alignGrainOf = (p: SnapPrefs): "beat" | "bar" | "phrase" => (p.grain === "section" ? "bar" : p.grain);
