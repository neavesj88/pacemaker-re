// Mock of src-tauri/src/commands/export.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
//
// The browser cannot run the Rust mixer, so a preview is a small synthetic WAV (a kick on every
// beat of the mix tempo, in a Blob) and an export is a timed fake job that reports progress and
// finishes. Nothing is written anywhere.
import { emit } from "@tauri-apps/api/event";
import type { TracklistEntry, TracklistFormat, TracklistOptions, TracklistOut } from "$lib/api/export";
import type { MixDoc } from "$lib/api/mixes";
import { DEFAULT_OPTIONS } from "$lib/state/tracklistprefs";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

const RATE = 22050;
let cancelled = false;

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

/** The .cue beside an audio file, and the fixture rule for "already there": a file name containing
 * "existing" has a sheet next to it, so the replace prompt can be seen in the browser. */
const cueFor = (audio: string) => audio.replace(/\.[^.\\/]*$/, "") + ".cue";
const cueTaken = (cue: string) => /existing/i.test(cue.split(/[\\/]/).pop() ?? "");

/** The real command refuses these before the job starts (render.rs `check_dest`). */
function checkDest(dest: string, format: string) {
  if (/[\\/]$/.test(dest)) throw "The destination is a folder; choose a file name.";
  if (!dest.toLowerCase().endsWith("." + format)) throw `The file name must end in .${format} to match the chosen format.`;
  if (/[\\/]device[\\/]/i.test(dest)) throw "Choose a folder outside the device: files written there would not be in its library.";
}

// ---- track list ------------------------------------------------------------------------------
// A browser-side version of src-tauri/src/commands/export/tracklist.rs, enough to see every format
// in the dialog: the same rules (the first audible frame is 0:00, bypassed and fully trimmed clips
// are not in the audio, YouTube chapters kept 10 s apart), none of the Rust tests. The real command
// is the one that is verified; do not extend this to match it line by line.

const SR = 44100;
const pad = (n: number) => String(n).padStart(2, "0");
function stamp(secs: number, hours: boolean, padded: boolean) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor(secs / 60) % 60;
  const s = secs % 60;
  if (hours) return padded ? `${pad(h)}:${pad(m)}:${pad(s)}` : `${h}:${pad(m)}:${pad(s)}`;
  return padded ? `${pad(Math.floor(secs / 60))}:${pad(s)}` : `${Math.floor(secs / 60)}:${pad(s)}`;
}
const clean = (s: string) => s.replace(/\s+/g, " ").trim();

function mockTracklist(doc: MixDoc, o: TracklistOptions, audioFileName: string): TracklistOut {
  const audible = doc.clips.filter((c) => !c.clipBypassed && c.visibleEnd > c.visibleStart && c.visibleEnd > 0);
  if (!audible.length) throw "the mix has no audible clips";
  const from = Math.min(...audible.map((c) => c.visibleStart));
  const total = Math.max(...audible.map((c) => c.visibleEnd)) - from;
  const warnings: string[] = [];
  const listed = audible
    .filter((c) => {
      const ok = c.trackId >= 0 && (c.track?.available ?? true);
      if (!ok && !o.includeUnavailable) warnings.push(`${c.track?.title || "A clip"} is left out of the list: its audio is not on the device, so the export leaves it out too`);
      return ok || o.includeUnavailable;
    })
    .sort((a, b) => a.visibleStart - b.visibleStart || a.visibleEnd - b.visibleEnd);
  let latestEnd = -Infinity;
  let floor = 0;
  const entries: TracklistEntry[] = listed.map((c, i) => {
    const overlap = latestEnd > c.visibleStart;
    const at = o.stampAt === "crossfadeMid" && overlap ? c.visibleStart + Math.floor((Math.min(latestEnd, c.visibleEnd) - c.visibleStart) / 2) : c.visibleStart;
    latestEnd = Math.max(latestEnd, c.visibleEnd);
    floor = Math.max(floor, Math.round(at) - Math.round(from));
    return {
      number: i + 1,
      clipId: c.id,
      trackId: c.trackId,
      artist: clean(c.track?.artist ?? c.fuzzyId.artist),
      title: clean(c.track?.title || c.fuzzyId.title) || "Unknown track",
      album: clean(c.track?.album ?? ""),
      key: clean(c.track?.key ?? ""),
      bpm: c.track?.bpm ?? c.bpm,
      available: c.trackId >= 0 && (c.track?.available ?? true),
      startFrames: floor,
      startSecs: floor / SR,
      startLabel: "",
      overlapsPrevious: overlap,
    };
  });
  const secsOf = (e: TracklistEntry) => (o.rounding === "floor" ? Math.floor(e.startFrames / SR) : Math.round(e.startFrames / SR));
  const hours = secsOf({ startFrames: Math.max(total, entries.at(-1)?.startFrames ?? 0) } as TracklistEntry) >= 3600;
  const name = (e: TracklistEntry) => (e.artist ? `${e.artist} - ${e.title}` : e.title);
  for (const e of entries) e.startLabel = stamp(secsOf(e), hours, true);
  const txt = entries.map((e) => (o.numbered ? `${pad(e.number)}. ` : "") + `${e.startLabel} ${name(e)}\n`).join("");
  const mixcloud = entries.map((e) => `${e.number}. ${name(e)} (${e.startLabel})\n`).join("");
  const cell = (v: string) => {
    const t = /^[=+\-@\t\r]/.test(v) ? "'" + v : v;
    return /[,"\n\r]/.test(t) ? `"${t.replace(/"/g, '""')}"` : t;
  };
  const csv =
    "Number,Start,Seconds,Artist,Title,Album,Key,BPM\r\n" +
    entries.map((e) => [e.number, e.startLabel, e.startSecs.toFixed(3), cell(e.artist), cell(e.title), cell(e.album), cell(e.key), e.bpm > 0 ? e.bpm.toFixed(1) : ""].join(",") + "\r\n").join("");

  // YouTube: the first chapter at 0:00, at least three, each at least 10 s long
  const issues: string[] = [];
  const chapters: { secs: number; name: string; n: number }[] = [];
  const fold = (n: number, nm: string, why: string) => {
    const last = chapters[chapters.length - 1];
    if (o.closeEntries === "merge") {
      if (!last.name.split(" / ").includes(nm)) last.name += " / " + nm;
      issues.push(`Track ${n} ${why}; its name was added to the chapter before it.`);
    } else issues.push(`Track ${n} ${why}; it is not a chapter.`);
  };
  entries.forEach((e, i) => {
    const nm = clean(name(e).replace(/[<>]/g, ""));
    const real = secsOf(e);
    if (i === 0) {
      if (real > 0) issues.push(`The first track enters at ${stamp(real, real >= 3600, false)}, but a YouTube chapter list has to begin at 0:00, so it starts there.`);
      chapters.push({ secs: 0, name: nm, n: e.number });
    } else if (real < chapters[chapters.length - 1].secs + 10) fold(e.number, nm, "starts less than 10 seconds after the chapter before it");
    else chapters.push({ secs: real, name: nm, n: e.number });
  });
  if (chapters.length > 1 && Math.floor(total / SR) < chapters[chapters.length - 1].secs + 10) {
    const last = chapters.pop()!;
    fold(last.n, last.name, "would be a chapter shorter than 10 seconds at the end of the video");
  }
  const youtubeValid = chapters.length >= 3;
  if (!youtubeValid) issues.push(`YouTube needs at least 3 chapters to show them, and this list gives ${chapters.length}. Add tracks (each at least 10 seconds long) or post the list as plain text.`);
  const youtube = chapters.map((c) => `${stamp(c.secs, c.secs >= 3600, false)} ${c.name}\n`).join("");

  // CUE sheet: exact to 1/75 s
  let cue = "";
  let cueIssue: string | null = null;
  if (!entries.length) cueIssue = "there is no track to put in a CUE sheet";
  else if (entries.length > 99) cueIssue = `a CUE sheet holds at most 99 tracks and this mix has ${entries.length}`;
  else {
    const q = (v: string) => clean(v).replace(/"/g, "'");
    const idx = (frames: number) => {
      const f = Math.floor(Math.max(0, frames) / 588);
      return `${pad(Math.floor(f / 4500))}:${pad(Math.floor(f / 75) % 60)}:${pad(f % 75)}`;
    };
    const lines = ['REM GENERATOR "Pacemaker Revival"'];
    if (q(doc.artist)) lines.push(`PERFORMER "${q(doc.artist)}"`);
    lines.push(`TITLE "${q(doc.name)}"`, `FILE "${q(audioFileName.split(/[\\/]/).pop() ?? "")}" WAVE`);
    for (const e of entries) {
      lines.push(`  TRACK ${pad(e.number)} AUDIO`, `    TITLE "${q(e.title)}"`);
      if (q(e.artist)) lines.push(`    PERFORMER "${q(e.artist)}"`);
      if (e.number === 1 && e.startFrames >= 588) lines.push("    INDEX 00 00:00:00");
      lines.push(`    INDEX 01 ${idx(e.startFrames)}`);
    }
    cue = lines.join("\r\n") + "\r\n";
  }
  return { entries, durationSecs: total / SR, txt, csv, youtube, youtubeValid, youtubeIssues: issues, mixcloud, cue, cueIssue, warnings };
}

const withDefaults = (o: unknown): TracklistOptions => ({ ...DEFAULT_OPTIONS, ...((o ?? {}) as Partial<TracklistOptions>) });

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
      durationSecs: secs,
      clipsFailed: doc.clips.filter((c) => c.trackId < 0).length,
      warnings: doc.clips.some((c) => c.trackId < 0) ? ["clip skipped: the track was removed from the library"] : [],
    };
  },
  export_tracklist: (a) => mockTracklist(a.mix as MixDoc, withDefaults(a.options), String(a.audioFileName ?? "")),
  export_tracklist_save: (a) => {
    const ext = ({ txt: "txt", csv: "csv", youtube: "txt", mixcloud: "txt", cue: "cue" } as Record<string, string>)[String(a.format as TracklistFormat)];
    if (!ext) throw `Unknown track list format '${a.format}' (use txt, csv, youtube, mixcloud or cue).`;
    if (!String(a.dest).toLowerCase().endsWith("." + ext)) throw `The file name must end in .${ext} to match the chosen format.`;
    if (/[\\/]device[\\/]/i.test(String(a.dest))) throw "Choose a folder outside the device: files written there would not be in its library.";
    mockTracklist(a.mix as MixDoc, withDefaults(a.options), String(a.audioFileName ?? ""));
    return String(a.dest); // nothing is written in the browser
  },
  export_cue_target: (a) => {
    const cuePath = cueFor(String(a.dest));
    return { cuePath, exists: cueTaken(cuePath) };
  },
  export_mix_audio: (a) => {
    const doc = a.mix as MixDoc;
    if (a.format !== "ogg" && a.format !== "wav") throw `Unknown audio format '${a.format}' (use ogg or wav).`;
    checkDest(String(a.dest), String(a.format));
    const cueSheet = a.cue ? mockTracklist(doc, withDefaults(a.cue), String(a.dest)) : null;
    if (cueSheet?.cueIssue) throw `Cannot write a CUE sheet: ${cueSheet.cueIssue}. Turn the CUE sheet off to export the audio.`;
    const cuePath = cueSheet ? cueFor(String(a.dest)) : null;
    const cueExists = !!cuePath && cueTaken(cuePath);
    if (cueExists && !a.replaceCue) throw `${cuePath!.split(/[\\/]/).pop()} is already there and would be replaced. Confirm the replacement, or turn the CUE sheet off to export the audio only.`;
    if (jobLock.running) throw "Another job is still running.";
    if (!doc.clips.length) throw "the mix has no audible clips";
    // audible span (first audible frame to last clip end), like the Rust command
    const total = Math.max(1, Math.ceil(Math.max(...doc.clips.map((c) => c.visibleEndSec)) - Math.min(...doc.clips.map((c) => c.visibleStartSec))));
    const skipped = doc.clips.filter((c) => c.trackId < 0).length;
    jobLock.running = true;
    cancelled = false;
    void (async () => {
      const steps = 6;
      for (let i = 1; i <= steps; i++) {
        await wait(450);
        if (cancelled) {
          jobLock.running = false;
          await emit("job-done", { job: "mix-export", ok: 0, failed: 0, summary: `Export of ${doc.name} was cancelled. No file was written.`, error: null });
          return;
        }
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
        summary: `Exported ${doc.name} (${m}:${s}) to ${String(a.dest).split(/[\\/]/).pop()}${cueSheet ? " and the CUE sheet" : ""}${skipped ? ` with ${skipped} clip(s) left out, so it is not the complete mix` : ""}`,
        error: null,
      });
    })();
    return { path: String(a.dest), cuePath, cueReplaces: cueExists };
  },
  export_cancel: () => {
    cancelled = true;
  },
};
