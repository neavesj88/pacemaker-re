// Settings of the mix list (sort, what search looks in, what a bulk export does with a file that
// exists) and of the tool menu. Per-person preferences of the app, kept in the browser storage
// like the recipe settings are (try/catch-guarded: everything works without storage).
import { DEFAULT_LIST_PREFS, mergeListPrefs, type ListPrefs } from "./mixlist";

const LIST_KEY = "pm.mixes.list.v1";
const TOOLS_KEY = "pm.mixes.tools.v1";

class ListSettings {
  value = $state<ListPrefs>({ ...DEFAULT_LIST_PREFS });

  constructor() {
    try {
      const raw = localStorage.getItem(LIST_KEY);
      if (raw) this.value = mergeListPrefs(JSON.parse(raw));
    } catch {
      // unavailable or corrupt: keep the defaults
    }
  }

  set(patch: Partial<ListPrefs>) {
    this.value = mergeListPrefs({ ...this.value, ...patch });
    try {
      localStorage.setItem(LIST_KEY, JSON.stringify(this.value));
    } catch {
      // not remembered this time
    }
  }
}

export const listPrefs = new ListSettings();

class ToolSettings {
  /** Offer the Trans tool in the editor's tool menu. Its file layout has not been checked on a
   * real Pacemaker (notes/mix-format.md), so this is a choice, on by default because the Editor
   * itself offers the tool. */
  showTrans = $state(true);

  constructor() {
    try {
      const raw = JSON.parse(localStorage.getItem(TOOLS_KEY) ?? "null");
      if (raw && typeof raw.showTrans === "boolean") this.showTrans = raw.showTrans;
    } catch {
      // keep the default
    }
  }

  setShowTrans(on: boolean) {
    this.showTrans = on;
    try {
      localStorage.setItem(TOOLS_KEY, JSON.stringify({ showTrans: on }));
    } catch {
      // not remembered this time
    }
  }
}

export const toolPrefs = new ToolSettings();
