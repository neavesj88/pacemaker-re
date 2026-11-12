// Suspect beat grids: which tracks of the open library look half or double tempo, or poorly
// analysed. The rules are the user's settings (remembered in the browser storage of the app) and are
// applied by the backend (commands/grid/suspect.rs), so the filter, the table marks and the
// "suggested fix" batch operation always agree. The check runs when a library loads or reloads
// (switchable) and when the rules change; `views/grid/Overlays.svelte` calls `sync`.
import { gridSuspects, type Suspect, type SuspectReport, type SuspectSettings } from "$lib/api/grid";
import type { LibraryView } from "$lib/api/types";
import { DEFAULT_SUSPECT_SETTINGS, normaliseSuspect, SUSPECT_STORAGE, validateSettings, type StoredSuspect } from "./batchLogic";
import { session } from "./session.svelte";

function load(): StoredSuspect {
  try {
    const raw = localStorage.getItem(SUSPECT_STORAGE);
    return normaliseSuspect(raw ? JSON.parse(raw) : null);
  } catch {
    return normaliseSuspect(null);
  }
}

class GridSuspectsStore {
  /** The rules, and whether the library is checked whenever it loads. */
  rules = $state<SuspectSettings>(load().rules);
  auto = $state(load().auto);

  report = $state<SuspectReport | null>(null);
  checking = $state(false);
  /** Why the last check failed, "" when it did not. */
  error = $state("");
  /** The settings dialog is open. */
  editing = $state(false);

  /** Flagged tracks by id. */
  byId = $derived(new Map<number, Suspect>((this.report?.suspects ?? []).map((s) => [s.track_id, s])));

  private seen: LibraryView | null = null;
  private run = 0;

  get count() {
    return this.report?.suspects.length ?? 0;
  }

  /** Saves the rules (the caller has checked them with `validateSettings`) and checks again. */
  save(rules: SuspectSettings, auto: boolean) {
    this.rules = rules;
    this.auto = auto;
    try {
      localStorage.setItem(SUSPECT_STORAGE, JSON.stringify({ rules: $state.snapshot(rules), auto }));
    } catch {
      // storage blocked or full: the rules still apply to this run, they are just forgotten
    }
    void this.check();
  }

  resetRules() {
    this.save(structuredClone(DEFAULT_SUSPECT_SETTINGS), this.auto);
  }

  /** Called whenever the open library object changes: checks it when `auto` is on, forgets the
   *  old result otherwise. */
  sync(lib: LibraryView | null) {
    if (lib === this.seen) return;
    this.seen = lib;
    this.error = "";
    if (!lib) {
      this.report = null;
      this.run++;
      this.checking = false;
      return;
    }
    if (this.auto) void this.check();
    else this.report = null;
  }

  /** Checks the open library now. Stale answers (the library or the rules changed meanwhile) are dropped. */
  async check() {
    const lib = session.library;
    if (!lib) return;
    const problem = validateSettings(this.rules);
    if (problem) {
      this.error = problem;
      return;
    }
    const mine = ++this.run;
    this.checking = true;
    this.error = "";
    try {
      const report = await gridSuspects(lib.root, $state.snapshot(this.rules));
      if (mine === this.run) this.report = report;
    } catch (e) {
      if (mine === this.run) {
        this.report = null;
        this.error = String(e);
      }
    } finally {
      if (mine === this.run) this.checking = false;
    }
  }

  /** Makes sure there is a result for the open library, checking if there is none yet. A failed
   *  check is not retried here (callers run inside effects that track `report` and `checking`, so
   *  retrying would loop); a library change, new rules or the Rules dialog try again. */
  async ensure() {
    if (!this.report && !this.checking && !this.error) await this.check();
  }
}

export const suspects = new GridSuspectsStore();
session.onReset(() => {
  suspects.report = null;
});
