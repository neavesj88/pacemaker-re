// The printable setlist's open request and the user's print options. Anything can ask for one:
// `setlist.openCase(id)` / `setlist.openMix(id)` (the Cases and Mixes views have a button); the
// dialog in Overlays.svelte does the rest. Options are a saved user setting (browser storage of
// the app window, like the matcher's); the notes and header text of each setlist go through
// api/setlist.ts into the app's config folder.
import { type Options, cleanOptions } from "./setlistLogic";

const STORAGE_KEY = "pacemaker-revival.setlist.v1";

function loadOptions(): Options {
  try {
    const s = localStorage.getItem(STORAGE_KEY);
    if (s) return cleanOptions(JSON.parse(s));
  } catch {
    // blocked or damaged storage: the defaults are used
  }
  return cleanOptions(undefined);
}

export type SetlistRequest = { kind: "case" | "mix"; id: number };

class SetlistUi {
  request = $state<SetlistRequest | null>(null);
  options = $state<Options>(loadOptions());

  openCase(id: number) {
    this.request = { kind: "case", id };
  }
  openMix(id: number) {
    this.request = { kind: "mix", id };
  }
  close() {
    this.request = null;
  }

  /** Remembers the options for the next time. */
  persist() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify($state.snapshot(this.options)));
    } catch {
      // storage blocked: the options last until the window closes
    }
  }
}

export const setlist = new SetlistUi();
