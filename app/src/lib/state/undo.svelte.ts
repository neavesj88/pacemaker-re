// The undo history of rating, tag and favourite edits for the open library (device area, "trust
// pack"). The history itself lives in the backend (commands/device/undo.rs) and is fed by the
// edit commands; this store mirrors it, shows the Undo pill after an edit, and runs an undo.
//
// An undo is always: preview (what each line becomes, what is left alone and why) then apply with
// the id of the newest entry the preview saw. The pill is the one-step shortcut for the edit that
// was just made: its text names the edit, and the result names what was put back.
import {
  deviceTrustSettings,
  deviceTrustSettingsSet,
  deviceUndoApply,
  deviceUndoList,
  deviceUndoPreview,
  type TrustSettings,
  type UndoChanged,
  type UndoEntry,
  type UndoReport,
} from "$lib/api/device";
import { cases } from "./cases.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

export const DEFAULT_SETTINGS: TrustSettings = { undo_limit: 20, dead_row_guard_percent: 50, undo_pill_seconds: 10 };

class UndoStore {
  entries = $state<UndoEntry[]>([]);
  /** The volume key of the library the list belongs to (events carry it). */
  key = $state("");
  settings = $state<TrustSettings>({ ...DEFAULT_SETTINGS });
  maxLimit = $state(200);
  settingsWarning = $state<string | null>(null);
  /** The edit that was just made, offered for a one-click undo. */
  pill = $state<{ id: number; text: string } | null>(null);
  busy = $state(false);
  #timer: ReturnType<typeof setTimeout> | undefined;
  #gen = 0;

  clear() {
    this.entries = [];
    this.key = "";
    this.dismissPill();
  }

  dismissPill() {
    clearTimeout(this.#timer);
    this.pill = null;
  }

  async loadSettings() {
    try {
      const v = await deviceTrustSettings();
      this.settings = v.settings;
      this.maxLimit = v.max_undo_limit;
      this.settingsWarning = v.warning;
    } catch (e) {
      this.settingsWarning = String(e);
    }
  }

  async saveSettings(next: TrustSettings) {
    try {
      const v = await deviceTrustSettingsSet(next);
      this.settings = v.settings;
      this.settingsWarning = v.warning;
      toast.show("Saved", "ok");
    } catch (e) {
      toast.error("Couldn't save the setting.", e);
    }
  }

  /** Reads the history of the open library. */
  async refresh() {
    const root = session.library?.root;
    if (!root) return;
    const gen = ++this.#gen;
    try {
      const v = await deviceUndoList(root);
      if (gen !== this.#gen || session.library?.root !== root) return;
      this.entries = v.entries;
      this.key = v.key;
    } catch (e) {
      session.error = String(e);
    }
  }

  /** An `undo-changed` event from the backend. */
  async onChanged(ev: UndoChanged) {
    if (this.key && ev.key !== this.key) return;
    await this.refresh();
    if (ev.why === "recorded" && ev.summary) {
      const secs = this.settings.undo_pill_seconds;
      if (secs > 0) {
        clearTimeout(this.#timer);
        this.pill = { id: ev.id, text: ev.summary };
        this.#timer = setTimeout(() => this.dismissPill(), secs * 1000);
      }
    } else if (ev.why !== "recorded") {
      this.dismissPill();
    }
  }

  /** What undoing the newest `count` edits would do, line by line. */
  async preview(count: number) {
    const root = session.library?.root;
    // async, so "no library" is a rejection the caller's handler sees, not a throw from the call
    if (!root) throw "No library is open.";
    return deviceUndoPreview(root, count);
  }

  /** Applies a previewed undo. Returns the report, or null if it failed (already reported). */
  async apply(count: number, topEntryId: number): Promise<UndoReport | null> {
    const root = session.library?.root;
    if (!root || this.busy) return null;
    this.busy = true;
    try {
      const report = await deviceUndoApply(root, count, topEntryId);
      await this.afterUndo(report);
      return report;
    } catch (e) {
      toast.error("Couldn't undo.", e);
      await this.refresh();
      return null;
    } finally {
      this.busy = false;
    }
  }

  /** The pill's one click: undo the newest edit. */
  async undoNewest() {
    const id = this.pill?.id;
    this.dismissPill();
    try {
      const p = await this.preview(1);
      if (id && p.top_entry_id !== id) {
        toast.show("Not the newest edit. Use the edit history.", "bad");
        return;
      }
      await this.apply(1, p.top_entry_id);
    } catch (e) {
      toast.error("Couldn't check the last edit.", e);
    }
  }

  async afterUndo(report: UndoReport) {
    const root = session.library?.root;
    const done = report.items.filter((i) => !i.skip);
    const left = report.items.filter((i) => i.skip);
    if (done.length === 0) {
      toast.show(left.length ? "Nothing put back: it changed since." : "Nothing to put back.", "info", { details: left.map((i) => `${i.label}, ${i.what.toLowerCase()} ${i.skip}`).join("\n") || undefined });
    } else {
      const first = done[0];
      const what = done.length === 1 ? `${first.what.toLowerCase()} is ${first.becomes} again` : `${done.length} changes put back`;
      toast.show(`Undone: ${what}${left.length ? `; ${left.length} left alone` : ""}.`, "ok", { details: done.length === 1 ? first.label : undefined });
    }
    if (root) {
      await session.load(root, true);
      if (done.some((i) => i.what === "Favourites")) await cases.refresh();
    }
  }
}

export const undo = new UndoStore();
session.onReset(() => undo.clear());
