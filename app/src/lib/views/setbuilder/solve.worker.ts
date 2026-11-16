// The set solver's worker: see solveRunner.ts.
import { handleSolve, type SolveInput } from "./solveHandler.ts";

self.onmessage = (e: MessageEvent<SolveInput>) => {
  self.postMessage(handleSolve(e.data));
};
