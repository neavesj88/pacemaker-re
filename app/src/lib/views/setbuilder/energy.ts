// Where the set builder reads a track's energy from.
//
// Energy is a number from 1 (calm) to 10 (peak), or null when nothing is known. Nothing in the app
// computes or stores it yet, so this returns null for every track and the energy curve in the
// settings says so and stays off. When energy is added (a tag column, an analysis job), change
// this one function to read it; the solver, the list and the settings need no other edit.
//
// It is synchronous on purpose: the solver is pure and the list calls it for every row.
export function energyOf(_trackId: number): number | null {
  return null;
}
