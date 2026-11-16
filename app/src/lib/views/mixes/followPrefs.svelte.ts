// How the view follows the playhead while a mix (and the beat grid editor's track) plays. Kept in
// the browser storage like the other mix settings (try/catch-guarded: everything works without
// storage). Defaults: scroll smoothly, holding the playhead 40 % in from the left.
import { loadPref, savePref } from "$lib/state/prefStore";
import { DEFAULT_FOLLOW_PREFS, mergeFollowPrefs, type FollowPrefs } from "./followLogic";

const STORAGE_KEY = "pm.mixes.follow.v1";

class FollowSettings {
  value = $state<FollowPrefs>({ ...DEFAULT_FOLLOW_PREFS });

  constructor() {
    this.value = loadPref(STORAGE_KEY, mergeFollowPrefs, () => ({ ...DEFAULT_FOLLOW_PREFS }));
  }

  set(patch: Partial<FollowPrefs>) {
    this.value = mergeFollowPrefs({ ...this.value, ...patch });
    savePref(STORAGE_KEY, this.value);
  }
}

export const followPrefs = new FollowSettings();
