// The set builder's settings. A per-person preference of the app, so it lives in the browser
// storage like the mix editor's auto-transition settings do (try/catch-guarded: the view works the
// same when storage is unavailable, it just forgets).
import { clampPref, DEFAULT_PREFS, mergePrefs, type NumberPref, type SetPrefs } from "./solver";
import { loadPref, savePref } from "$lib/state/prefStore";

const STORAGE_KEY = "pm.setbuilder.prefs.v1";

class SetBuilderPrefs {
  value = $state<SetPrefs>({ ...DEFAULT_PREFS, allowedMoves: [...DEFAULT_PREFS.allowedMoves] });

  constructor() {
    // nothing saved, unreadable or corrupt: the defaults stay
    this.value = loadPref(STORAGE_KEY, (raw) => (raw === null ? this.value : mergePrefs(raw)), () => this.value);
  }

  set(patch: Partial<SetPrefs>) {
    this.value = mergePrefs({ ...this.value, ...patch }, this.value);
    savePref(STORAGE_KEY, this.value);
  }

  /** Applies what was typed into a number box: clamped into the setting's range; a blank box sets
   * a setting that may be unset (`nullable`) to unset and is otherwise ignored. Either way the box is
   * rewritten with the value now in force, so it never shows a number the setting does not hold. */
  commitNumber(el: HTMLInputElement, key: NumberPref, nullable = false) {
    const raw = el.value.trim();
    const n = Number(raw);
    if (raw === "" ? nullable : Number.isFinite(n)) this.set({ [key]: raw === "" ? null : clampPref(key, n) });
    const now = this.value[key];
    el.value = now == null ? "" : String(now);
  }

  reset() {
    this.set({ ...DEFAULT_PREFS, allowedMoves: [...DEFAULT_PREFS.allowedMoves] });
  }
}

export const setPrefs = new SetBuilderPrefs();
