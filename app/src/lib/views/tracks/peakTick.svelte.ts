// A per-track signal that a waveform which was missing is now there: a row's waveform effect reads
// `peakTick.of(location)` and asks again when it moves. Only the rows of the tracks that finished
// are touched, so a long background run does not redraw the table.
import { SvelteMap } from "svelte/reactivity";

const ticks = new SvelteMap<string, number>();

export const peakTick = {
  of(location: string): number {
    return ticks.get(location) ?? 0;
  },
  bump(location: string) {
    ticks.set(location, (ticks.get(location) ?? 0) + 1);
  },
};
