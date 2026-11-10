// Track ids highlighted in whichever table is showing. Shared so any view (and the player and
// inspector) can read it. Area-specific selection (the open case, the open mix) lives in that
// area's own store.
import { session } from "./session.svelte";

class Selection {
  trackIds = $state<number[]>([]);

  reset() {
    this.trackIds = [];
  }
}

export const selection = new Selection();
session.onReset(() => selection.reset());
