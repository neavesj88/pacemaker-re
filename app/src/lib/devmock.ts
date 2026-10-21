// Browser preview: when the UI runs outside Tauri (plain `npm run dev` in a browser), answer
// the app's commands with a synthetic library so the interface can be developed and checked
// without the native shell. Never loaded inside the Tauri app.
//
// Mirrors the commands in src-tauri/src/lib.rs, including the add / re-analyse jobs, which
// emit the same `job-progress` / `job-done` events on timers. Nothing here touches a disk.
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

const genres = ["House", "Techno", "Drum & Bass", "Psy-Trance", "Trance"];
const tracks = Array.from({ length: 24 }, (_, i) => ({
  track_id: i + 1,
  title: `Demo Track ${String(i + 1).padStart(2, "0")}`,
  artist: `Example Artist ${String.fromCharCode(65 + (i % 8))}`,
  album: `Sample Album ${1 + (i % 5)}`,
  genre: genres[i % genres.length],
  bpm: [124, 126, 128, 87, 138, 140][i % 6] + (i % 3 === 0 ? 0 : 0.5),
  key: ["Am", "Cm", "F#m", "Ab Minor", ""][i % 5],
  play_time_secs: 180 + ((i * 37) % 240),
  rating: i % 6,
  location: `/pmdb_tracks/${i % 10}/${(i * 3) % 16}/demo${i}.mp3`,
  file_size: 8_000_000 + i * 12345,
  format: "MP3",
  analysed: i % 7 !== 3,
  times_played: i % 4,
}));

// tracks whose .str is bad, with the reasons the real backend reports
const needs = new Map<number, string>([
  [4, "never analysed"],
  [11, "empty .str file (0 bytes)"],
  [18, "no .str file"],
]);
for (const t of tracks) t.analysed = !needs.has(t.track_id) || needs.get(t.track_id) !== "never analysed";

const cases = [
  { id: 1, name: "Warm-up", play_time_secs: 3600, track_count: 6, ids: [1, 2, 3, 5, 8, 13] },
  { id: 2, name: "Peak time", play_time_secs: 2400, track_count: 4, ids: [21, 22, 4, 999] },
  { id: 3, name: "Empty case", play_time_secs: 0, track_count: 0, ids: [] as number[] },
];

const AUDIO = /\.(mp3|flac|wav|m4a|aac|ogg|aiff?)$/i;
const baseName = (p: string) => p.split(/[\\/]/).pop() || p;
let running = false;

// emit one job-progress per item on a timer, then job-done
function runJob(job: "add" | "reanalyse", items: string[], each: (label: string, i: number) => { bpm?: number; error?: string }) {
  running = true;
  const sum = { added: 0, skipped: 0, reanalysed: 0, failed: 0 };
  items.forEach((label, i) =>
    setTimeout(async () => {
      const r = each(label, i);
      const status = r.error ? "failed" : job === "add" ? (/skip/i.test(label) ? "skipped" : "added") : "reanalysed";
      if (status === "failed") sum.failed++;
      else if (status === "skipped") sum.skipped++;
      else if (status === "added") sum.added++;
      else sum.reanalysed++;
      await emit("job-progress", {
        job,
        label,
        done: i + 1,
        total: items.length,
        status,
        bpm: status === "skipped" || r.error ? null : (r.bpm ?? 128),
        error: r.error ?? null,
      });
      if (i === items.length - 1) {
        running = false;
        await emit("job-done", { job, ...sum, error: null });
      }
    }, 700 * (i + 1)),
  );
}

export function installDevMock() {
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, any>;
      if (cmd === "list_volumes") return ["P:\\"];
      if (cmd === "open_library") {
        const root = a.root ?? "P:\\";
        return {
          root,
          problems: root.includes("broken") ? ["user_version is 12, device requires 677"] : [],
          tracks,
          mixes: [{ id: 1, name: "Example Mix", play_time_secs: 600, track_count: 3 }],
          cases: cases.map(({ ids: _ids, ...c }) => c),
          needs_analysis: [...needs].map(([track_id, reason]) => ({ track_id, reason })),
        };
      }
      if (cmd === "case_tracks") return cases.find((c) => c.id === a.caseId)?.ids ?? [];
      if (cmd === "expand_audio_paths") return (a.paths as string[]).filter((p) => AUDIO.test(p) || !/\.[a-z0-9]+$/i.test(p));
      if (cmd === "add_tracks") {
        if (running) throw "Another add or analysis job is still running.";
        const files = (a.files as string[]).map(baseName);
        runJob("add", files, (label, i) => {
          if (/bad/i.test(label)) return { error: "Decode(Unsupported(\"no suitable format reader found\"))" };
          if (!/skip/i.test(label)) {
            const id = tracks.length + 1;
            tracks.push({
              ...tracks[i % 6],
              track_id: id,
              title: label.replace(/\.[^.]+$/, ""),
              location: `/pmdb_tracks/0/0/new${id}.mp3`,
              analysed: true,
            });
          }
          return { bpm: 120 + i * 3.5 };
        });
        return null;
      }
      if (cmd === "reanalyse_tracks") {
        if (running) throw "Another add or analysis job is still running.";
        const ids = a.trackIds as number[];
        runJob("reanalyse", ids.map((id) => tracks.find((t) => t.track_id === id)?.title ?? String(id)), (_l, i) => {
          needs.delete(ids[i]);
          const t = tracks.find((x) => x.track_id === ids[i]);
          if (t) t.analysed = true;
          return { bpm: 126 };
        });
        return null;
      }
      if (cmd === "plugin:dialog|open") {
        const o = a.options ?? {};
        if (o.multiple) return ["C:\\Music\\New Artist - New Tune.mp3", "C:\\Music\\Another - Skip Me.flac", "C:\\Music\\bad file.wav"];
        return "P:\\";
      }
      throw new Error(`devmock: unhandled command ${cmd}`);
    },
    { shouldMockEvents: true },
  );
}
