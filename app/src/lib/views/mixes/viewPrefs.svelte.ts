// The mix editor's View options, remembered between sessions in the browser storage like the other
// mix settings (try/catch-guarded: everything works without storage). The View menu in the mix
// editor and Settings > Mixes > Mix editor view both read and write this one copy.
import { loadPref, savePref } from "$lib/state/prefStore";
import { DEFAULT_VIEW_PREFS, mergeViewPrefs, type ViewPrefs } from "./viewPrefsLogic";

const STORAGE_KEY = "pm.mixes.view.v1";

class ViewSettings {
  value = $state<ViewPrefs>({ ...DEFAULT_VIEW_PREFS });

  constructor() {
    this.value = loadPref(STORAGE_KEY, mergeViewPrefs, () => ({ ...DEFAULT_VIEW_PREFS }));
  }

  set(patch: Partial<ViewPrefs>) {
    this.value = mergeViewPrefs({ ...this.value, ...patch });
    savePref(STORAGE_KEY, this.value);
  }
}

export const viewPrefs = new ViewSettings();
