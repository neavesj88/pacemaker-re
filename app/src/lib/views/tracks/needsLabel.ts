// The text of the sidebar's analysis buttons (energy, sections). They open a panel that runs the
// analysis, so they read as actions: "Energy 32 / 32…" looked like finished progress and
// "Sections 0 / 32…" like a failure.

/** "Find sections for 32 tracks…" while some are left; "Sections: all 32 done…" once none is. */
export function analysisLabel(noun: string, action: string, ready: number, total: number): string {
  const left = Math.max(0, total - ready);
  if (left === 0) return `${noun}: all ${total} done…`;
  return `${action} for ${left} track${left === 1 ? "" : "s"}…`;
}
