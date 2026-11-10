// Mock of src-tauri/src/commands/analysis.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
import { emit } from "@tauri-apps/api/event";
import { tracks } from "./db";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

function seeded(text: string) {
  let s = 0;
  for (const c of text) s = (s * 31 + c.charCodeAt(0)) >>> 0;
  return () => ((s = (s * 1103515245 + 12345) >>> 0) / 2 ** 32);
}

const KEYS = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

export const handlers: Handlers = {
  // synthetic but stable per track: a kick-heavy body with quieter intro and outro
  analysis_peaks: (a) => {
    const r = seeded(String(a.location));
    const width = Math.max(1, Math.min(16384, Number(a.width) || 100));
    return Array.from({ length: width }, (_, i) => {
      const t = i / width;
      const env = t < 0.08 || t > 0.92 ? 0.35 : 0.8 + 0.2 * Math.sin(t * 9);
      return [Math.round(255 * env * (0.55 + 0.45 * r())), Math.round(170 * env * r()), Math.round(110 * env * r())];
    });
  },

  // like the real job: only keyless tracks (unless overwrite), one progress event each, then done
  analysis_detect_keys: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const ids = a.trackIds as number[] | null;
    const targets = tracks.filter((t) => (!ids || ids.includes(t.track_id)) && (a.overwrite || !t.key.trim()));
    if (ids && ids.length === 0) throw "No tracks to analyse.";
    jobLock.running = true;
    const finish = async () => {
      jobLock.running = false;
      await emit("job-done", {
        job: "keys",
        ok: targets.length,
        failed: 0,
        summary: targets.length ? `${targets.length} key${targets.length === 1 ? "" : "s"} written` : "nothing to do",
        error: null,
      });
    };
    if (!targets.length) setTimeout(finish, 300);
    targets.forEach((t, i) =>
      setTimeout(async () => {
        const r = seeded(t.location);
        const key = KEYS[Math.floor(r() * 12)] + (r() < 0.5 ? "m" : "");
        t.key = key;
        await emit("job-progress", {
          job: "keys",
          label: `${t.artist} - ${t.title}`,
          done: i + 1,
          total: targets.length,
          status: "detected",
          bpm: null,
          detail: `${key} (${60 + Math.floor(r() * 35)}%)`,
          error: null,
        });
        if (i === targets.length - 1) await finish();
      }, 500 * (i + 1)),
    );
    return null;
  },
};
