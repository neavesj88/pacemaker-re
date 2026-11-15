// The text of the sidebar's analysis buttons (energy, sections). They open a panel that runs the
// analysis, so they read as actions: "Energy 32 / 32…" looked like finished progress and
// "Sections 0 / 32…" like a failure.

/** "Find sections for 32 tracks…" while some are left; "Sections: all 32 done…" once none is. */
export function analysisLabel(noun: string, action: string, ready: number, total: number): string {
  const left = Math.max(0, total - ready);
  if (left === 0) return `${noun}: all ${total} done…`;
  return `${action} for ${left} track${left === 1 ? "" : "s"}…`;
}

/** Tracks the background worker still has to try (never analysed, or the file changed since). Flat and
 *  skipped ones were tried and gave nothing: waiting for playback does not change that, so they must
 *  not keep the sidebar's "Analysis waiting" note up. */
export function untried(s: { none: number; stale: number }): number {
  return Math.max(0, s.none + s.stale);
}
