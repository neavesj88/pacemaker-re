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

  /** Tracks the owner said to leave alone, or edited by hand, this session (not stored). */
  accepted = $state<ReadonlySet<number>>(new Set());
  /** Offered tracks the owner unticked in the open offer. */
  unticked = $state<ReadonlySet<number>>(new Set());

  /** Tracks of the open library whose BPM lies outside the window now in force, less those the
   *  owner has accepted as they are. */
  outside = $derived(outsideRange(session.library?.tracks ?? [], this.lo, session.needs).filter((t) => !this.accepted.has(t.track_id)));

  /** The offered tracks that are ticked. */
  ticked = $derived(this.outside.filter((t) => !this.unticked.has(t.track_id)));

  toggle(id: number) {
    const next = new Set(this.unticked);
    if (!next.delete(id)) next.add(id);
    this.unticked = next;
  }

  /** Stops offering these tracks (a BPM set, halved or doubled on purpose, or "leave alone"). */
  accept(ids: Iterable<number>) {
    this.accepted = new Set([...this.accepted, ...ids]);
  }

  /** "Leave these alone": every listed track stops being offered for the session. */
  leaveAlone() {
    this.accept(this.outside.map((t) => t.track_id));
    this.offerOpen = false;
  }

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

  /** Bumped when saving fails, so the radios are drawn again from the saved window. */
  failed = $state(0);

  /** Saves a new window. When it leaves tracks of the open library outside it, opens the offer to
   *  re-analyse them. */
  async choose(lo: number) {
    if (lo === this.lo) return;
    try {
      const saved = await bpmRangeSet({ lo });
      this.lo = normaliseLo(saved.lo);
    } catch (e) {
      toast.show(String(e), "bad");
      this.failed++;
      return;
    }
    this.openOffer(true);
  }

  /** Opens the offer. `onlyIfAny`: stay closed when there is nothing to offer. */
  openOffer(onlyIfAny = false) {
    if (onlyIfAny && this.outside.length === 0) return;
    this.unticked = new Set();
    this.offerOpen = true;
  }

  closeOffer() {
    this.offerOpen = false;
  }

  /** Step two: closes the offer and asks the usual re-analyse confirmation, which names every
   *  track and warns about hand-set grids and the mixes that use them. */
  reanalyse() {
    const ids = this.ticked.map((t) => t.track_id);
    this.offerOpen = false;
    if (ids.length > 0) void jobs.askReanalyse(ids);
  }
}

export const bpmRange = new BpmRange();
session.onReset(() => {
  bpmRange.accepted = new Set();
  bpmRange.unticked = new Set();
});
