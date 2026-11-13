// Mock of src-tauri/src/commands/export/vdj.rs (the VirtualDJ list export). The plan is computed
// from the shared mock library with the same rules as the Rust side (names, numbering, refusals);
// the "export" is a timer-driven job that only reports progress, nothing is written anywhere.
import { emit } from "@tauri-apps/api/event";
import type { PlanEntry } from "../api/vdj";
import { cases, tracks } from "./db";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

const BAD = /[<>:"/\\|?*\x00-\x1f]/g;
const safe = (s: string, fallback: string) =>
  s.replace(BAD, "_").replace(/\s+/g, " ").trim().replace(/[. ]+$/, "").slice(0, 120) || fallback;

type Req = {
  root: string;
  case_id: number;
  only_track_ids: number[];
  list_name: string;
  dest_dir: string;
  options: { copy_audio: boolean; name_files: string; ordered: boolean; no_duplicates: boolean; path_base: string };
};

let cancelled = false;
/** A list file that "already exists", so the replace question can be seen in the preview. */
const EXISTING = "E:\\Gigs\\Existing set\\Existing set.xml";

function plan(r: Req) {
  const name = safe(r.list_name ?? "", "");
  if (!name) throw "Give the list a name.";
  if (!/^([A-Za-z]:[\\/]|\/)/.test(r.dest_dir ?? "")) throw "Choose a destination folder.";
  // the mock device is drive P:
  if (/^P:/i.test(r.dest_dir)) throw "Choose a folder outside the Pacemaker: this export is not part of its library.";
  const caseIds = cases.find((c) => c.id === r.case_id)?.ids ?? [];
  const keep = new Set(r.only_track_ids ?? []);
  const asked = keep.size ? caseIds.filter((id) => keep.has(id)) : caseIds;
  const ids = [...new Set(asked)];
  if (!ids.length) throw "There are no tracks to export.";
  const dest = r.dest_dir.replace(/[\\/]+$/, "");
  const folder = `${dest}\\${name}`;
  const base = (r.options.path_base ?? "").trim() || folder;
  if (r.options.copy_audio && !/^([A-Za-z]:[\\/]|\\\\|\/)/.test(base)) {
    throw "The folder VirtualDJ will find the audio in: Write a full path such as D:\\Gig or /Volumes/USB/Gig.";
  }
  const used = new Set<string>();
  const entries = ids.map((id, i) => {
    const t = tracks.find((x) => x.track_id === id);
    const out = (o: Pick<PlanEntry, "label" | "status"> & Partial<PlanEntry>): PlanEntry => ({ track_id: id, file: "", path: "", size: 0, reason: "", ...o });
    if (!t) return out({ label: `track #${id}`, status: "left_out", reason: "No longer in the library." });
    const label = `${t.artist} - ${t.title}`;
    // one track of a longer case is "missing" so the left-out wording can be seen
    if (i === 3 && ids.length > 4) return out({ label, status: "left_out", reason: "The audio file is missing from the Pacemaker." });
    const size = 4_000_000 + id * 137_000;
    const original = t.location.split("/").pop() ?? "track.mp3";
    if (!r.options.copy_audio) {
      return out({ label, status: "reference", file: original, size, path: `P:\\.Pacemaker\\Music\\${t.location.replace("/pmdb_tracks/", "").replace(/\//g, "\\")}` });
    }
    const ext = original.split(".").pop() ?? "mp3";
    const stem = r.options.name_files === "original" ? safe(original.replace(/\.[^.]*$/, ""), "track") : safe(label, "track");
    let n = 1;
    let file = `${stem}.${ext}`;
    while (used.has(file.toLowerCase())) file = `${stem} (${++n}).${ext}`;
    used.add(file.toLowerCase());
    return out({ label, status: i === 1 ? "reuse" : "copy", file, size, path: `${base.replace(/[\\/]+$/, "")}\\Audio\\${file}` });
  });
  const count = (s: string) => entries.filter((e) => e.status === s).length;
  const listFile = `${folder}\\${name}.xml`;
  return {
    list_name: name,
    folder,
    list_file: listFile,
    list_exists: listFile === EXISTING,
    copy_audio: r.options.copy_audio,
    entries,
    copy_count: count("copy"),
    reuse_count: count("reuse"),
    reference_count: count("reference"),
    left_out_count: count("left_out"),
    copy_bytes: entries.filter((e) => e.status === "copy").reduce((s, e) => s + e.size, 0),
    notes: ids.length < caseIds.length && !keep.size ? [`${caseIds.length - ids.length} repeated entries in the case are listed once.`] : [],
  };
}

export const handlers: Handlers = {
  vdj_export_plan: (a) => plan(a.request as Req),
  vdj_export_start: (a) => {
    const p = plan(a.request as Req);
    if (p.list_exists && !a.replaceList) throw `${p.list_file} already exists. Choose to replace it, or pick another list name.`;
    if (jobLock.running) throw "Another job is still running.";
    cancelled = false;
    jobLock.running = true;
    const todo = p.entries.filter((e) => e.status !== "left_out");
    const finish = (summary: string) => {
      jobLock.running = false;
      void emit("job-done", { job: "virtualdj-export", ok: todo.length, failed: p.left_out_count, summary, error: null });
    };
    todo.forEach((e, i) =>
      setTimeout(() => {
        if (!jobLock.running) return;
        if (cancelled) {
          finish(`The export of ${p.list_name} was stopped. No list was written; ${i} files already copied stay in ${p.folder}.`);
          return;
        }
        void emit("job-progress", { job: "virtualdj-export", label: e.label, done: i + 1, total: todo.length, status: e.status === "copy" ? "copied" : "listed", bpm: null, detail: null, error: null });
        if (i === todo.length - 1) finish(`Wrote ${p.list_file} with ${todo.length} tracks; ${p.copy_count} files copied, ${p.reuse_count} reused`);
      }, 400 * (i + 1)),
    );
    return { folder: p.folder, list_file: p.list_file };
  },
  vdj_export_cancel: () => {
    cancelled = true;
  },
};
