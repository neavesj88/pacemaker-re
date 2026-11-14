// The BPM range analysis setting and the offer to re-analyse tracks that were analysed outside it.
// The range is kept by the backend (analysis_bpm_range_get / _set), because adding tracks and every
// re-analyse read it there. Which tracks were read under another window is not stored: a track
// whose BPM lies outside the window now in force is one (see `outsideRange`).
import { bpmRangeGet, bpmRangeSet } from "$lib/api/analysis";
import { DEFAULT_RANGE_LO, normaliseLo, outsideRange } from "./bpmRangeLogic";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

class BpmRange {
  /** Low end of the saved window (the high end is twice that). */
  lo = $state(DEFAULT_RANGE_LO);
  loaded = $state(false);
  /** The offer dialog is open. */
  offerOpen = $state(false);
  #loading: Promise<void> | null = null;

  /** Tracks of the open library whose BPM lies outside the window now in force. */
  outside = $derived(outsideRange(session.library?.tracks ?? [], this.lo, session.needs));

  /** Reads the saved window once (settings entry and library open both call it). */
  ensure() {
    this.#loading ??= bpmRangeGet().then(
      (s) => {
        this.lo = normaliseLo(s.lo);
        this.loaded = true;
      },
      () => {
        this.#loading = null;
      },
    );
    return this.#loading;
  }

  /** Saves a new window. When it leaves tracks of the open library outside it, opens the offer to
   *  re-analyse them. */
  async choose(lo: number) {
    if (lo === this.lo) return;
    try {
      const saved = await bpmRangeSet({ lo });
      this.lo = normaliseLo(saved.lo);
    } catch (e) {
      toast.show(String(e), "bad");
      return;
    }
    this.openOffer(true);
  }

  /** Opens the offer. `onlyIfAny`: stay closed when there is nothing to offer. */
  openOffer(onlyIfAny = false) {
    if (onlyIfAny && this.outside.length === 0) return;
    this.offerOpen = true;
  }

  closeOffer() {
    this.offerOpen = false;
  }

  /** Step two: closes the offer and asks the usual re-analyse confirmation, which names every
   *  track and warns about hand-set grids and the mixes that use them. */
  reanalyse() {
    const ids = this.outside.map((t) => t.track_id);
    this.offerOpen = false;
    if (ids.length > 0) void jobs.askReanalyse(ids);
  }
}

export const bpmRange = new BpmRange();
