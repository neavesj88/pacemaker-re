// Browser stand-in for the live mix engine (api/mixLive.ts): decoding takes a moment, per clip,
// and a clock runs while "playing". It makes no sound; it exists so the Mix tab's transport,
// loading indicator and playhead can be checked without the native shell.
import { emit } from "@tauri-apps/api/event";
import type { Handlers } from "./types";

type Doc = { clips?: unknown[]; lengthSec?: number };

let doc: Doc | null = null;
let sec = 0;
let playing = false;
let loaded = 0;
let total = 0;
let state: "idle" | "loading" | "ready" | "playing" | "error" = "idle";
let clock: ReturnType<typeof setInterval> | null = null;
let loader: ReturnType<typeof setInterval> | null = null;
let lastTick = 0;

const status = () => void emit("mix-live-status", { state, loaded, total, message: "" });
const position = () => void emit("mix-live-position", { sec, playing });

function stopClock() {
  if (clock) clearInterval(clock);
  clock = null;
}

function setState(next: typeof state) {
  state = next;
  status();
}

export const handlers: Handlers = {
  mix_live_load: (a) => {
    doc = a.doc as Doc;
    total = doc.clips?.length ?? 0;
    loaded = 0;
    sec = 0;
    playing = false;
    stopClock();
    if (loader) clearInterval(loader);
    setState(total ? "loading" : "ready");
    // one clip "decodes" every 350 ms, so a long mix shows its progress
    loader = setInterval(() => {
      loaded = Math.min(total, loaded + 1);
      if (loaded >= total) {
        if (loader) clearInterval(loader);
        loader = null;
        setState(playing ? "playing" : "ready");
      } else status();
    }, 350);
    position();
  },
  mix_live_update: (a) => {
    doc = a.doc as Doc;
    const n = doc.clips?.length ?? 0;
    if (n !== total) {
      total = n;
      loaded = Math.min(loaded, total);
      status();
    }
  },
  mix_live_play: (a) => {
    sec = Number(a.fromSec) || 0;
    playing = true;
    lastTick = performance.now();
    stopClock();
    clock = setInterval(() => {
      const now = performance.now();
      sec += (now - lastTick) / 1000;
      lastTick = now;
      const end = doc?.lengthSec ?? 0;
      if (end > 0 && sec >= end) {
        sec = end;
        playing = false;
        stopClock();
        setState("ready");
      }
      position();
    }, 33);
    setState(loaded >= total ? "playing" : "loading");
    position();
  },
  mix_live_pause: () => {
    playing = false;
    stopClock();
    setState(loaded >= total ? "ready" : "loading");
    position();
  },
  mix_live_seek: (a) => {
    sec = Math.max(0, Number(a.sec) || 0);
    lastTick = performance.now();
    position();
  },
  mix_live_set_master: () => undefined,
  mix_live_unload: () => {
    doc = null;
    playing = false;
    loaded = 0;
    total = 0;
    stopClock();
    if (loader) clearInterval(loader);
    loader = null;
    setState("idle");
  },
};
