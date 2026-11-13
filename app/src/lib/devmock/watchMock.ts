// Mock of the watch-folder commands (src-tauri/src/commands/import/watch.rs). Invented files only.
// Each watched folder "holds" the same few files; one is already on the device (a name the add
// mock skips), one was just written (left for the next scan), one matches a track by title, one
// fails to add, and any file whose name is now a track title (added through the add mock) is in
// the library. Settings, ignore list and log are kept in memory per page load.
// Dev URL switches: ?watchauto (add automatically), ?watchempty (no folder yet), ?watchfolders
// (two folders, one of them inside the library).
import { tracks } from "./db";
import type { Handlers } from "./types";

const flag = (name: string) => {
  try {
    return new URLSearchParams(location.search).has(name);
  } catch {
    return false;
  }
};

type Settings = {
  folders: string[];
  mode: "ask" | "auto";
  scan_on_open: boolean;
  interval_minutes: number;
  subfolders: boolean;
  min_age_secs: number;
  auto_max_files: number;
  open_confirmation: boolean;
  length_tolerance_secs: number;
};

let settings: Settings = {
  folders: flag("watchempty") ? [] : flag("watchfolders") ? ["C:\\Example Music\\Downloads", "P:\\Staging"] : ["C:\\Example Music\\Downloads"],
  mode: flag("watchauto") ? "auto" : "ask",
  scan_on_open: true,
  interval_minutes: 0,
  subfolders: true,
  min_age_secs: 10,
  auto_max_files: 100,
  open_confirmation: false,
  length_tolerance_secs: 2.5,
};

type Ignored = { path: string; size: number; at: number };
type Entry = { at: number; mode: string; requested: number; names: string[]; ok: number; failed: number; summary: string };
type Candidate = { path: string; name: string; size: number; modified_ms: number; folder: string };
type Scan = Record<string, any> & { root: string; candidates: Candidate[]; candidates_total: number; ignored: number };
const states = new Map<string, { ignored: Ignored[]; last_scan: Scan | null; log: Entry[] }>();
const stateOf = (root: string) => {
  let s = states.get(root);
  if (!s) states.set(root, (s = { ignored: [], last_scan: null, log: [] }));
  return s;
};
const view = (root: string) => ({ ...stateOf(root), problem: null });

const FILES: [string, number][] = [
  ["Example Tune One.mp3", 7_340_032],
  ["Example Tune Two.mp3", 9_100_000],
  ["Example Tune Three.flac", 31_457_280],
  ["Example Tune Two (copy).mp3", 9_100_001],
  ["Example Skip Me.flac", 25_000_000],
  ["Example Bad File.wav", 1_200_000],
  ["Example Fresh Download.mp3", 5_000_000],
];

const trim = (p: string) => p.replace(/[\\/]+$/, "");
const stem = (n: string) => n.replace(/\.[^.]+$/, "");
const inside = (p: string, root: string) => trim(p).toLowerCase().startsWith(trim(root).toLowerCase());

function scan(root: string): Scan {
  const st = stateOf(root);
  const ignore = new Set(st.ignored.map((i) => `${i.path.toLowerCase()}|${i.size}`));
  const now = Date.now();
  const out: Scan = {
    at: now,
    root,
    folders: [],
    candidates: [],
    candidates_total: 0,
    audio_files: 0,
    in_library: 0,
    matched_total: 0,
    matched: [],
    ignored: 0,
    too_new: 0,
    duplicates: 0,
    unusable_names: 0,
    cut_short: false,
  };
  const titles = new Set(tracks.map((t) => t.title.toLowerCase()));
  for (const folder of settings.folders) {
    if (inside(folder, root)) {
      out.folders.push({ path: folder, error: "This folder is inside the open library; the library's own files are not watched.", audio_files: 0 });
      continue;
    }
    out.folders.push({ path: folder, error: null, audio_files: FILES.length });
    FILES.forEach(([name, size], i) => {
      const path = `${trim(folder)}\\${name}`;
      out.audio_files++;
      if (ignore.has(`${path.toLowerCase()}|${size}`)) return void out.ignored++;
      if (/skip/i.test(name) || titles.has(stem(name).toLowerCase())) return void out.in_library++;
      if (/fresh/i.test(name) && settings.min_age_secs > 0) return void out.too_new++;
      if (/\(copy\)/i.test(name)) {
        out.matched_total++;
        out.matched.push({ path, name, track_id: tracks[2].track_id, how: "same title, artist and length" });
        return;
      }
      out.candidates.push({ path, name, size, modified_ms: now - (i + 1) * 3_600_000, folder });
    });
  }
  out.candidates_total = out.candidates.length;
  st.last_scan = out;
  return out;
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const handlers: Handlers = {
  watch_settings_get: () => ({ settings: { ...settings, folders: [...settings.folders] }, problem: null }),
  watch_settings_set: (a) => {
    const s = a.settings as Settings;
    for (const f of s.folders) if (!/^([A-Za-z]:[\\/]|\\\\|\/)/.test(f.trim())) throw `"${f}" is not a full folder path.`;
    const seen = new Set<string>();
    settings = {
      ...s,
      folders: s.folders.map((f) => f.trim()).filter((f) => f && !seen.has(f.toLowerCase()) && seen.add(f.toLowerCase())),
      interval_minutes: Math.min(s.interval_minutes, 1440),
      min_age_secs: Math.min(s.min_age_secs, 3600),
      auto_max_files: Math.min(Math.max(s.auto_max_files, 1), 5000),
      length_tolerance_secs: Number.isFinite(s.length_tolerance_secs) ? Math.min(Math.max(s.length_tolerance_secs, 0), 30) : 2.5,
    };
    return settings;
  },
  watch_scan: async (a) => {
    await wait(350);
    return scan(String(a.root));
  },
  watch_state: (a) => view(String(a.root)),
  watch_ignore: (a) => {
    const root = String(a.root);
    const st = stateOf(root);
    for (const f of a.files as { path: string; size: number }[]) {
      if (!st.ignored.some((i) => i.path.toLowerCase() === f.path.toLowerCase() && i.size === f.size)) st.ignored.push({ path: f.path, size: f.size, at: Date.now() });
    }
    if (st.last_scan) {
      const gone = new Set((a.files as { path: string; size: number }[]).map((f) => `${f.path.toLowerCase()}|${f.size}`));
      const before = st.last_scan.candidates.length;
      st.last_scan.candidates = st.last_scan.candidates.filter((c) => !gone.has(`${c.path.toLowerCase()}|${c.size}`));
      const removed = before - st.last_scan.candidates.length;
      st.last_scan.candidates_total -= removed;
      st.last_scan.ignored += removed;
    }
    return view(root);
  },
  watch_unignore: (a) => {
    const st = stateOf(String(a.root));
    const gone = new Set((a.paths as string[]).map((p) => p.toLowerCase()));
    st.ignored = st.ignored.filter((i) => !gone.has(i.path.toLowerCase()));
    return view(String(a.root));
  },
  watch_log_add: (a) => {
    const st = stateOf(String(a.root));
    st.log.push({ at: Date.now(), ...(a.entry as Omit<Entry, "at">) });
    st.log = st.log.slice(-100);
    return view(String(a.root));
  },
};
