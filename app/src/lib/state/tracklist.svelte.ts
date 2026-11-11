// The track list dialog's state and settings. The settings are per-viewer conveniences kept in
// this browser's storage (like the track table's columns): they describe how the lists are written,
// not anything on the device, and the dialog says so. The pure parts are in tracklistprefs.ts.
import type { TracklistFormat, TracklistOptions } from "$lib/api/export";
import { defaultPrefs, mergePrefs, type TracklistPrefs } from "./tracklistprefs";

const STORAGE_KEY = "pacemaker-revival.tracklist";

function load(): TracklistPrefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return mergePrefs(raw ? JSON.parse(raw) : null);
  } catch {
    return defaultPrefs();
  }
}

class Tracklist {
  /** The dialog is open. */
  open = $state(false);
  options = $state<TracklistOptions>(defaultPrefs().options);
  /** Write a CUE sheet beside every audio export of a mix. */
  writeCue = $state(false);
  format = $state<TracklistFormat>("txt");
  #loaded = false;

  /** Reads the saved settings once (storage can be missing or blocked: the defaults then apply). */
  ensureLoaded() {
    if (this.#loaded) return;
    this.#loaded = true;
    const p = load();
    this.options = p.options;
    this.writeCue = p.writeCue;
    this.format = p.format;
  }

  /** A plain copy of the options, safe to send over IPC. */
  snapshot(): TracklistOptions {
    return { ...this.options };
  }

  save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ options: this.options, writeCue: this.writeCue, format: this.format }));
    } catch {
      // private window, blocked or full storage: the settings still apply, they are just forgotten
    }
  }
}

export const tracklist = new Tracklist();
