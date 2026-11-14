// Pure logic of the bulk tempo tools: reading a typed BPM and wording what a tempo edit will do to
// each track. No store or component imports so it runs under plain node: see tempoLogic.test.mjs.
import type { TempoOp, TempoRow } from "../api/grid";

/** Tempo range the device and the grid editor accept; the same numbers as `ops_grid::MIN_BPM`
 *  and `MAX_BPM` in pm-formats, which the backend enforces anyway. */
export const MIN_BPM = 20;
export const MAX_BPM = 400;

export type { TempoOp, TempoRow };

/** A typed tempo ("128", "127,5") as a number inside the allowed range, else an error message. */
export function parseTempo(text: string): { bpm: number } | { error: string } {
  const t = text.trim().replace(",", ".");
  if (t === "") return { error: "Type a tempo." };
  const n = Number(t);
  if (!Number.isFinite(n)) return { error: `"${text.trim()}" is not a number.` };
  if (n < MIN_BPM || n > MAX_BPM) return { error: `The tempo must be between ${MIN_BPM} and ${MAX_BPM} BPM.` };
  return { bpm: Math.round(n * 100) / 100 };
}

export const fmtBpm = (n: number) => n.toFixed(2);

/** The confirmation's line for a row: what happens to the track, in words. */
export function rowLine(r: TempoRow): string {
  if (r.skip !== null) return `${r.label}: left as it is (${r.skip})`;
  return `${r.label}: ${r.before === null ? "?" : fmtBpm(r.before)} → ${r.after === null ? "?" : fmtBpm(r.after)} BPM`;
}

/** Rows the job will change. */
export const changing = (rows: TempoRow[]) => rows.filter((r) => r.skip === null);

/** The confirmation's question: "Halve the BPM of 3 tracks on P:\ ?". */
export function opTitle(op: TempoOp, n: number, root: string): string {
  const tracks = `${n} track${n === 1 ? "" : "s"}`;
  switch (op.kind) {
    case "halve":
      return `Halve the BPM of ${tracks} on ${root} ?`;
    case "double":
      return `Double the BPM of ${tracks} on ${root} ?`;
    case "set":
      return `Set the BPM of ${tracks} to ${fmtBpm(op.bpm)} on ${root} ?`;
  }
}

export function opDescription(op: TempoOp): string {
  const both =
    "The beat grid (.str) and the BPM of each track are rewritten together, so they always agree. Each track's previous beat grid is first copied off the device (the app's data folder, next to the music.db snapshots), because Restore from a snapshot does not bring beat grids back.";
  switch (op.kind) {
    case "halve":
      return `Halves the tempo the way the grid editor's ½ BPM does: every second beat line is kept and the bar lines stay on the same beats. ${both}`;
    case "double":
      return `Doubles the tempo the way the grid editor's ×2 BPM does: a beat line is added midway between every two. ${both}`;
    case "set":
      return `Replaces the grid by an even grid at ${fmtBpm(op.bpm)} BPM starting at the track's first beat, the way the grid editor's Set BPM does. Tempo changes inside the track are lost; line the beats up by ear in the grid editor afterwards. ${both}`;
  }
}

/** Why nothing would change, for the message shown instead of an empty confirmation. */
export function nothingToDo(rows: TempoRow[]): string {
  if (rows.length === 0) return "No tracks to change.";
  const why = [...new Set(rows.map((r) => r.skip ?? ""))].filter(Boolean);
  return `Nothing would change: ${why.slice(0, 3).join("; ")}${why.length > 3 ? "; …" : ""}.`;
}

/** One tempo edit that was written: what was done and each changed track's tempo before and after.
 *  Kept for the session so the last edit can be taken back. */
export type TempoRun = { op: TempoOp; at: number; rows: { track_id: number; label: string; before: number; after: number }[] };

/** The edit that takes `op` back, or null when none does: halving undoes doubling and the other
 *  way round; "Set BPM" threw the old beat positions away, so only the saved copy of the old grid
 *  (named in the finished line) brings it back. */
export function inverseOp(op: TempoOp): TempoOp | null {
  if (op.kind === "double") return { kind: "halve" };
  if (op.kind === "halve") return { kind: "double" };
  return null;
}

/** The tracks of `run` whose tempo is still the one the run wrote, judged against `now` (the
 *  preview of the inverse edit). A track edited again since is left out, never undone over. */
export function stillAsWritten(run: TempoRun, now: TempoRow[]): number[] {
  const wrote = new Map(run.rows.map((r) => [r.track_id, r.after]));
  return now.filter((r) => r.skip === null && r.before !== null && Math.abs((wrote.get(r.track_id) ?? Number.NaN) - r.before) < 0.01).map((r) => r.track_id);
}

export function undoDescription(op: TempoOp): string {
  const inv = op.kind === "double" ? "Halves" : "Doubles";
  const lost =
    op.kind === "halve"
      ? " Doubling puts a beat midway between every two, so a grid that was uneven before the halving comes back even, and when the first bar line was on an odd beat the beat before the first kept one is gone for good. The grid copy saved off the device before the halving is the way back to the exact original."
      : "";
  return `Takes back the last tempo edit by running its opposite on the same tracks: ${inv} each track's tempo.${lost} Tracks whose tempo is not the one that edit wrote are left out.`;
}
