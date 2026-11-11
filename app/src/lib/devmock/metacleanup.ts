// Mock of src-tauri/src/commands/metadata/{cleanup,tagwrite}.rs. Placeholder data only. The
// recipes run on the TypeScript port (cleanupEngine.ts), held to the Rust engine by the golden table.
import { emit } from "@tauri-apps/api/event";
import { FEAT_FORMS, TEXT_FIELDS, cleanFields, problemOf } from "./cleanupEngine";
import golden from "./cleanupGolden.json";
import { jobLock } from "./jobs";
import { handlers as metadata } from "./metadata";
import { tracks } from "./db";
import type { CleanupSettings } from "../api/metacleanup";
import type { Handlers } from "./types";

// Browser preview only: when the clean-up page is first opened, some of the mock library's
// tracks get messy names (invented) so the tool has something to show. The real app never runs
// this file; the other pages of the mock see the messy names only after this page was opened.
let seeded = false;
const seedMessyNames = () => {
  if (seeded) return;
  seeded = true;
  const edits: Record<string, unknown>[] = [];
  tracks.forEach((t, i) => {
    const n = String(i + 1).padStart(2, "0");
    if (i % 4 === 0) edits.push({ track_id: t.track_id, title: `${n} - ${t.title} (Free Download)`, artist: `${t.artist}  ft. Guest Artist`, genre: "Tech-House" });
    else if (i % 4 === 1) edits.push({ track_id: t.track_id, title: `  ${t.title} [www.example.test] `, genre: "Drum n Bass" });
    else if (i % 4 === 2) edits.push({ track_id: t.track_id, title: `${t.title} (Example Remixer Remix)` });
  });
  try {
    metadata.metadata_save({ edits });
  } catch {
    /* the preview just shows clean data */
  }
};

let saved: CleanupSettings = structuredClone(golden.defaults) as CleanupSettings;

const detailsOf = (ids: number[]) =>
  ids.flatMap((id) => {
    try {
      return [(metadata.metadata_track_details({ trackIds: [id] }) as Record<string, any>[])[0]];
    } catch {
      return []; // an id removed since the caller looked is skipped
    }
  });

const sanitise = (s: CleanupSettings): CleanupSettings => {
  const out = structuredClone(s);
  const known = (v: string[]) => [...new Set(v)].filter((f) => (TEXT_FIELDS as readonly string[]).includes(f));
  out.case_fields = known(out.case_fields);
  out.promo_fields = known(out.promo_fields);
  if (!FEAT_FORMS.includes(out.feat_form)) out.feat_form = "feat.";
  for (const m of [out.genre_map, out.artist_map]) {
    for (const e of m) {
      e.from = e.from.trim();
      e.to = e.to.trim();
    }
  }
  out.number_max_digits = Math.min(6, Math.max(1, Math.trunc(out.number_max_digits) || 3));
  for (const key of ["keep_upper", "small_words", "artist_separators"] as const) {
    out[key] = out[key].map((x) => x.replace(/\s+/g, " ").trim()).filter((x) => x);
  }
  out.genre_map = out.genre_map.filter((e) => e.from && e.to);
  out.artist_map = out.artist_map.filter((e) => e.from && e.to);
  return out;
};

type Change = { field: string; old: string; new: string; problem: string | null };
type Row = { track_id: number; artist: string; title: string; changes: Change[] };

function preview(details: Record<string, any>[], s: CleanupSettings) {
  const rows: Row[] = [];
  for (const d of details) {
    const cur: Record<string, string> = Object.fromEntries(TEXT_FIELDS.map((f) => [f, String(d[f] ?? "")]));
    const now = cleanFields(cur, s);
    const changes = TEXT_FIELDS.filter((f) => cur[f] !== now[f]).map((f) => ({
      field: f as string,
      old: cur[f],
      new: now[f],
      problem: problemOf(f, cur[f], now[f]),
    }));
    if (changes.length) rows.push({ track_id: d.track_id, artist: d.artist, title: d.title, changes });
  }
  return { scanned: details.length, rows };
}

// the audio files of the mock card: what each file's tags hold (invented, derived from the id)
const fileValue = (id: number, field: string, want: string): string => {
  if (id % 3 === 0) return "";
  return id % 2 === 0 ? want.toUpperCase() : want;
};
const unsupportedExt = (id: number) => id % 7 === 0;
const fmt = (id: number) => (id % 4 === 0 ? { name: "FLAC (Vorbis comments)", ext: "flac" } : { name: "MP3 (ID3v2)", ext: "mp3" });

export const handlers: Handlers = {
  metadata_cleanup_settings: () => {
    seedMessyNames();
    return { settings: structuredClone(saved), warning: null };
  },
  metadata_cleanup_defaults: () => structuredClone(golden.defaults),
  metadata_cleanup_save_settings: (a) => {
    saved = sanitise(a.settings as CleanupSettings);
    return structuredClone(saved);
  },
  metadata_cleanup_preview: (a) => preview(detailsOf(a.trackIds as number[]), sanitise(a.settings as CleanupSettings)),
  metadata_cleanup_apply: (a) => {
    const s = sanitise(a.settings as CleanupSettings);
    const accepted = a.accepted as { track_id: number; field: string; old: string; new: string }[];
    const ids = [...new Set(accepted.map((x) => x.track_id))];
    const now = new Map<string, Change>();
    for (const r of preview(detailsOf(ids), s).rows) for (const c of r.changes) now.set(`${r.track_id}:${c.field}`, c);
    let stale = 0;
    let rejected = 0;
    const edits = new Map<number, Record<string, any>>();
    let fields = 0;
    for (const x of accepted) {
      const c = now.get(`${x.track_id}:${x.field}`);
      if (!c || c.old !== x.old || c.new !== x.new) stale++;
      else if (c.problem) rejected++;
      else {
        edits.set(x.track_id, { ...(edits.get(x.track_id) ?? { track_id: x.track_id }), [x.field]: x.new });
        fields++;
      }
    }
    const changed = edits.size ? (metadata.metadata_save({ edits: [...edits.values()] }) as number) : 0;
    return { tracks: changed, fields, stale, rejected };
  },

  metadata_tags_formats: () => [
    { ext: "mp3", container: "MP3", tag: "ID3v2" },
    { ext: "flac", container: "FLAC", tag: "Vorbis comments" },
    { ext: "ogg", container: "Ogg Vorbis", tag: "Vorbis comments" },
    { ext: "opus", container: "Opus", tag: "Vorbis comments" },
    { ext: "m4a", container: "MP4 audio", tag: "MP4 atoms" },
    { ext: "aac", container: "AAC (ADTS)", tag: "ID3v2" },
    { ext: "wav", container: "WAV", tag: "ID3v2 chunk" },
    { ext: "aif", container: "AIFF", tag: "ID3v2 chunk" },
  ],
  metadata_tags_plan: (a) =>
    (a.items as Record<string, any>[]).map((item) => {
      const t = tracks.find((x) => x.track_id === item.track_id);
      const id = item.track_id as number;
      const label = t ? `${t.artist} - ${t.title}` : `track ${id}`;
      const file = t ? t.location.replace("/pmdb_tracks/", "") : "";
      if (!t) return { track_id: id, label, file: "", format: "", status: "error", changes: [], unsupported_fields: [], notes: [], message: "that track is not in the library" };
      if (unsupportedExt(id)) {
        return { track_id: id, label, file: file.replace(/\.mp3$/, ".wma"), format: "", status: "unsupported", changes: [], unsupported_fields: [], notes: [], message: "tags of this file type are not written" };
      }
      const f = fmt(id);
      const changes: { field: string; old: string; new: string }[] = [];
      const unsupported: string[] = [];
      for (const field of Object.keys(item)) {
        if (field === "track_id" || item[field] === undefined || item[field] === null) continue;
        const want = String(item[field]);
        if (!want || want === "0") continue;
        if (field === "producer" && f.ext === "mp3") {
          unsupported.push(field);
          continue;
        }
        const old = fileValue(id, field, want);
        if (old !== want) changes.push({ field, old, new: want });
      }
      return {
        track_id: id,
        label,
        file: file.replace(/\.mp3$/, `.${f.ext}`),
        format: f.name,
        status: changes.length ? "write" : "same",
        changes,
        unsupported_fields: unsupported,
        notes: id % 5 === 0 && f.ext === "mp3" ? ["its ID3v2.2 tag is upgraded to ID3v2.4 (all frames kept)"] : [],
        message: changes.length ? "" : "the tags already match",
      };
    }),
  metadata_tags_write: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const items = a.items as Record<string, any>[];
    jobLock.running = true;
    let written = 0;
    let failed = 0;
    items.forEach((item, i) =>
      setTimeout(async () => {
        const id = item.track_id as number;
        const t = tracks.find((x) => x.track_id === id);
        const bad = !t || unsupportedExt(id);
        if (bad) failed++;
        else written++;
        await emit("job-progress", {
          job: "tag-write",
          label: t ? `${t.artist} - ${t.title} (${t.location.replace("/pmdb_tracks/", "")})` : `track ${id}`,
          done: i + 1,
          total: items.length,
          status: bad ? "failed" : "written",
          bpm: null,
          detail: null,
          error: bad ? "tags of this file type are not written" : null,
        });
        if (i === items.length - 1) {
          jobLock.running = false;
          await emit("job-done", {
            job: "tag-write",
            ok: written,
            failed,
            summary: [`${written} file${written === 1 ? "" : "s"} tagged`, failed && `${failed} failed (left as they were)`].filter(Boolean).join(", "),
            error: null,
          });
        }
      }, 500 * (i + 1)),
    );
  },
};
