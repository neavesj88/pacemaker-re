// Mock of src-tauri/src/commands/export.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
//
// The browser cannot run the Rust mixer, so a preview is a small synthetic WAV (a kick on every
// beat of the mix tempo, in a Blob) and an export is a timed fake job that reports progress and
// finishes. Nothing is written anywhere.
import { emit } from "@tauri-apps/api/event";
import type { MixDoc } from "$lib/api/mixes";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

const RATE = 22050;

/** A mono 16-bit WAV with a decaying 60 Hz thump on every beat. */
function clickWav(secs: number, bpm: number): Blob {
  const n = Math.round(secs * RATE);
  const buf = new ArrayBuffer(44 + n * 2);
  const v = new DataView(buf);
  const text = (o: number, s: string) => [...s].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)));
  text(0, "RIFF");
  v.setUint32(4, 36 + n * 2, true);
  text(8, "WAVEfmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, 1, true);
  v.setUint32(24, RATE, true);
  v.setUint32(28, RATE * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  text(36, "data");
  v.setUint32(40, n * 2, true);
  const beat = (60 / bpm) * RATE;
  for (let i = 0; i < n; i++) {
    const t = (i % beat) / RATE;
    const s = Math.sin(2 * Math.PI * 60 * t) * Math.exp(-t * 14) * 0.6;
    v.setInt16(44 + i * 2, Math.round(s * 32767), true);
  }
  return new Blob([buf], { type: "audio/wav" });
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const handlers: Handlers = {
  export_render_preview: (a) => {
    const doc = a.mix as MixDoc;
    const from = Number(a.fromSec);
    const to = Number(a.toSec);
    if (!Number.isFinite(from) || !Number.isFinite(to) || from < 0 || to <= from) throw "The preview range is empty.";
    if (to - from > 900) throw "A preview can be at most 15 minutes; export the mix to hear more.";
    if (!doc.clips.length) throw "the mix has no audible clips";
    const secs = Math.min(to - from, 20);
    return {
      path: URL.createObjectURL(clickWav(secs, doc.tempoBpm ?? 128)),
      durationSecs: to - from,
      warnings: doc.clips.some((c) => c.trackId < 0) ? ["clip skipped: the track was removed from the library"] : [],
    };
  },
  export_mix_audio: (a) => {
    const doc = a.mix as MixDoc;
    if (a.format !== "ogg" && a.format !== "wav") throw `Unknown audio format '${a.format}' (use ogg or wav).`;
    if (jobLock.running) throw "Another job is still running.";
    if (!doc.clips.length) throw "the mix has no audible clips";
    const total = Math.max(1, Math.floor(doc.lengthSec));
    const skipped = doc.clips.filter((c) => c.trackId < 0).length;
    jobLock.running = true;
    void (async () => {
      const steps = 6;
      for (let i = 1; i <= steps; i++) {
        await wait(450);
        await emit("job-progress", {
          job: "mix-export",
          label: doc.name,
          done: Math.round((total * i) / steps),
          total,
          status: "rendering",
          bpm: null,
          detail: null,
          error: null,
        });
      }
      jobLock.running = false;
      const m = Math.floor(total / 60);
      const s = String(total % 60).padStart(2, "0");
      await emit("job-done", {
        job: "mix-export",
        ok: 1,
        failed: skipped,
        summary: `Exported ${doc.name} (${m}:${s}) to ${String(a.dest).split(/[\\/]/).pop()}`,
        error: null,
      });
    })();
    return { path: String(a.dest) };
  },
};
