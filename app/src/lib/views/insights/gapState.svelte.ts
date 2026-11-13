// State of the Library gap map that outlives the component: its settings (remembered in the
// browser, like the Tracks columns), which grid is showing and which cell is picked. The picked
// cell is kept by bin ids, so it survives a re-bin after a setting changes (and goes away if its
// row or column no longer exists).
import { session } from "$lib/state/session.svelte";
import { DEFAULT_SETTINGS, mergeSettings, settingNotice, type GapSettings, type MatrixId } from "./gapLogic";

const STORAGE_KEY = "pm.insights.gap.v1";

function load(): GapSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return mergeSettings(raw ? JSON.parse(raw) : null);
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

class GapState {
  settings = $state<GapSettings>(load());
  matrix = $state<MatrixId>("bpm-key");
  /** The picked cell, by row and column bin id. */
  picked = $state<{ row: string; col: string } | null>(null);

  /** Says in words when the last change was not used as typed (clamped, or the range adjusted). */
  notice = $state("");

  /** Applies one edit; returns the settings now in effect so the caller can show them. */
  update(patch: Partial<GapSettings>): GapSettings {
    const edited = (Object.keys(patch) as (keyof GapSettings)[])[0];
    const before = $state.snapshot(this.settings);
    const next = mergeSettings({ ...before, ...patch }, edited);
    this.notice = edited ? settingNotice(edited, patch[edited] as number, next, before) : "";
    this.settings = next;
    this.save();
    return next;
  }

  resetSettings() {
    this.notice = "";
    this.settings = { ...DEFAULT_SETTINGS };
    this.save();
  }

  private save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify($state.snapshot(this.settings)));
    } catch {
      /* private mode or full storage: the settings just last for this session */
    }
  }

  /** A different library was opened. */
  reset() {
    this.picked = null;
  }
}

export const gap = new GapState();
session.onReset(() => gap.reset());
