// Unsaved work that opening another library would throw away. An area that keeps edits in memory
// registers a guard (see shell/leaveGuards.ts); `Session.confirmLeave` asks the user before a library
// is replaced. Pure, so it runs under plain node (leaveGuard.test.mjs).

export type UnsavedWork = {
  /** One line per thing that would be lost, naming it ("The mix “Friday”: 3 unsaved edits"). */
  lines: string[];
  /** Writes it. Absent when saving needs a question of its own (the grid editor confirms its write).
   *  Resolves false when it did not save. */
  save?: () => Promise<boolean>;
};

type Guard = () => UnsavedWork | null;

const guards: Guard[] = [];

/** Registers a guard; it returns the unsaved work right now, or null when there is none. */
export function registerLeaveGuard(fn: Guard) {
  guards.push(fn);
}

/** Everything unsaved at this moment. */
export function unsavedWork(): UnsavedWork[] {
  const out: UnsavedWork[] = [];
  for (const g of guards) {
    const w = g();
    if (w && w.lines.length > 0) out.push(w);
  }
  return out;
}

const edits = (n: number) => (n === 1 ? "1 unsaved edit" : `${n} unsaved edits`);

/** The line for an open mix with unsaved edits. */
export function mixLine(name: string, undoDepth: number): string {
  return undoDepth > 0 ? `The mix “${name}”: ${edits(undoDepth)}` : `The mix “${name}”: unsaved changes`;
}

/** The line for the beat grid editor: what is unsaved on which track. */
export function gridLine(track: string, grid: boolean, cue: boolean): string {
  const what = grid && cue ? "beat grid and cue point" : cue ? "cue point" : "beat grid";
  return `The ${what} of “${track}”: unsaved changes`;
}

/** True when every piece of work can be saved from the question (so a Save button is honest). */
export const allSaveable = (work: UnsavedWork[]) => work.length > 0 && work.every((w) => !!w.save);
