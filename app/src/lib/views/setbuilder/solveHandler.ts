// What the solver worker does with one message, kept apart from the worker itself so that a test
// (and the inline fallback in solveRunner.ts) can run it without one.
import { solve, type SetTrack, type SolveResult, type SolverOptions } from "./solver.ts";

export type SolveInput = { pool: SetTrack[]; pins: [number, number][]; o: SolverOptions };
export type SolveReply = { ok: true; result: SolveResult } | { ok: false; error: string };

export function handleSolve(input: SolveInput): SolveReply {
  try {
    return { ok: true, result: solve(input.pool, new Map(input.pins), input.o) };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

