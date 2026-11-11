// Waveform peaks of the tracks on the timeline, fetched once per track file through the analysis
// area's `peaks` contract and cached. `version` changes whenever one arrives, so a canvas that
// reads `get()` inside an $effect redraws itself.
import { peaks as fetchPeaks, type Peak } from "$lib/api/analysis";
import { session } from "$lib/state/session.svelte";
import { peakColumns } from "./layout";

class PeakCache {
  version = $state(0);
  #done = new Map<string, Peak[] | null>();
  #busy = new Set<string>();

  /** The peaks of a track, or null until they have arrived (or when they cannot be had). */
  get(location: string, durationSecs: number): Peak[] | null {
    void this.version;
    if (!location) return null;
    if (this.#done.has(location)) return this.#done.get(location) ?? null;
    if (!this.#busy.has(location)) void this.#load(location, durationSecs);
    return null;
  }

  async #load(location: string, durationSecs: number) {
    const root = session.library?.root;
    if (!root) return;
    this.#busy.add(location);
    try {
      this.#done.set(location, await fetchPeaks(root, location, peakColumns(durationSecs)));
    } catch {
      // no waveform for this file: the clip shows a plain bar, the beat ticks still draw
      this.#done.set(location, null);
    } finally {
      this.#busy.delete(location);
      this.version++;
    }
  }

  clear() {
    this.#done.clear();
    this.version++;
  }
}

export const peakCache = new PeakCache();
session.onReset(() => peakCache.clear());
