// How the open case's table looks: which columns show, the view sort, and the merge dialog's
// "delete the source cases" choice. Remembered in the browser (per viewer, not per library, and
// never on the device). Storage may be blocked or full; the view then just forgets.
import { CASE_COLUMNS, type CaseColKey, type CasePrefs, type CaseSortKey, defaultCasePrefs, mergeCasePrefs } from "./caseLogic";
import { loadPref, savePref } from "$lib/state/prefStore";

const STORAGE_KEY = "pm.cases.view.v1";

function load(): CasePrefs {
  return loadPref(STORAGE_KEY, mergeCasePrefs, () => defaultCasePrefs());
}

class CaseView {
  prefs = $state<CasePrefs>(load());

  /** Columns in table order, hidden ones left out. */
  shown = $derived(CASE_COLUMNS.filter((c) => c.fixed || this.prefs.visible[c.key]));

  /** Clicking a header: sort by it, or flip the direction if it already sorts. */
  sortBy(key: CaseSortKey) {
    if (this.prefs.sortKey === key) this.prefs.sortAsc = !this.prefs.sortAsc;
    else {
      this.prefs.sortKey = key;
      this.prefs.sortAsc = true;
    }
    this.save();
  }

  /** Back to the case's own order. */
  useCaseOrder() {
    this.prefs.sortKey = "order";
    this.prefs.sortAsc = true;
    this.save();
  }

  toggleColumn(key: CaseColKey) {
    this.prefs.visible[key] = !this.prefs.visible[key];
    this.save();
  }

  setMergeDeletes(on: boolean) {
    this.prefs.mergeDeletesSources = on;
    this.save();
  }

  save() {
    savePref(STORAGE_KEY, $state.snapshot(this.prefs));
  }
}

export const caseView = new CaseView();
