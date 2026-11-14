// The live mix engine (commands/mix_live.rs, built by the audio area): plays the open, possibly
// unsaved mix as it stands, with no render step. The contract, which both sides follow:
//
//   mixLiveLoad(root, doc)     starts decoding the clips in the background; resolves when accepted
//   mixLiveUpdate(doc)         an edit, while stopped or playing; applied live, glitch-free
//   mixLivePlay(fromSec)       starts playing at a mix time in seconds
//   mixLivePause()             stops, keeping the position
//   mixLiveSeek(sec)           moves the position, playing or not
//   mixLiveSetMaster(gainDb)   preview level; a listening aid, not part of the mix
//   mixLiveUnload()            drops the decoded audio
//
// Events: "mix-live-position" at about 30 Hz while playing and once on every state change, and
// "mix-live-status" (idle | loading | ready | playing | error, with how many clips are decoded).
//
// `vocalToolIds` rides along on load and update as an extra argument: the ids of Wah tools the app
// marks as Vocal tools (views/mixes/vocalTools.svelte.ts). The file has no Vocal tool, so only the
// app knows; an engine that wants to render them as the vocal filter reads it, one that does not
// ignores it.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { MixDoc } from "./mixes";

export type MixLivePosition = { sec: number; playing: boolean };
export type MixLiveState = "idle" | "loading" | "ready" | "playing" | "error";
export type MixLiveStatus = { state: MixLiveState; loaded: number; total: number; message: string };

export const MIX_LIVE_POSITION = "mix-live-position";
export const MIX_LIVE_STATUS = "mix-live-status";

export const mixLiveLoad = (root: string, doc: MixDoc, vocalToolIds: string[] = []) => invoke<void>("mix_live_load", { root, doc, vocalToolIds });
export const mixLiveUpdate = (doc: MixDoc, vocalToolIds: string[] = []) => invoke<void>("mix_live_update", { doc, vocalToolIds });
export const mixLivePlay = (fromSec: number) => invoke<void>("mix_live_play", { fromSec });
export const mixLivePause = () => invoke<void>("mix_live_pause");
export const mixLiveSeek = (sec: number) => invoke<void>("mix_live_seek", { sec });
export const mixLiveSetMaster = (gainDb: number) => invoke<void>("mix_live_set_master", { gainDb });
export const mixLiveUnload = () => invoke<void>("mix_live_unload");

export const onMixLivePosition = (cb: (p: MixLivePosition) => void) => listen<MixLivePosition>(MIX_LIVE_POSITION, (e) => cb(e.payload));
export const onMixLiveStatus = (cb: (s: MixLiveStatus) => void) => listen<MixLiveStatus>(MIX_LIVE_STATUS, (e) => cb(e.payload));
