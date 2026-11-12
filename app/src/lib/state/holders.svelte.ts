// "Which cases hold this track": the dialog the track menu opens and the click-through into the
// Cases view that the dialog and the inspector's chips share. Reads only.
import { type TrackHolding, casesHolding } from "$lib/api/cases";
import { cases } from "./cases.svelte";
import { nav } from "./nav.svelte";
import { session } from "./session.svelte";

class Holders {
  /** The track whose cases the dialog shows, null when closed. */
  trackId = $state<number | null>(null);
  holding = $state.raw<TrackHolding | null>(null);
  loading = $state(false);
  error = $state("");
  #seq = 0;

  async show(trackId: number) {
    const root = session.library?.root;
    if (!root) return;
    const seq = ++this.#seq;
    this.trackId = trackId;
    this.holding = null;
    this.error = "";
    this.loading = true;
    try {
      const [h] = await casesHolding(root, [trackId]);
      if (seq === this.#seq) this.holding = h ?? { track_id: trackId, cases: [], smart: [] };
    } catch (e) {
      if (seq === this.#seq) this.error = String(e);
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  close() {
    this.#seq++;
    this.trackId = null;
    this.holding = null;
  }

  /** Opens a case in the Cases view (after the editor there has asked about unsaved edits). */
  goToCase(caseId: number) {
    const c = session.library?.cases.find((x) => x.id === caseId);
    if (!c) return;
    this.close();
    cases.leave(() => {
      nav.go("cases");
      void cases.openCase(c);
    });
  }

  goToSmart(id: number) {
    this.close();
    cases.leave(() => {
      nav.go("cases");
      cases.openSmart(id);
    });
  }
}

export const holders = new Holders();
session.onReset(() => holders.close());
