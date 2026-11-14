// Mock of the bulk case export (src-tauri/src/commands/cases/export_bulk.rs and export_prefs.rs).
// The browser cannot write files, so a "disk" is a Map of the names the mock has written: a second
// export into the same folder meets them and shows the conflict choices. Invented names only.
import { emit } from "@tauri-apps/api/event";
import type { ExportAction, ExportAudioEntry, ExportListEntry, ExportPlan, ExportPrefs, ExportRequest } from "../api/cases";
import { cases, tracks } from "./db";
import { smartForExport } from "./cases";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

let prefs: ExportPrefs = { folder: "C:\\Exports", format: "m3u8", relative_paths: false, copy_audio: false, audio_layout: "shared", audio_names: "tags", conflict: "skip", remember: true };
let cancelled = false;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

// what is "in the folder": lower-case path -> size (playlists are 1)
const disk = new Map<string, number>([["c:\\exports\\warm-up.m3u8", 1]]);
const EXT: Record<string, string> = { m3u8: "m3u8", csv: "csv", rekordbox: "xml" };
const byId = new Map(tracks.map((t) => [t.track_id, t]));

const safe = (s: string, fallback: string) => {
  const v = s.replace(/[\\/:*?"<>|]/g, "_").replace(/^[ .]+|[ .]+$/g, "");
  return (v || fallback).slice(0, 100);
};
const join = (a: string, ...rest: string[]) => [a.replace(/[\\/]+$/, ""), ...rest].join("\\");
const key = (p: string) => p.toLowerCase();

function unique(stem: string, ext: string, used: Set<string>, taken: (n: string) => boolean = () => false) {
  for (let n = 1; ; n++) {
    const c = n === 1 ? `${stem}${ext}` : `${stem} (${n})${ext}`;
    if (!used.has(c.toLowerCase()) && !taken(c)) {
      used.add(c.toLowerCase());
      return c;
    }
  }
}

function settle(path: string, size: number | null, policy: ExportRequest["conflict"], used: Set<string>): { path: string; action: ExportAction; wanted: string | null } {
  const have = disk.get(key(path));
  if (have === undefined) return { path, action: "write", wanted: null };
  if (size !== null && have === size) return { path, action: "same", wanted: null };
  if (policy === "skip") return { path, action: "skip", wanted: null };
  if (policy === "replace") return { path, action: "replace", wanted: null };
  const dir = path.slice(0, path.lastIndexOf("\\"));
  const file = path.slice(path.lastIndexOf("\\") + 1);
  const dot = file.lastIndexOf(".");
  const name = unique(dot > 0 ? file.slice(0, dot) : file, dot > 0 ? file.slice(dot) : "", used, (c) => disk.has(key(join(dir, c))));
  return { path: join(dir, name), action: "renamed", wanted: file };
}

/** A small stand-in for the backend's plan fingerprint (FNV-1a over the plan's text). */
function fingerprint(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return h.toString(16).padStart(8, "0");
}

function plan(req: ExportRequest): ExportPlan {
  if (!req.folder.trim()) throw "Choose a folder to export to.";
  if (/^p:/i.test(req.folder)) throw "Choose a folder outside the library: files written there are not part of it.";
  const ext = EXT[req.format];
  if (!ext) throw `unknown export format ${req.format}`;
  if (req.case_ids.length === 0 && req.smart_ids.length === 0) throw "Tick at least one case to export.";
  const notes: string[] = [];
  type Src = { name: string; smart: boolean; ids: number[] };
  const smart: Src[] = [];
  const copies = new Map<number, string>();
  for (const id of new Set(req.smart_ids)) {
    try {
      const s = smartForExport(id);
      if (!s) notes.push(`Smart case ${id} no longer exists, so it was left out.`);
      else {
        smart.push({ name: s.name, smart: true, ids: s.ids });
        if (s.copy != null) copies.set(s.copy, s.name);
      }
    } catch (e) {
      notes.push(`Smart case ${id} was left out: ${e}`);
    }
  }
  const srcs: Src[] = [];
  for (const id of new Set(req.case_ids)) {
    const c = cases.find((x) => x.id === id);
    if (!c) notes.push(`Case ${id} no longer exists, so it was left out.`);
    else if (copies.has(id)) notes.push(`“${c.name}” is the Pacemaker copy of the smart case “${copies.get(id)}”, which is exported as itself, so it was not written a second time.`);
    else srcs.push({ name: c.name, smart: false, ids: c.ids.filter((i) => byId.has(i)) });
  }
  srcs.push(...smart);

  const root = req.folder.replace(/[\\/]+$/, "");
  const perCase = req.copy_audio && req.audio_layout === "per_case";
  const stems = new Set<string>();
  const used = new Map<string, Set<string>>();
  const lists: (ExportListEntry & { ids: number[]; dir: string })[] = [];
  for (const s of srcs) {
    if (s.ids.length === 0) {
      notes.push(`“${s.name}” has no tracks to export, so no file was written for it.`);
      continue;
    }
    const base = safe(s.name, "case");
    const first = s.smart && stems.has(base.toLowerCase()) ? `${base} (smart)` : base;
    const stem = unique(first, "", stems);
    const dir = perCase ? join(root, stem) : root;
    const set = used.get(dir) ?? new Set<string>();
    used.set(dir, set);
    set.add(`${stem}.${ext}`.toLowerCase());
    const r = settle(join(dir, `${stem}.${ext}`), null, req.conflict, set);
    lists.push({ name: s.name, smart: s.smart, file: r.path.slice(root.length + 1).replace(/\\/g, "/"), path: r.path, tracks: s.ids.length, action: r.action, wanted: r.wanted, ids: s.ids, dir });
  }
  if (lists.length === 0) throw `Nothing to export: none of the chosen cases has tracks. ${notes.join(" ")}`.trim();

  const audio: ExportAudioEntry[] = [];
  if (req.copy_audio) {
    const index = new Map<string, number>();
    const dirUsed = new Map<string, Set<string>>();
    lists.forEach((l, li) => {
      const dir = perCase ? l.dir : join(root, "Audio");
      for (const id of l.ids) {
        const t = byId.get(id)!;
        const k = `${perCase ? li : 0}|${t.location}`;
        const have = index.get(k);
        if (have !== undefined) {
          audio[have].used_by++;
          continue;
        }
        const file = t.location.split("/").pop() ?? "audio";
        const dot = file.lastIndexOf(".");
        const stem = req.audio_names === "device" ? file.slice(0, dot) : `${t.artist} - ${t.title}`;
        const set = dirUsed.get(dir) ?? new Set<string>();
        dirUsed.set(dir, set);
        const name = unique(safe(stem, "track"), file.slice(dot), set);
        const r = settle(join(dir, name), t.file_size, req.conflict, set);
        audio.push({ title: t.title, file: r.path.slice(root.length + 1).replace(/\\/g, "/"), path: r.path, size: t.file_size, action: r.action, wanted: r.wanted, used_by: 1, problem: null });
        index.set(k, audio.length - 1);
      }
    });
  }
  const out = {
    folder: req.folder,
    lists: lists.map(({ ids: _i, dir: _d, ...l }) => l),
    audio,
    notes,
    copy_bytes: audio.filter((a) => ["write", "replace", "renamed"].includes(a.action)).reduce((n, a) => n + a.size, 0),
  };
  return { ...out, now_ms: 1_700_000_000_000, digest: fingerprint(JSON.stringify({ out, ids: lists.map((l) => l.ids) })) };
}

const writes = (a: ExportAction) => a === "write" || a === "replace" || a === "renamed";

export const handlers: Handlers = {
  cases_export_prefs_get: () => ({ prefs: { ...prefs }, warning: null }),
  cases_export_prefs_set: (a) => {
    prefs = { ...(a.prefs as ExportPrefs) };
    return { prefs: { ...prefs }, warning: null };
  },
  cases_export_plan: (a) => plan(a.request as ExportRequest),
  cases_export_start: (a) => {
    const p = plan(a.request as ExportRequest);
    const c = a.confirmed as { digest: string } | undefined;
    if (c && c.digest !== p.digest) throw "The cases, the library or the folder changed after the preview was made, so nothing was written. The preview has been refreshed: check it and press Export again.";
    if (jobLock.running) throw "Another job is still running.";
    const files = [...p.audio.filter((x) => writes(x.action)).map((x) => ({ path: x.path, size: x.size, file: x.file })), ...p.lists.filter((x) => writes(x.action)).map((x) => ({ path: x.path, size: 1, file: x.file }))];
    jobLock.running = true;
    cancelled = false;
    void (async () => {
      let n = 0;
      for (const f of files) {
        await wait(350);
        if (cancelled) {
          jobLock.running = false;
          await emit("job-done", { job: "cases-export", ok: n, failed: 0, summary: `Cancelled. Files already written stay in the folder: ${n} of ${files.length} written (${p.folder}).`, error: null });
          return;
        }
        disk.set(key(f.path), f.size);
        n++;
        await emit("job-progress", { job: "cases-export", label: f.file, done: n, total: files.length, status: "written", bpm: null, detail: null, error: null });
      }
      jobLock.running = false;
      const lists = p.lists.filter((x) => writes(x.action)).length;
      const audio = files.length - lists;
      const kept = p.lists.filter((x) => x.action === "skip").length + p.audio.filter((x) => x.action === "skip").length;
      const parts = [`${lists} playlist${lists === 1 ? "" : "s"} written`, audio ? `${audio} audio file${audio === 1 ? "" : "s"} copied` : "", kept ? `${kept} existing file${kept === 1 ? "" : "s"} left alone` : ""].filter(Boolean);
      await emit("job-done", { job: "cases-export", ok: files.length, failed: 0, summary: `Exported: ${parts.join(", ")} (${p.folder}).`, error: null });
    })();
    return { folder: p.folder, writes: files.length };
  },
  cases_export_cancel: () => {
    cancelled = true;
  },
};
