// The Snap setting, remembered between sessions in the browser storage like the other mix
// settings (try/catch-guarded: everything works without storage). Defaults: on, by bar.
import { DEFAULT_SNAP_PREFS, mergeSnapPrefs, type SnapPrefs } from "$lib/state/snapPrefs";
import { loadPref, savePref } from "$lib/state/prefStore";

const STORAGE_KEY = "pm.mixes.snap.v1";

class SnapSettings {
  value = $state<SnapPrefs>({ ...DEFAULT_SNAP_PREFS });

  constructor() {
    this.value = loadPref(STORAGE_KEY, mergeSnapPrefs, () => ({ ...DEFAULT_SNAP_PREFS }));
  }

  set(patch: Partial<SnapPrefs>) {
    this.value = mergeSnapPrefs({ ...this.value, ...patch });
    savePref(STORAGE_KEY, this.value);
  }
}

export const snapPrefs = new SnapSettings();
