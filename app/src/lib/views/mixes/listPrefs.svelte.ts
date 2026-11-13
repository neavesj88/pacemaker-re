// Settings of the mix list (sort, what search looks in, what a bulk export does with a file that
// exists) and of the tool menu. Per-person preferences of the app, kept in the browser storage
// like the recipe settings are (try/catch-guarded: everything works without storage).
import { DEFAULT_LIST_PREFS, mergeListPrefs, type ListPrefs } from "./mixlist";
import { loadPref, savePref } from "$lib/state/prefStore";

const LIST_KEY = "pm.mixes.list.v1";
const TOOLS_KEY = "pm.mixes.tools.v1";

class ListSettings {
  value = $state<ListPrefs>({ ...DEFAULT_LIST_PREFS });

  constructor() {
    // nothing saved, unreadable or corrupt: the defaults stay
    this.value = loadPref(LIST_KEY, (raw) => (raw === null ? this.value : mergeListPrefs(raw)), () => this.value);
  }

  set(patch: Partial<ListPrefs>) {
    this.value = mergeListPrefs({ ...this.value, ...patch });
    savePref(LIST_KEY, this.value);
  }
}

export const listPrefs = new ListSettings();

class ToolSettings {
  /** Offer the Trans tool in the editor's tool menu. Its file layout has not been checked on a
   * real Pacemaker (notes/mix-format.md: no document with a Trans tool has been seen), so it is
   * opt-in: off until the person turns it on in the Mixes settings. */
  showTrans = $state(false);

  constructor() {
    this.showTrans = loadPref(TOOLS_KEY, (raw) => ((raw as { showTrans?: unknown } | null)?.showTrans === true ? true : this.showTrans), () => this.showTrans);
  }

  setShowTrans(on: boolean) {
    this.showTrans = on;
    savePref(TOOLS_KEY, { showTrans: on });
  }
}

export const toolPrefs = new ToolSettings();
