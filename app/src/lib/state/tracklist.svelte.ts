// The track list dialog's state and settings. The settings are per-viewer conveniences kept in
// this browser's storage (like the track table's columns): they describe how the lists are written,
// not anything on the device, and the dialog says so. The pure parts are in tracklistprefs.ts.
import type { TracklistFormat, TracklistOptions } from "$lib/api/export";
import { defaultPrefs, mergePrefs, type CueIfExists, type TracklistPrefs } from "./tracklistprefs";
import { loadPref, savePref } from "./prefStore";

const STORAGE_KEY = "pacemaker-revival.tracklist";

function load(): TracklistPrefs {
  return loadPref(STORAGE_KEY, mergePrefs, () => defaultPrefs());
}

class Tracklist {
  /** The dialog is open. */
  open = $state(false);
  options = $state<TracklistOptions>(defaultPrefs().options);
  /** Write a CUE sheet beside every audio export of a mix. */
  writeCue = $state(false);
  /** An export finds its .cue already there: ask first (default) or replace it. */
  cueIfExists = $state<CueIfExists>("ask");
  format = $state<TracklistFormat>("txt");
  #loaded = false;

  /** Reads the saved settings once (storage can be missing or blocked: the defaults then apply). */
  ensureLoaded() {
    if (this.#loaded) return;
    this.#loaded = true;
    const p = load();
    this.options = p.options;
    this.writeCue = p.writeCue;
    this.cueIfExists = p.cueIfExists;
    this.format = p.format;
  }

  /** A plain copy of the options, safe to send over IPC. */
  snapshot(): TracklistOptions {
    return { ...this.options };
  }

  save() {
    savePref(STORAGE_KEY, { options: this.options, writeCue: this.writeCue, cueIfExists: this.cueIfExists, format: this.format });
  }
}

export const tracklist = new Tracklist();
