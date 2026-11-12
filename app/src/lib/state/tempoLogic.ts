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
  const both = "The beat grid (.str) and the BPM of each track are rewritten together, so they always agree.";
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
