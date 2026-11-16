// Settings of the mix tab that are policy choices rather than document content: how tempo
// matching treats a big stretch, whether new clips are matched at all, and the width of the track
// table beside the timeline. Kept in the browser storage like the other mix settings
// (try/catch-guarded: everything works without storage).
import { loadPref, savePref } from "$lib/state/prefStore";

export { DEFAULT_TEMPO_PREFS, mergeTempoPrefs, type OnAddOverStretch, type TempoPrefs } from "./tempoPrefsLogic";
import { DEFAULT_TEMPO_PREFS, mergeTempoPrefs, type TempoPrefs } from "./tempoPrefsLogic";

const STORAGE_KEY = "pm.mixes.tempo.v1";

class TempoSettings {
  value = $state<TempoPrefs>({ ...DEFAULT_TEMPO_PREFS });

  constructor() {
    this.value = loadPref(STORAGE_KEY, mergeTempoPrefs, () => ({ ...DEFAULT_TEMPO_PREFS }));
  }

  set(patch: Partial<TempoPrefs>) {
    this.value = mergeTempoPrefs({ ...this.value, ...patch });
    savePref(STORAGE_KEY, this.value);
  }
}

export const tempoPrefs = new TempoSettings();
