// Runs the set solver off the page's thread. Ordering a few hundred tracks takes from one to
// several seconds (a full pair-cost table, then a local search), and run on the page's own thread
// it froze the whole window for that long, including on opening the builder, which solves as it
// loads. A module worker gets the same input and returns the same answer (the solver is pure and
// has no clock or randomness), so the set on screen is the one the old inline call gave.
//
// Where a worker cannot be made (no Worker in the runtime, or the script fails to start) the solve
// runs inline, as before: slower to respond, never wrong.
import { handleSolve, type SolveInput, type SolveReply } from "./solveHandler.ts";
import type { SetTrack, SolveResult, SolverOptions } from "./solver.ts";

export type SolveRun = { result: Promise<SolveResult>; cancel: () => void };

export function runSolve(pool: readonly SetTrack[], pins: ReadonlyMap<number, number>, o: SolverOptions): SolveRun {
  const input: SolveInput = { pool: pool.map((t) => ({ ...t })), pins: [...pins], o: { ...o } };
  let worker: Worker | null = null;
  const result = new Promise<SolveResult>((resolve, reject) => {
    const inline = () => {
      const r = handleSolve(input);
      if (r.ok) resolve(r.result);
      else reject(new Error(r.error));
    };
    if (typeof Worker === "undefined") return inline();
    try {
      worker = new Worker(new URL("./solve.worker.ts", import.meta.url), { type: "module" });
    } catch {
      return inline();
    }
    worker.onmessage = (e: MessageEvent<SolveReply>) => {
      worker?.terminate();
      worker = null;
      if (e.data.ok) resolve(e.data.result);
      else reject(new Error(e.data.error));
    };
    worker.onerror = (e) => {
      // the worker could not run at all: do it here
      e.preventDefault();
      worker?.terminate();
      worker = null;
      inline();
    };
    worker.postMessage(input);
  });
  return {
    result,
    cancel: () => {
      worker?.terminate();
      worker = null;
    },
  };
}
