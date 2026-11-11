// Mock of src-tauri/src/commands/export.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
//
// The browser cannot run the Rust mixer, so a preview is a small synthetic WAV (a kick on every
// beat of the mix tempo, in a Blob) and an export is a timed fake job that reports progress and
// finishes. Nothing is written anywhere.
import { emit } from "@tauri-apps/api/event";
import type { EncodingSpec, LevelSettings, LoudnessReport, RangeInfo, RangePrefs, RangeSpec, TagSpec, TracklistEntry, TracklistFormat, TracklistOptions, TracklistOut } from "$lib/api/export";
import type { MixDoc } from "$lib/api/mixes";
import { DEFAULT_OPTIONS } from "$lib/state/tracklistprefs";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

const RATE = 22050;
let cancelled = false;

/** A mono 16-bit WAV with a decaying 60 Hz thump on every beat; `gainDb` scales it, so a preview
 * made with loudness gains is audibly louder or quieter. */
function clickWav(secs: number, bpm: number, gainDb = 0): Blob {
  const gain = Math.pow(10, gainDb / 20);
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
    const s = Math.max(-1, Math.min(1, Math.sin(2 * Math.PI * 60 * t) * Math.exp(-t * 14) * 0.6 * gain));
    v.setInt16(44 + i * 2, Math.round(s * 32767), true);
  }
  return new Blob([buf], { type: "audio/wav" });
}

/** The .cue beside an audio file, and the fixture rule for "already there": a file name containing
 * "existing" has a sheet next to it, so the replace prompt can be seen in the browser. */
const cueFor = (audio: string) => audio.replace(/\.[^.\\/]*$/, "") + ".cue";
const cueTaken = (cue: string) => /existing/i.test(cue.split(/[\\/]/).pop() ?? "");

/** Likewise a file name containing "occupied" is an audio file that is already there. */
const audioTaken = (audio: string) => /occupied/i.test(audio.split(/[\\/]/).pop() ?? "");

/** The cover fixtures the real check tells apart: a name containing "missing" or "huge". */
function coverProblem(path: string): string | null {
  const name = path.split(/[\\/]/).pop() ?? path;
  if (/missing/i.test(name)) return `The cover image ${name} cannot be read: file not found. Choose another image or remove the cover.`;
  if (/huge/i.test(name)) return `The cover image ${name} is 12 MB; the most a tag takes is 8 MB. Use a smaller image.`;
  return null;
}

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

// ---- partial export and loudness --------------------------------------------------------------
// Browser-side stand-ins for src-tauri/src/commands/export/{range,loudness}.rs: the same rules in
// miniature (a range resolves against the clips that are heard; gains are the distance to a target,
// limited), with invented readings derived from the fixture clips. The real commands are verified
// by the Rust tests; do not grow this to match them.

type MockClip = MixDoc["clips"][number];
const heardClips = (doc: MixDoc) => doc.clips.filter((c) => !c.clipBypassed && c.visibleEnd > c.visibleStart && c.visibleEnd > 0).sort((a, b) => a.visibleStart - b.visibleStart);
const clipName = (c?: MockClip) => c?.track?.title || c?.fuzzyId.title || "a clip";

function mockRange(doc: MixDoc, spec: RangeSpec, prefs?: Partial<RangePrefs> | null): RangeInfo {
  const heard = heardClips(doc);
  if (!heard.length) throw "The mix has no audible clips.";
  const first = heard[0].visibleStartSec;
  const last = Math.max(...heard.map((c) => c.visibleEndSec));
  const info = (from: number, to: number, whole: boolean): RangeInfo => ({
    whole,
    fromSecs: from,
    toSecs: to,
    durationSecs: to - from,
    clipIds: heard.filter((c) => c.visibleEndSec > from && c.visibleStartSec < to).map((c) => c.id),
  });
  if (!spec || spec.kind === "whole") return info(first, last, true);
  if (spec.kind === "time") {
    const from = Math.max(0, spec.fromSec);
    const to = Math.min(spec.toSec, last);
    if (!Number.isFinite(spec.fromSec) || !Number.isFinite(spec.toSec)) throw "The range needs a start and an end time.";
    if (spec.toSec <= from) throw "The range is empty: it must end after it starts.";
    if (from >= last) throw "The range starts after the mix has ended.";
    const r = info(from, to, false);
    if (!r.clipIds.length) throw "No track is heard in that range.";
    return r;
  }
  const pick = (id: string) => {
    const c = doc.clips.find((x) => x.id === id);
    if (!c) throw "A clip of the range is not in the mix any more (it may have been deleted).";
    if (!heard.includes(c)) throw `${clipName(c)} is not heard in the export (it is bypassed or trimmed away), so it cannot start or end a range.`;
    return c;
  };
  const [a, b] = [pick(spec.fromClip), pick(spec.toClip)].sort((x, y) => x.visibleStart - y.visibleStart);
  const ender = a.visibleEnd > b.visibleEnd ? a : b;
  let to = ender.visibleEndSec;
  if (prefs?.endAt === "nextEnters") {
    const next = heard.find((c) => c !== ender && c.visibleStart > ender.visibleStart && c.visibleStart > a.visibleStart);
    if (next) to = Math.min(to, next.visibleStartSec);
  }
  return info(a.visibleStartSec, to, false);
}

/** An invented but stable loudness for a fixture clip. */
const clipLufs = (c: MockClip) => -9 - ((Math.abs(c.trackId) * 7 + 3) % 11);

function mockReport(doc: MixDoc, spec: RangeSpec, prefs: Partial<RangePrefs> | null, l: Partial<LevelSettings> | null, source: "analysis" | "export"): LoudnessReport {
  const lv: LevelSettings = { targetLufs: null, maxBoostDb: 12, maxCutDb: 24, capAtPeak: false, peakCeilingDb: -1, clipLevel: "off", clipFixedLufs: -14, clipMaxDb: 9, ...(l ?? {}) };
  const range = mockRange(doc, spec, prefs);
  const clips = doc.clips.filter((c) => range.clipIds.includes(c.id));
  const own = clips.map(clipLufs);
  const sorted = [...own].sort((a, b) => a - b);
  const median = sorted.length ? (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.ceil((sorted.length - 1) / 2)]) / 2 : 0;
  const ref = lv.clipLevel === "fixed" ? lv.clipFixedLufs : median;
  const rows = lv.clipLevel === "off" ? [] : clips.map((c, i) => ({ clipId: c.id, lufs: own[i], truePeakDb: -2 - (i % 3), gainDb: Math.round(Math.max(-lv.clipMaxDb, Math.min(lv.clipMaxDb, ref - own[i])) * 100) / 100, note: null }));
  const before = clips.length ? 10 * Math.log10(own.reduce((s, x, i) => s + Math.pow(10, (x + (rows[i]?.gainDb ?? 0)) / 10), 0) / clips.length) : null;
  const peak = -3;
  let gain = 0;
  const notes: string[] = [];
  if (lv.targetLufs !== null && before !== null) {
    const want = lv.targetLufs - before;
    gain = Math.max(-lv.maxCutDb, Math.min(lv.maxBoostDb, want));
    if (lv.capAtPeak) gain = Math.min(gain, lv.peakCeilingDb - peak);
    gain = Math.round(gain * 100) / 100;
    if (Math.abs(gain - want) > 1e-9) notes.push(`The gain needed for ${lv.targetLufs.toFixed(1)} LUFS is ${want > 0 ? "+" : ""}${want.toFixed(1)} dB; it was limited to ${gain > 0 ? "+" : ""}${gain.toFixed(1)} dB.`);
  }
  const limiterWillAct = peak + gain > -0.18;
  const after = before === null ? null : limiterWillAct ? before + gain - 1.5 : before;
  const reading = (lufs: number | null, tp: number) => ({ lufs: lufs === null ? null : Math.round(lufs * 100) / 100, truePeakDb: tp, samplePeakDb: tp - 0.3 });
  const clipGainDb: Record<string, number> = {};
  for (const r of rows) clipGainDb[r.clipId] = r.gainDb;
  return {
    source,
    fromSecs: range.fromSecs,
    toSecs: range.toSecs,
    clips: rows,
    clipReferenceLufs: lv.clipLevel === "off" ? null : ref,
    beforeLimiter: reading(before, peak),
    afterLimiter: reading(after, Math.min(peak, -0.6)),
    targetLufs: lv.targetLufs,
    masterGainDb: gain,
    predictedPeakDb: peak + gain,
    limiterWillAct,
    result: source === "export" ? reading(before === null ? null : (limiterWillAct ? before + gain - 1.5 : before + gain), Math.min(peak + gain, -0.6)) : null,
    notes,
    gains: { clipGainDb, masterGainDb: gain },
  };
}

export const handlers: Handlers = {
  export_render_preview: (a) => {
    const doc = a.mix as MixDoc;
    const from = Number(a.fromSec);
    const to = Number(a.toSec);
    if (!Number.isFinite(from) || !Number.isFinite(to) || from < 0 || to <= from) throw "The preview range is empty.";
    if (to - from > 900) throw "A preview can be at most 15 minutes; export the mix to hear more.";
    if (!doc.clips.length) throw "the mix has no audible clips";
    const secs = Math.min(to - from, 20);
    const g = a.gains as { clipGainDb: Record<string, number>; masterGainDb: number } | null;
    return {
      path: URL.createObjectURL(clickWav(secs, doc.tempoBpm ?? 128, g ? g.masterGainDb : 0)),
      durationSecs: secs,
      clipsFailed: doc.clips.filter((c) => c.trackId < 0).length,
      warnings: doc.clips.some((c) => c.trackId < 0) ? ["clip skipped: the track was removed from the library"] : [],
    };
  },
  export_range_resolve: (a) => mockRange(a.mix as MixDoc, a.range as RangeSpec, a.rangePrefs as Partial<RangePrefs> | null),
  export_loudness_analyse: (a) => {
    const doc = a.mix as MixDoc;
    const report = mockReport(doc, (a.range as RangeSpec) ?? { kind: "whole" }, a.rangePrefs as Partial<RangePrefs> | null, a.levels as Partial<LevelSettings> | null, "analysis");
    if (jobLock.running) throw "Another job is still running.";
    jobLock.running = true;
    cancelled = false;
    void (async () => {
      const total = Math.max(1, Math.round(report.toSecs - report.fromSecs));
      for (let i = 1; i <= 4; i++) {
        await wait(350);
        if (cancelled) {
          jobLock.running = false;
          await emit("job-done", { job: "mix-loudness", ok: 0, failed: 0, summary: `Measuring ${doc.name} was cancelled.`, error: null });
          return;
        }
        await emit("job-progress", { job: "mix-loudness", label: doc.name, done: Math.round((total * i) / 4), total, status: "measuring", bpm: null, detail: null, error: null });
      }
      jobLock.running = false;
      await emit("mix-loudness-report", report);
      await emit("job-done", { job: "mix-loudness", ok: 1, failed: 0, summary: `Measured ${doc.name}`, error: null });
    })();
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
    return { cuePath, exists: cueTaken(cuePath), audioExists: audioTaken(String(a.dest)) };
  },
  export_cover_check: (a) => coverProblem(String(a.path)),
  export_mix_audio: (a) => {
    const doc = a.mix as MixDoc;
    if (!["ogg", "wav", "flac", "mp3"].includes(String(a.format))) throw `Unknown audio format '${a.format}' (use ogg, wav, flac or mp3).`;
    checkDest(String(a.dest), String(a.format));
    // the real command checks the settings and the tags before the job starts (encoding.rs): the
    // same sentences for the cases a fixture can show (a cover whose name contains "missing" or
    // "huge", a bad bitrate)
    const enc = (a.encoding as Partial<EncodingSpec> | null) ?? {};
    if (enc.mp3?.kind === "cbr" && ![96, 112, 128, 160, 192, 224, 256, 320].includes(enc.mp3.kbps)) throw `${enc.mp3.kbps} kbit/s is not an MP3 bitrate here (use one of [96, 112, 128, 160, 192, 224, 256, 320]).`;
    if (enc.mp3?.kind === "vbr" && !(enc.mp3.quality >= 0 && enc.mp3.quality <= 9)) throw `MP3 variable-bitrate quality ${enc.mp3.quality} is out of range (0 is best, 9 smallest).`;
    if (enc.mp3Effort !== undefined && !(Number.isInteger(enc.mp3Effort) && enc.mp3Effort >= 0 && enc.mp3Effort <= 9)) throw `MP3 encoder effort ${enc.mp3Effort} is out of range (0 is best and slowest, 9 fastest).`;
    if (enc.oggQuality !== undefined && !(Number.isInteger(enc.oggQuality) && enc.oggQuality >= -1 && enc.oggQuality <= 10)) throw `Ogg quality ${enc.oggQuality} is out of range (use -1 to 10, in tenths).`;
    if (enc.flacBits !== undefined && enc.flacBits !== 16 && enc.flacBits !== 24) throw `${enc.flacBits} bits is not a FLAC sample size here (use 16 or 24).`;
    const tags = a.tags as TagSpec | null;
    const coverName = tags?.coverPath?.split(/[\\/]/).pop();
    const coverBad = tags?.coverPath ? coverProblem(tags.coverPath) : null;
    if (coverBad) throw coverBad;
    const chapterCount = tags?.chapters ? mockTracklist(doc, withDefaults(tags.chapters), String(a.dest)).entries.length : 0;
    const noCover = a.format === "wav" && !!coverName;
    const cueSheet = a.cue ? mockTracklist(doc, withDefaults(a.cue), String(a.dest)) : null;
    if (cueSheet?.cueIssue) throw `Cannot write a CUE sheet: ${cueSheet.cueIssue}. Turn the CUE sheet off to export the audio.`;
    const cuePath = cueSheet ? cueFor(String(a.dest)) : null;
    const cueExists = !!cuePath && cueTaken(cuePath);
    if (cueExists && !a.replaceCue) throw `${cuePath!.split(/[\\/]/).pop()} is already there and would be replaced. Confirm the replacement, or turn the CUE sheet off to export the audio only.`;
    if (jobLock.running) throw "Another job is still running.";
    if (!doc.clips.length) throw "the mix has no audible clips";
    const spec = (a.range as RangeSpec | null) ?? { kind: "whole" };
    const levels = a.levels as Partial<LevelSettings> | null;
    const report = mockReport(doc, spec, a.rangePrefs as Partial<RangePrefs> | null, levels, "export");
    const measuring = !!levels && (levels.targetLufs != null || (levels.clipLevel ?? "off") !== "off");
    // seconds exported (the range, or first audible frame to last clip end), like the Rust command
    const total = Math.max(1, Math.ceil(report.toSecs - report.fromSecs));
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
          status: measuring && i <= steps / 2 ? "measuring" : "rendering",
          bpm: null,
          detail: null,
          error: null,
        });
      }
      jobLock.running = false;
      const m = Math.floor(total / 60);
      const s = String(total % 60).padStart(2, "0");
      const res = report.result;
      const level = res?.lufs != null ? `, ${res.lufs.toFixed(1)} LUFS, ${res.truePeakDb?.toFixed(1)} dBTP` : "";
      await emit("mix-loudness-report", report);
      await emit("job-done", {
        job: "mix-export",
        ok: 1,
        failed: skipped,
        summary: `Exported ${doc.name} (${m}:${s}) to ${String(a.dest).split(/[\\/]/).pop()}${skipped ? ` with ${skipped} clip(s) left out, so it is not the complete mix` : ""}${level}${chapterCount ? `, ${chapterCount} chapter(s)` : ""}${cueSheet ? " and the CUE sheet" : ""}${noCover ? ". A WAV file cannot hold a cover image, so it was left out." : ""}`,
        error: null,
      });
    })();
    return { path: String(a.dest), cuePath, cueReplaces: cueExists };
  },
  export_cancel: () => {
    cancelled = true;
  },
};
