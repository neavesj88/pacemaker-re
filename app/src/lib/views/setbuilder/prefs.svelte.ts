// The set builder's settings. A per-person preference of the app, so it lives in the browser
// storage like the mix editor's auto-transition settings do (try/catch-guarded: the view works the
// same when storage is unavailable, it just forgets).
import { DEFAULT_PREFS, mergePrefs, type SetPrefs } from "./solver";

const STORAGE_KEY = "pm.setbuilder.prefs.v1";

class SetBuilderPrefs {
  value = $state<SetPrefs>({ ...DEFAULT_PREFS, allowedMoves: [...DEFAULT_PREFS.allowedMoves] });

  constructor() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) this.value = mergePrefs(JSON.parse(raw));
    } catch {
      // unavailable or corrupt: keep the defaults
    }
  }

  set(patch: Partial<SetPrefs>) {
    this.value = mergePrefs({ ...this.value, ...patch });
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.value));
    } catch {
      // not remembered this time
    }
  }

  reset() {
    this.set({ ...DEFAULT_PREFS, allowedMoves: [...DEFAULT_PREFS.allowedMoves] });
  }
}

export const setPrefs = new SetBuilderPrefs();
