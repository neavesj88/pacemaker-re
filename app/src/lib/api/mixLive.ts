// The live mix player (src-tauri/src/commands/mixlive.rs): plays the open, possibly unsaved mix in
// real time through the device's audio output, with no render step. An edit is applied while it
// plays, without a click. The DSP is the export's, so what plays is what exports.
//
// Contract (both the Mix screen and the engine use exactly this): load -> update* -> play / pause /
// seek -> unload; the player reports through two events. A browser preview answers these commands
// from devmock/mixLive.ts (a running clock, no sound).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PreviewGains } from "./export";
import type { MixDoc } from "./mixes";

export const MIX_LIVE_POSITION = "mix-live-position";
export const MIX_LIVE_STATUS = "mix-live-status";

export type MixLivePosition = { sec: number; playing: boolean };
export type MixLiveState = "idle" | "loading" | "ready" | "playing" | "error";
export type MixLiveStatus = {
  state: MixLiveState;
  /** Clips decoded so far, of `total` distinct audio files in the mix. */
  loaded: number;
  total: number;
  /** What to tell the user: an underrun, a clip that cannot be played, no output device. */
  message: string;
};

/** What `mixLiveLoad` and `mixLiveUpdate` can carry besides the document. */
export type MixLiveExtras = {
  /** The loudness gains of the export settings (per clip and master): the live mix then plays at the
   * level an export with the same settings has. Omitted on an update = unchanged. */
  gains?: PreviewGains;
  /** Update only: read the card's Vocal flag again (it decides what Wah tools sound like). */
  refreshVocal?: boolean;
  /** Ids of the Wah tools the app marks as Vocal tools (views/mixes/vocalTools.svelte.ts). The
   * device file has no Vocal tool, so only the app knows; the live player renders those as the
   * vocal filter and every other Wah tool as Wah. Omitted on an update = unchanged. */
  vocalToolIds?: string[];
};

/** Starts decoding the clips of `doc` in the background; resolves once the mix is accepted (the
 * player is stopped at 0 and `mix-live-status` reports the decoding). Replaces a mix loaded before. */
export const mixLiveLoad = (root: string, doc: MixDoc, extras: MixLiveExtras = {}) => invoke<void>("mix_live_load", { root, doc, gains: extras.gains ?? null, vocalToolIds: extras.vocalToolIds ?? null });

/** An edit, while stopped or playing; applied live without a click. */
export const mixLiveUpdate = (doc: MixDoc, extras: MixLiveExtras = {}) => invoke<void>("mix_live_update", { doc, gains: extras.gains ?? null, refreshVocal: extras.refreshVocal ?? null, vocalToolIds: extras.vocalToolIds ?? null });

/** Plays from `fromSec` (mix timeline seconds); rejects with the reason when there is no audio output. */
export const mixLivePlay = (fromSec: number) => invoke<void>("mix_live_play", { fromSec });

export const mixLivePause = () => invoke<void>("mix_live_pause");

/** Moves the position; keeps playing when it was playing. */
export const mixLiveSeek = (sec: number) => invoke<void>("mix_live_seek", { sec });

/** The listening level of the live mix in dB (0 = as exported), moved without a click. */
export const mixLiveSetMaster = (gainDb: number) => invoke<void>("mix_live_set_master", { gainDb });

/** Stops, frees the decoded audio and closes the audio device. Fine with nothing loaded. */
export const mixLiveUnload = () => invoke<void>("mix_live_unload");

/** About 30 times a second while playing, and once on every change of state. */
export const onMixLivePosition = (cb: (p: MixLivePosition) => void) => listen<MixLivePosition>(MIX_LIVE_POSITION, (e) => cb(e.payload));

export const onMixLiveStatus = (cb: (s: MixLiveStatus) => void) => listen<MixLiveStatus>(MIX_LIVE_STATUS, (e) => cb(e.payload));
