// Mock of src-tauri/src/commands/bganalysis.rs: a simulated background worker for the browser
// preview. It follows the real rules in small (lane order waveforms, keys, energy, sections; a
// library on this computer starts by itself, a Pacemaker only after Run now; paused, a playing
// preview, a running job and the master switch hold it; a track it cannot decode is listed and not
// tried again until "Try again") but does not read any audio: a made waveform just stops being
// held (bgHeld.ts), a key is a seeded value, energy and sections go into the other mocks' caches.
// Picked up automatically by devmock/index.ts.
import { emit } from "@tauri-apps/api/event";
import { libraryKind } from "../state/place";
import { bgEnergy } from "./analysis";
import { bgSections } from "./analysisSections";
import { heldPeaks, UNREADABLE_TRACK } from "./bgHeld";
import { jobLock } from "./jobs";
import { libraryFor } from "./libraries";
import type { Handlers } from "./types";

type Kind = "peaks" | "keys" | "energy" | "sections";
type MTrack = ReturnType<typeof libraryFor>["tracks"][number];
type Failure = { kind: Kind; track_id: number; label: string; location: string; size: number; reason: string };

const KINDS: Kind[] = ["peaks", "keys", "energy", "sections"];
const WORD: Record<Kind, string> = { peaks: "waveforms", keys: "keys", energy: "energy", sections: "sections" };
const TICK_MS = 380;
const KEYS = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

let settings = { enabled: true, waveforms: true, keys: true, energy: true, sections: true, pause_during_playback: true, key_min_confidence_pct: 0 };
let root: string | null = null;
let manualRoot: string | null = null;
let paused = false;
let playing = false;
let failures: Failure[] = [];
let lanes: { kind: Kind; items: MTrack[]; total: number; done: number }[] | null = null;
let timer: ReturnType<typeof setInterval> | null = null;
let status = blank("no-library");

function blank(state: string) {
  return { state, detail: "", root: null as string | null, pacemaker: false, word: "", done: 0, total: 0, left: 0, label: "", failures: 0 };
}

const wants = (k: Kind) =>
  settings.enabled && { peaks: settings.waveforms, keys: settings.keys, energy: settings.energy, sections: settings.sections }[k];

const label = (t: MTrack) => `${t.artist} - ${t.title}`;
const failed = (k: Kind, t: MTrack) => failures.some((f) => f.kind === k && f.location === t.location && f.size === t.file_size);

function missing(k: Kind, tracks: MTrack[], computer: boolean): MTrack[] {
  switch (k) {
    case "peaks":
      return tracks.filter((t) => heldPeaks.has(t.location));
    case "keys":
      return tracks.filter((t) => !t.key.trim());
    case "energy": {
      if (!computer) return [];
      const m = new Set(bgEnergy.missing().map((t) => t.track_id));
      return tracks.filter((t) => m.has(t.track_id));
    }
    case "sections": {
      if (!computer) return [];
      const m = new Set(bgSections.missing().map((t) => t.track_id));
      return tracks.filter((t) => m.has(t.track_id));
    }
  }
}

function plan() {
  if (!root) return;
  const lib = libraryFor(root);
  const computer = libraryKind(root) === "computer";
  lanes = KINDS.filter(wants).map((kind) => {
    const items = missing(kind, lib.tracks, computer).filter((t) => !failed(kind, t));
    return { kind, items, total: items.length, done: 0 };
  });
}

function hold(): string | null {
  if (!root) return "no-library";
  if (!KINDS.some(wants)) return "off";
  if (libraryKind(root) === "pacemaker" && manualRoot !== root) return "manual";
  if (paused) return "paused";
  if (settings.pause_during_playback && playing) return "playing";
  if (jobLock.running) return "waiting";
  return null;
}

const left = () => (lanes ?? []).reduce((n, l) => n + l.items.length, 0);

function publish(patch: Partial<ReturnType<typeof blank>> = {}) {
  status = { ...status, root, pacemaker: root ? libraryKind(root) === "pacemaker" : false, failures: failures.length, left: left(), ...patch };
  void emit("bg-analysis-status", status);
}

function stateOnly(state: string) {
  const clear = ["no-library", "off", "manual"].includes(state);
  publish({ state, ...(clear ? { word: "", label: "", done: 0, total: 0, left: 0 } : {}) });
}

function tick() {
  const h = hold();
  if (h) {
    if (status.state !== h) stateOnly(h);
    return;
  }
  if (!lanes) plan();
  const lane = lanes?.find((l) => l.items.length > 0 && wants(l.kind));
  if (!lane || !root) {
    if (status.state !== "idle") publish({ state: "idle", word: "", label: "", left: 0 });
    return;
  }
  const t = lane.items.shift()!;
  lane.done++;
  if (lane.kind === "peaks" && t.track_id === UNREADABLE_TRACK) {
    failures.push({ kind: "peaks", track_id: t.track_id, label: label(t), location: t.location, size: t.file_size, reason: "could not decode the file" });
  } else if (lane.kind === "peaks") {
    heldPeaks.delete(t.location);
    void emit("bg-analysis-changed", { kind: "peaks", root, locations: [t.location] });
  } else if (lane.kind === "keys") {
    let s = 0;
    for (const c of t.location) s = (s * 31 + c.charCodeAt(0)) >>> 0;
    const key = KEYS[s % 12] + (s % 5 < 2 ? "m" : "");
    t.key = key;
    void emit("bg-analysis-changed", { kind: "keys", root, keys: [{ track_id: t.track_id, key }] });
  } else if (lane.kind === "energy") {
    bgEnergy.make(t as never);
    void emit("bg-analysis-changed", { kind: "energy", root });
  } else {
    bgSections.make(t as never);
    void emit("bg-analysis-changed", { kind: "sections", root });
  }
  publish({ state: "running", word: WORD[lane.kind], done: lane.done, total: lane.total, label: label(t), detail: "" });
}

function ensureTimer() {
  timer ??= setInterval(tick, TICK_MS);
}

const replan = () => {
  lanes = null;
  tick();
};

export const handlers: Handlers = {
  bganalysis_get: () => ({ settings: { ...settings }, status, failures: [...failures] }),
  bganalysis_save_settings: (a) => {
    const s = a.settings;
    if (!Number.isInteger(s.key_min_confidence_pct) || s.key_min_confidence_pct < 0 || s.key_min_confidence_pct > 100) {
      throw "The key confidence floor must be between 0 and 100 per cent.";
    }
    settings = { ...s };
    replan();
    return { ...settings };
  },
  bganalysis_watch: (a) => {
    const next = (a.root as string | null) ?? null;
    if (next !== root) {
      manualRoot = null;
      failures = [];
    }
    root = next;
    if (root) ensureTimer();
    replan();
    return null;
  },
  bganalysis_set_paused: (a) => {
    paused = !!a.paused;
    tick();
    return null;
  },
  bganalysis_set_playing: (a) => {
    playing = !!a.playing;
    tick();
    return null;
  },
  bganalysis_run_now: (a) => {
    if (!KINDS.some(wants)) throw "Background analysis is switched off in Settings (or every kind is off).";
    if (a.root !== root) throw "That library is not the open one.";
    manualRoot = root;
    paused = false;
    replan();
    return null;
  },
  bganalysis_retry_failures: () => {
    failures = [];
    publish();
    replan();
    return null;
  },
};
