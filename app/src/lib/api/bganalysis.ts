// Typed wrappers over the background analysis commands (src-tauri/src/commands/bganalysis.rs).
// One worker on the Rust side makes the missing waveforms, keys, energy estimates and sections of
// the open library whichever view is shown; this is how the UI configures it and hears from it.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type BgKind = "peaks" | "keys" | "energy" | "sections";

export type BgSettings = {
  /** The master switch. Off: nothing is analysed in the background at all. */
  enabled: boolean;
  waveforms: boolean;
  keys: boolean;
  energy: boolean;
  sections: boolean;
  /** Wait while a track or a mix preview plays. */
  pause_during_playback: boolean;
  /** A detected key below this confidence (per cent, 0-100) is listed, not written. */
  key_min_confidence_pct: number;
};

export type BgState = "no-library" | "off" | "manual" | "paused" | "playing" | "waiting" | "running" | "idle" | "blocked" | "error";

export type BgStatus = {
  state: BgState;
  /** Words for "blocked" and "error". */
  detail: string;
  root: string | null;
  pacemaker: boolean;
  /** What is being made now: "waveforms", "keys", "energy" or "sections"; "" when nothing is. */
  word: string;
  /** Finished / to do in the lane being worked on. */
  done: number;
  total: number;
  /** To do over all lanes. */
  left: number;
  /** The track being read. */
  label: string;
  /** Tracks given up on. */
  failures: number;
};

export type BgFailure = { kind: BgKind; track_id: number; label: string; location: string; size: number; reason: string };

export type BgView = { settings: BgSettings; status: BgStatus; failures: BgFailure[] };

/** Finished work, so open views update without a reload. */
export type BgChanged =
  | { kind: "peaks"; root: string; locations: string[] }
  | { kind: "keys"; root: string; keys: { track_id: number; key: string }[] }
  | { kind: "energy"; root: string }
  | { kind: "sections"; root: string };

export const BG_STATUS = "bg-analysis-status";
export const BG_CHANGED = "bg-analysis-changed";

export const bgGet = () => invoke<BgView>("bganalysis_get");
/** Saves the settings (the backend checks them) and makes the worker look again at what is missing. */
export const bgSaveSettings = (settings: BgSettings) => invoke<BgSettings>("bganalysis_save_settings", { settings });
/** Tells the worker which library is open (null: none). Called on every open and in-place reload. */
export const bgWatch = (root: string | null) => invoke<void>("bganalysis_watch", { root });
export const bgSetPaused = (paused: boolean) => invoke<void>("bganalysis_set_paused", { paused });
/** The player is playing a track or a mix preview. */
export const bgSetPlaying = (playing: boolean) => invoke<void>("bganalysis_set_playing", { playing });
/** Starts the worker on the open library even if it is a Pacemaker (which it never starts on by itself). */
export const bgRunNow = (root: string) => invoke<void>("bganalysis_run_now", { root });
/** Forgets the tracks given up on, so they are tried again. */
export const bgRetryFailures = () => invoke<void>("bganalysis_retry_failures");

export const onBgStatus = (cb: (s: BgStatus) => void) => listen<BgStatus>(BG_STATUS, (e) => cb(e.payload));
export const onBgChanged = (cb: (c: BgChanged) => void) => listen<BgChanged>(BG_CHANGED, (e) => cb(e.payload));
