// Mock of src-tauri/src/commands/stems.rs (browser preview, no Tauri). Nothing is downloaded or
// separated: a download is a timed fake that fills a progress bar, and a run emits the same
// events as the real job (stems-progress per part, stems-result, job-done) on a timer. The plan
// follows the real rules (plan.rs): names from "artist - title" plus the suffix, existing files
// kept or replaced, a folder on the device refused, over-long tracks refused. Files "written" by
// a mock run are remembered for the session so a second plan shows them as already done.
import { emit } from "@tauri-apps/api/event";
import type { Plan, PlanAction, PlanItem, StemSettings } from "$lib/api/stems";
import { tracks } from "./db";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

const DEVICE_ROOT = "P:\\";
const DATA_DIR = "C:\\Users\\demo\\AppData\\Local\\pacemaker-revival\\stems";

type Part = {
  id: string;
  kind: "model" | "runtime";
  name: string;
  summary: string;
  url: string;
  page_url: string;
  sha256: string;
  size_bytes: number;
  file: string;
  licence_name: string;
  licence_status: "clear" | "unclear";
  licence_text: string;
  notice_version: number;
  available_here: boolean;
};

const parts: Part[] = [
  {
    id: "htdemucs-ort",
    kind: "model",
    name: "HTDemucs 4-stem model (ONNX Runtime format)",
    summary: "Vocals and instrumental from one model. About 210 MB.",
    url: "https://models.example.invalid/htdemucs.ort",
    page_url: "https://models.example.invalid/htdemucs",
    sha256: "0".repeat(64),
    size_bytes: 209_884_896,
    file: "htdemucs.ort",
    licence_name: "Weights licence not clear (code: MIT)",
    licence_status: "unclear",
    licence_text:
      "Example notice for the preview. The real notice says the weights' licence is UNCLEAR, that this project does not host or bundle the file, and that the app checks its SHA-256 before using it.",
    notice_version: 1,
    available_here: true,
  },
  {
    id: "onnxruntime-win-x64",
    kind: "runtime",
    name: "ONNX Runtime 1.31.1 (Windows x64, CPU)",
    summary: "The engine that runs the model. About 83 MB to download.",
    url: "https://engine.example.invalid/onnxruntime.zip",
    page_url: "https://engine.example.invalid/onnxruntime",
    sha256: "f".repeat(64),
    size_bytes: 82_929_944,
    file: "onnxruntime.zip",
    licence_name: "MIT",
    licence_status: "clear",
    licence_text: "Example notice for the preview: MIT licence, CPU build.",
    notice_version: 1,
    available_here: true,
  },
];

const defaults = (): StemSettings => ({
  device: "auto",
  threads: 0,
  format: "mp3",
  mp3_kbps: 320,
  mp3_align: true,
  ogg_quality: 0.6,
  overlap_percent: 50,
  instrumental: "sum",
  max_minutes: 30,
  vocals_suffix: "(Acapella)",
  instrumental_suffix: "(Instrumental)",
  existing: "skip",
  runtime_path: "",
  last_folder: "",
});

let settings = defaults();
const installed = new Set<string>();
const consents: Record<string, { notice_version: number; accepted_at: number }> = {};
/** Result files "written" this session, lowercase full paths. */
const written = new Set<string>();
let cancelled = false;

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

function status() {
  const model = installed.has("htdemucs-ort");
  const engine = installed.has("onnxruntime-win-x64") || !!settings.runtime_path;
  const blocked = !model ? "The separation model is not downloaded yet." : engine ? null : "ONNX Runtime is not downloaded yet.";
  return {
    components: parts.map((p) => ({
      ...p,
      installed: installed.has(p.id),
      installed_bytes: installed.has(p.id) ? p.size_bytes : 0,
      consent: consents[p.id] ?? null,
      consent_current: consents[p.id]?.notice_version === p.notice_version,
    })),
    runtime: settings.runtime_path
      ? { source: "custom", path: settings.runtime_path, problem: null }
      : installed.has("onnxruntime-win-x64")
        ? { source: "downloaded", path: `${DATA_DIR}\\runtime\\onnxruntime-win-x64\\onnxruntime.dll`, problem: null }
        : { source: null, path: null, problem: "ONNX Runtime is not downloaded yet." },
    ready: blocked === null,
    blocked,
    settings: { ...settings, consents },
    settings_warning: null,
    busy: jobLock.running,
    data_dir: DATA_DIR,
  };
}

/** plan.rs `safe_name`, simplified. */
function safeName(s: string): string {
  return s.replace(/[\u0000-\u001f\\/:*?"<>|]/g, "_").replace(/\s+/g, " ").trim().slice(0, 100).replace(/[. ]+$/, "");
}

const fileOf = (dest: string, base: string, suffix: string) => `${dest.replace(/[\\/]+$/, "")}\\${base} ${suffix}.${settings.format}`;

function formatLabel(): string {
  return settings.format === "ogg" ? `Ogg Vorbis, quality ${settings.ogg_quality.toFixed(1)}` : settings.format === "wav" ? "WAV, 16-bit" : `MP3, ${settings.mp3_kbps} kbps`;
}

function segmentsFor(secs: number): number {
  const window = 343_980;
  const hop = Math.max(1, Math.round(window * (1 - settings.overlap_percent / 100)));
  const frames = secs * 44_100;
  return frames <= window ? 1 : 1 + Math.ceil((frames - window) / hop);
}

function buildPlan(ids: number[], dest: string): Plan {
  if (!ids.length) throw "Select at least one track.";
  if (!dest.trim() || /[\u0000]/.test(dest)) throw `${dest} is not a folder that exists.`;
  if (dest.toLowerCase().startsWith(DEVICE_ROOT.toLowerCase())) {
    throw "Choose a folder on this computer, outside the library. To put the results in the library, make them here first and then add them (the same step as adding any audio files).";
  }
  const seen = new Set<number>();
  const claimed = new Map<string, string>();
  const items: PlanItem[] = [];
  for (const id of ids) {
    if (seen.has(id)) continue;
    seen.add(id);
    const t = tracks.find((x) => x.track_id === id);
    if (!t) {
      items.push(refused(id, `track ${id}`, "That track is not in the library."));
      continue;
    }
    const label = `${t.artist} - ${t.title}`;
    const base = safeName(label) || `track ${id}`;
    const vp = fileOf(dest, base, settings.vocals_suffix);
    const ip = fileOf(dest, base, settings.instrumental_suffix);
    let problem: string | null = null;
    if (settings.max_minutes > 0 && t.play_time_secs > settings.max_minutes * 60) {
      problem = `${Math.round(t.play_time_secs / 60)} minutes is longer than the ${settings.max_minutes}-minute limit in the settings.`;
    }
    for (const p of [vp, ip]) {
      const other = claimed.get(p.toLowerCase());
      if (other && !problem) problem = `${other} would be written to the same file name; rename one of the two tracks first.`;
    }
    if (!problem) for (const p of [vp, ip]) claimed.set(p.toLowerCase(), label);
    const ve = written.has(vp.toLowerCase());
    const ie = written.has(ip.toLowerCase());
    const item: PlanItem = {
      track_id: id,
      label,
      minutes: t.play_time_secs / 60,
      source: `${DEVICE_ROOT}.Pacemaker\\Music\\${t.location.split("/").pop()}`,
      vocals_path: vp,
      instrumental_path: ip,
      vocals_exists: ve,
      instrumental_exists: ie,
      write_vocals: true,
      write_instrumental: true,
      action: "make",
      what: "Writes the acapella and the instrumental.",
      problem,
      segments: segmentsFor(t.play_time_secs),
    };
    if (problem) {
      Object.assign(item, { action: "refuse" as PlanAction, write_vocals: false, write_instrumental: false, what: `Not done: ${problem}` });
    } else if (settings.existing === "replace" && (ve || ie)) {
      const n = Number(ve) + Number(ie);
      Object.assign(item, { action: "replace" as PlanAction, what: `Writes both files and REPLACES ${n} existing file${n === 1 ? "" : "s"}.` });
    } else if (ve && ie) {
      Object.assign(item, { action: "skip" as PlanAction, write_vocals: false, write_instrumental: false, what: "Skipped: both files already exist and are kept." });
    } else if (ve) {
      Object.assign(item, { action: "partial" as PlanAction, write_vocals: false, what: "Writes the instrumental only; the acapella already exists and is kept." });
    } else if (ie) {
      Object.assign(item, { action: "partial" as PlanAction, write_instrumental: false, what: "Writes the acapella only; the instrumental already exists and is kept." });
    }
    items.push(item);
  }
  const count = (a: PlanAction) => items.filter((i) => i.action === a).length;
  const runs = items.filter((i) => ["make", "replace", "partial"].includes(i.action));
  const replaced_files = items.filter((i) => i.action === "replace").flatMap((i) => [i.vocals_exists ? i.vocals_path : "", i.instrumental_exists ? i.instrumental_path : ""].filter(Boolean));
  return {
    digest: JSON.stringify([items.map((i) => [i.track_id, i.action, i.vocals_path, i.vocals_exists, i.instrumental_path, i.instrumental_exists]), replaced_files, dest]),
    dest,
    format_label: formatLabel(),
    items,
    make: count("make"),
    replace: count("replace"),
    partial: count("partial"),
    skip: count("skip"),
    refuse: count("refuse"),
    minutes: runs.reduce((a, i) => a + i.minutes, 0),
    replaced_files,
  };
}

function refused(id: number, label: string, problem: string): PlanItem {
  return {
    track_id: id,
    label,
    minutes: 0,
    source: "",
    vocals_path: "",
    instrumental_path: "",
    vocals_exists: false,
    instrumental_exists: false,
    write_vocals: false,
    write_instrumental: false,
    action: "refuse",
    what: `Not done: ${problem}`,
    problem,
    segments: 0,
  };
}

export const handlers: Handlers = {
  stems_status: () => status(),

  stems_settings_save: (a) => {
    const s = a.settings as StemSettings;
    if (!["auto", "cpu", "gpu"].includes(s.device)) throw "Device must be one of auto, cpu, gpu.";
    if (!["mp3", "ogg", "wav"].includes(s.format)) throw "Format must be one of mp3, ogg, wav.";
    if (s.overlap_percent < 10 || s.overlap_percent > 90) throw "Overlap must be between 10 and 90 percent.";
    if (!s.vocals_suffix.trim() || !s.instrumental_suffix.trim()) throw "A suffix must be 1 to 40 characters.";
    if (s.vocals_suffix.trim().toLowerCase() === s.instrumental_suffix.trim().toLowerCase()) throw "The two suffixes must differ, or both results would get the same file name.";
    settings = { ...s, vocals_suffix: s.vocals_suffix.trim(), instrumental_suffix: s.instrumental_suffix.trim() };
    return settings;
  },

  stems_download: (a) => {
    const p = parts.find((x) => x.id === a.id);
    if (!p) throw `Unknown download ${a.id}.`;
    if (a.acceptNoticeVersion !== p.notice_version) throw "The licence notice must be accepted before anything is downloaded.";
    if (installed.has(p.id)) throw `${p.name} is already downloaded.`;
    if (jobLock.running) throw "Another job is still running.";
    jobLock.running = true;
    cancelled = false;
    consents[p.id] = { notice_version: p.notice_version, accepted_at: Math.floor(Date.now() / 1000) };
    void (async () => {
      const steps = 20;
      for (let i = 1; i <= steps; i++) {
        await wait(150);
        if (cancelled) {
          jobLock.running = false;
          await emit("job-done", { job: "stems-download", ok: 0, failed: 0, summary: `Download of ${p.name} cancelled. Nothing was kept.`, error: null });
          return;
        }
        const done = Math.round((p.size_bytes * i) / steps);
        await emit("job-progress", {
          job: "stems-download",
          label: p.name,
          done,
          total: p.size_bytes,
          status: "downloading",
          bpm: null,
          detail: `${Math.round(done / 1e6)} of ${Math.round(p.size_bytes / 1e6)} MB`,
          error: null,
        });
      }
      installed.add(p.id);
      jobLock.running = false;
      await emit("job-done", { job: "stems-download", ok: 1, failed: 0, summary: `Downloaded ${p.name} and checked its SHA-256.`, error: null });
    })();
  },

  stems_remove: (a) => {
    if (jobLock.running) throw "A job is still running.";
    installed.delete(String(a.id));
  },

  stems_cancel: () => {
    cancelled = true;
  },

  stems_plan: (a) => buildPlan(a.trackIds as number[], String(a.dest)),

  stems_run: (a) => {
    const st = status();
    if (st.blocked) throw st.blocked;
    if (jobLock.running) throw "Another job is still running.";
    const plan = buildPlan(a.trackIds as number[], String(a.dest));
    const runs = plan.items.filter((i) => ["make", "replace", "partial"].includes(i.action));
    if (!runs.length) throw `Nothing to do: ${plan.skip} left alone, ${plan.refuse} cannot be done.`;
    if (plan.digest !== a.digest) throw "The files at the destination changed since you reviewed the plan. Review it again.";
    if (plan.replace > 0 && !a.confirmReplace) throw `This would replace ${plan.replaced_files.length} existing file(s); confirm that first.`;
    jobLock.running = true;
    cancelled = false;
    const device = settings.device === "cpu" ? "CPU" : "CPU (no GPU provider in this onnxruntime)";
    void (async () => {
      const count = plan.items.length;
      const result = { dest: plan.dest, device, written: [] as unknown[], skipped: [] as unknown[], failed: [] as unknown[], cancelled: false, limited_samples: 0 };
      await emit("stems-progress", { phase: "checking-model", index: 0, count, label: "", segment: 0, segments: 0, eta_secs: null, device: "" });
      await wait(300);
      await emit("stems-progress", { phase: "loading-model", index: 0, count, label: "", segment: 0, segments: 0, eta_secs: null, device: "" });
      await wait(400);
      // the mock caps every track at 12 parts so a demo finishes quickly
      const total = runs.reduce((n, i) => n + Math.min(i.segments, 12), 0);
      let done = 0;
      stop: for (let index = 0; index < plan.items.length; index++) {
        const item = plan.items[index];
        if (!["make", "replace", "partial"].includes(item.action)) {
          result.skipped.push({ label: item.label, error: item.what });
          continue;
        }
        const segs = Math.min(item.segments, 12);
        for (let s = 0; s <= segs; s++) {
          if (cancelled) {
            result.cancelled = true;
            break stop;
          }
          await emit("stems-progress", {
            phase: "separating",
            index,
            count,
            label: item.label,
            segment: s,
            segments: segs,
            eta_secs: done > 0 ? Math.round(((total - done) * 0.25) / 1) : null,
            device,
          });
          if (s < segs) {
            await wait(250);
            done++;
          }
        }
        if (item.write_vocals) written.add(item.vocals_path.toLowerCase());
        if (item.write_instrumental) written.add(item.instrumental_path.toLowerCase());
        result.written.push({
          track_id: item.track_id,
          label: item.label,
          vocals: item.write_vocals ? item.vocals_path : null,
          instrumental: item.write_instrumental ? item.instrumental_path : null,
          seconds: segs * 0.25,
        });
      }
      jobLock.running = false;
      await emit("stems-result", result);
      const n = result.written.length;
      await emit("job-done", {
        job: "stems",
        ok: n,
        failed: 0,
        summary: `${n} track${n === 1 ? "" : "s"} done in 0:${String(Math.round(total * 0.25)).padStart(2, "0")} (${device})${result.cancelled ? "; cancelled; the files of finished tracks were kept, nothing half-written was left" : ""}`,
        error: null,
      });
    })();
    return { tracks: runs.length };
  },
};
