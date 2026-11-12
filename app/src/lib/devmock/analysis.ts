// Mock of src-tauri/src/commands/analysis.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
import { emit } from "@tauri-apps/api/event";
import { canonicalKeyName } from "../state/keyLogic";
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
  // like the real quick write: every entry is checked first, then all are written or none; a
  // Camelot code or free text is refused, flats are stored as the detector spells them (sharps)
  analysis_set_keys: (a) => {
    const entries = a.entries as { track_id: number; key: string }[];
    if (entries.length === 0) throw "No tracks to change.";
    const wanted = entries.map((e) => {
      const name = canonicalKeyName(e.key);
      if (!name) throw `"${String(e.key).trim()}" is not a musical key (for example Am, F#m, Bb or C).`;
      const t = tracks.find((x) => x.track_id === e.track_id);
      if (!t) throw `invalid: no track ${e.track_id}`;
      return { t, name };
    });
    let changed = 0;
    for (const { t, name } of wanted) {
      if (t.key !== name) {
        t.key = name;
        changed++;
      }
    }
    return changed;
  },

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

  // like the real job: only keyless tracks (unless overwrite, which needs named tracks), one
  // progress event each, then done. Every 4th target is skipped as too short and the 6th fails,
  // and results under minConfidence are skipped, so the UI's skipped and failed states show up.
  analysis_detect_keys: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const ids = a.trackIds as number[] | null;
    if (ids && ids.length === 0) throw "No tracks to analyse.";
    if (a.overwrite && !ids) throw "Overwriting existing keys needs the tracks to be named.";
    const floor = Number(a.minConfidence) || 0;
    const targets = tracks.filter((t) => (!ids || ids.includes(t.track_id)) && (a.overwrite || !t.key.trim()));
    jobLock.running = true;
    let ok = 0;
    let failed = 0;
    let skipped = 0;
    const finish = async () => {
      jobLock.running = false;
      const parts = [
        ok ? `${ok} key${ok === 1 ? "" : "s"} written` : "",
        skipped ? `${skipped} skipped` : "",
        failed ? `${failed} failed` : "",
      ].filter(Boolean);
      await emit("job-done", { job: "keys", ok, failed, summary: parts.join(", ") || "nothing to do", error: null });
    };
    if (!targets.length) setTimeout(finish, 300);
    targets.forEach((t, i) =>
      setTimeout(async () => {
        const r = seeded(t.location);
        const key = KEYS[Math.floor(r() * 12)] + (r() < 0.5 ? "m" : "");
        const confidence = (60 + Math.floor(r() * 35)) / 100;
        let status = "detected";
        let detail: string | null = `${key} (${Math.round(confidence * 100)}%)`;
        let error: string | null = null;
        if (i % 6 === 5) {
          status = "failed";
          detail = null;
          error = "could not decode the file";
          failed++;
        } else if (i % 4 === 3) {
          status = "skipped";
          detail = "no clear key (too short, silent or atonal)";
          skipped++;
        } else if (confidence < floor) {
          status = "skipped";
          detail = `low confidence: ${key} (${Math.round(confidence * 100)}%)`;
          skipped++;
        } else {
          t.key = key;
          ok++;
        }
        await emit("job-progress", {
          job: "keys",
          label: `${t.artist} - ${t.title}`,
          done: i + 1,
          total: targets.length,
          status,
          bpm: null,
          detail,
          error,
        });
        if (i === targets.length - 1) await finish();
      }, 500 * (i + 1)),
    );
    return null;
  },
};
