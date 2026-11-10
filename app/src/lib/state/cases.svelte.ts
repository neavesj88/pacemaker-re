// The case open in the Cases view, if any, and its track ids in order.
import type { Collection } from "$lib/api/types";
import { session } from "./session.svelte";

class Cases {
  open = $state<Collection | null>(null);
  trackIds = $state<number[]>([]);

  close() {
    this.open = null;
    this.trackIds = [];
  }
}

export const cases = new Cases();
session.onReset(() => cases.close());
