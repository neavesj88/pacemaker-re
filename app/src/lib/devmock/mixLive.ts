// Mock of src-tauri/src/commands/mixlive.rs for the browser preview. There is no audio: the mock
// simulates decoding progress (one clip every 250 ms) and a running clock that emits
// `mix-live-position` at about 30 Hz while playing, so the Mix screen behaves as it does in the app.
import { emit } from "@tauri-apps/api/event";
import type { MixDoc } from "$lib/api/mixes";
import type { MixLivePosition, MixLiveStatus } from "$lib/api/mixLive";
import type { Handlers } from "./types";

const POSITION = "mix-live-position";
const STATUS = "mix-live-status";

let doc: MixDoc | null = null;
let loaded = 0;
let total = 0;
let playing = false;
let sec = 0;
let from = 0;
let startedAt = 0;
let clock: ReturnType<typeof setInterval> | undefined;
let loader: ReturnType<typeof setInterval> | undefined;

/** Seconds of the mix; the mock plays to the end of the last clip. */
const length = () => doc?.lengthSec ?? 0;
const files = (d: MixDoc) => new Set(d.clips.map((c) => c.trackId)).size;

function status(message = ""): MixLiveStatus {
  const state = !doc ? "idle" : playing ? "playing" : loaded < total ? "loading" : "ready";
  return { state, loaded, total, message };
}
const sendStatus = (message = "") => void emit(STATUS, status(message));
const sendPosition = () => void emit(POSITION, { sec, playing } satisfies MixLivePosition);

function stopClock() {
  if (clock) clearInterval(clock);
  clock = undefined;
}

function tick() {
  sec = from + (performance.now() - startedAt) / 1000;
  if (sec >= length()) {
    sec = length();
    playing = false;
    stopClock();
    sendPosition();
    sendStatus();
    return;
  }
  sendPosition();
}

export const handlers: Handlers = {
  mix_live_load: ({ doc: d }) => {
    stopClock();
    if (loader) clearInterval(loader);
    doc = d as MixDoc;
    playing = false;
    sec = 0;
    loaded = 0;
    total = files(doc);
    loader = setInterval(() => {
      loaded = Math.min(total, loaded + 1);
      sendStatus();
      if (loaded >= total && loader) clearInterval(loader);
    }, 250);
    sendStatus();
    sendPosition();
  },
  mix_live_update: ({ doc: d }) => {
    if (!doc) throw "No mix is loaded in the live player.";
    doc = d as MixDoc;
    total = files(doc);
    loaded = Math.min(loaded, total);
    // a clip the mix did not have before decodes now
    if (loaded < total && !loader) {
      loader = setInterval(() => {
        loaded = Math.min(total, loaded + 1);
        sendStatus();
        if (loaded >= total && loader) {
          clearInterval(loader);
          loader = undefined;
        }
      }, 250);
    }
    if (sec > length()) sec = length();
  },
  mix_live_play: ({ fromSec }) => {
    if (!doc) throw "No mix is loaded in the live player.";
    from = Math.max(0, Number(fromSec));
    if (from >= length() - 0.05) from = 0;
    sec = from;
    startedAt = performance.now();
    playing = true;
    stopClock();
    clock = setInterval(tick, 33);
    sendPosition();
    sendStatus();
  },
  mix_live_pause: () => {
    if (playing) tick();
    playing = false;
    stopClock();
    sendPosition();
    sendStatus();
  },
  mix_live_seek: ({ sec: s }) => {
    sec = Math.max(0, Math.min(length(), Number(s)));
    from = sec;
    startedAt = performance.now();
    sendPosition();
  },
  mix_live_set_master: () => {},
  mix_live_unload: () => {
    stopClock();
    if (loader) clearInterval(loader);
    loader = undefined;
    doc = null;
    playing = false;
    sec = 0;
    loaded = total = 0;
    sendPosition();
    sendStatus();
  },
};
