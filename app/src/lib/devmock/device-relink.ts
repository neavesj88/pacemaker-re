// Mock of the missing-file relocation commands in src-tauri/src/commands/device/relink.rs.
// Invented data only: the tracks without audio come from the trust-pack mock (Health card), the
// "found" files are made-up paths. Picked up automatically by devmock/index.ts.
//
// Switches in the folder's name: "nowhere" is not a folder, "empty" holds nothing that fits,
// ".Pacemaker" (the card's Music folder) holds one renamed file already on the card, "big" is cut
// off at the file limit.
import { emit } from "@tauri-apps/api/event";
import { missingMockTracks, restoreMockAudio } from "./device-trust";
import { runSteps } from "./device";
import type { Handlers } from "./types";

const MB = 1024 * 1024;

type Settings = {
  use_tags: boolean; duration_tolerance_secs: number; max_tag_reads: number; subfolders: boolean; max_files: number;
  card_files: "reuse" | "standard"; preselect: "none" | "sure" | "likely"; last_folder: string;
};

let settings: Settings = {
  use_tags: true, duration_tolerance_secs: 2, max_tag_reads: 20_000, subfolders: true, max_files: 100_000,
  card_files: "reuse", preselect: "sure", last_folder: "",
};

type Cand = {
  path: string; name: string; size: number; kind: "exact" | "same_name" | "tags" | "manual"; confidence: "sure" | "likely" | "check";
  notes: string[]; duration_secs: number | null; action: "point" | "copy"; dest: string; new_location: string; on_card: boolean; non_standard: boolean;
};

const PREFIX = "/pmdb_tracks/";
const AUDIO = /\.(mp3|flac|wav|m4a|aac|ogg|opus|aiff?|wma|mp4)$/i;
const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

/** Where a track's file is expected on the card, relative to Music. */
const relOf = (location: string) => location.startsWith(PREFIX) ? location.slice(PREFIX.length) : location;

function copyCand(path: string, size: number, kind: Cand["kind"], confidence: Cand["confidence"], notes: string[], location: string): Cand {
  return {
    path, name: fileName(path), size, kind, confidence, notes, duration_secs: 200, action: "copy",
    dest: relOf(location), new_location: location, on_card: false, non_standard: false,
  };
}

let cancelled = false;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const handlers: Handlers = {
  device_relink_settings: () => ({ settings: { ...settings }, warning: null }),
  device_relink_settings_set: (a) => {
    const s = a.settings as Settings;
    settings = {
      ...s,
      duration_tolerance_secs: Math.min(60, Math.max(0, s.duration_tolerance_secs)),
      max_files: Math.min(1_000_000, Math.max(100, s.max_files)),
      last_folder: s.last_folder.trim(),
    };
    return { settings: { ...settings }, warning: null };
  },

  device_relink_scan: async (a) => {
    const folder = String(a.folder).trim();
    const lower = folder.toLowerCase();
    if (lower.includes("nowhere")) throw `invalid: ${folder} is not a folder`;
    cancelled = false;
    const steps = [
      { phase: 0, done: 0, total: 0, label: folder },
      { phase: 0, done: 214, total: 0, label: `${folder}\\Old sets` },
      { phase: 1, done: 40, total: 428, label: "demo-a.mp3" },
      { phase: 1, done: 260, total: 428, label: "demo-b.flac" },
      { phase: 1, done: 428, total: 428, label: "" },
    ];
    for (const p of steps) {
      await emit("relink-scan-progress", p);
      await sleep(260);
      if (cancelled) throw "invalid: Cancelled: the search was stopped";
    }
    const missing = missingMockTracks();
    const onCard = lower.includes(".pacemaker");
    const empty = lower.includes("empty");
    const rows = missing.map((m, i) => {
      const candidates: Cand[] = [];
      const notes: string[] = [];
      if (!empty) {
        if (onCard && i === missing.length - 1) {
          candidates.push({
            path: `${folder}\\misc\\renamed demo ${m.track_id}.mp3`, name: `renamed demo ${m.track_id}.mp3`, size: 8_041_000, kind: "tags", confidence: "sure",
            notes: ["Same title and artist in its tags.", "Same length and same size.", "Its path under Music is not one this app generated. The device should follow any plain path there, but that has not been checked on hardware; \"copy to the standard name\" in the options avoids the question."],
            duration_secs: 201, action: "point", dest: `misc/renamed demo ${m.track_id}.mp3`, new_location: `${PREFIX}misc/renamed demo ${m.track_id}.mp3`, on_card: true, non_standard: true,
          });
        } else if (!onCard && i === 0) {
          candidates.push(copyCand(`C:\\Music\\Demo\\Sets\\demo${m.track_id}.mp3`, 8_012_345, "exact", "sure", ["Same file name and size as when the track was added."], m.location));
        } else if (!onCard && i === 1) {
          candidates.push(
            copyCand(`C:\\Music\\Demo\\Old\\demo${m.track_id}.mp3`, 8_163_000, "same_name", "likely", [
              "Same file name, but 8163000 bytes instead of the 8012345 the library records: it was re-tagged or re-encoded since. The beat grid may not line up with it; re-analyse the track afterwards.",
              "Its length fits the track.",
            ], `${PREFIX}4/b/${m.track_id}c0ffee.mp3`),
            copyCand(`D:\\Backup\\Tracks\\demo${m.track_id}.mp3`, 8_172_000, "same_name", "check", [
              "Same file name, but 8172000 bytes instead of the 8012345 the library records: it was re-tagged or re-encoded since. The beat grid may not line up with it; re-analyse the track afterwards.",
              "Its length is 6:02 but the track is 3:20: it may be a different version.",
            ], `${PREFIX}5/b/${m.track_id}c0ffee.mp3`),
          );
        } else if (!onCard && i === 2 && missing.length > 2) {
          notes.push(`C:\\Music\\Demo\\demo${m.track_id}.mp3 is already used by another track, so it cannot also be this one's audio`);
        }
      }
      return {
        track_id: m.track_id, label: m.label, location: m.location, file_size: 8_012_345, play_time_secs: 200, format: "MP3",
        title: `Demo Track ${m.track_id}`, artist: "Example Artist", in_mixes: m.in_mixes, candidates, notes,
      };
    });
    const big = lower.includes("big");
    return {
      folder,
      total_missing: missing.length,
      files_seen: big ? 100_000 : 428,
      tag_reads: empty ? 0 : 61,
      truncated: big,
      tag_limit_hit: false,
      rows,
      warnings: [
        ...(big ? ["The folder holds more than 100000 audio files; only the first 100000 were looked at. Choose a narrower folder to be sure of finding everything."] : []),
        ...(onCard && !empty ? ["Some matches are files already on the card under a path this app did not generate. The track will point at that path, and whether the Pacemaker plays a track from such a path has not been checked on hardware. To avoid the question, set \"A match already on the card\" to copy it to the name this app gives new files."] : []),
      ],
      skipped_locations: 0,
      music_missing: false,
      tags_without_length: 0,
      non_standard: onCard && !empty,
    };
  },
  device_relink_scan_cancel: () => {
    cancelled = true;
  },

  device_relink_inspect: (a) => {
    const m = missingMockTracks().find((x) => x.track_id === a.trackId);
    if (!m) throw "invalid: that track's audio file is not missing (any more)";
    const picked = String(a.path);
    const path = AUDIO.test(picked) ? picked : "C:\\Music\\Demo\\picked file.mp3";
    return {
      path, name: fileName(path), size: 7_300_000, kind: "manual", confidence: "check",
      notes: [
        "You picked this file; nothing else says it is the track's audio.",
        "Size 7300000 bytes, the library records 8012345.",
        "Its name is not the one the track was added with.",
        "Its length is 3:05, the track's is 3:20.",
        "The beat grid is kept as it is; if this is a different recording it will not line up, so re-analyse the track afterwards.",
      ],
      duration_secs: 185, action: "copy", dest: `7/d/${m.track_id}beef01.mp3`, new_location: `${PREFIX}7/d/${m.track_id}beef01.mp3`, on_card: false, non_standard: false,
    } satisfies Cand;
  },

  device_relink_run: (a) => {
    const picks = a.picks as { track_id: number; path: string; manual: boolean; expect_location: string }[];
    if (picks.length === 0) throw "No matches are ticked.";
    const missing = missingMockTracks();
    const ok = picks.filter((p) => missing.some((m) => m.track_id === p.track_id));
    const stale = picks.filter((p) => !ok.includes(p));
    const label = (id: number) => missing.find((m) => m.track_id === id)?.label ?? `track ${id}`;
    const labels = ok.map((p) => `${/\.Pacemaker/i.test(p.path) ? "Relinking" : "Copying"} ${label(p.track_id)}`);
    const copies = ok.filter((p) => !/\.Pacemaker/i.test(p.path)).length;
    const parts = [`Relinked ${ok.length} track${ok.length === 1 ? "" : "s"}`];
    if (copies) parts[0] += ` (${copies} copied onto the card, ${(copies * 7.6).toFixed(1)} MB)`;
    if (stale.length) parts.push(`${stale.length} track${stale.length === 1 ? "" : "s"} left alone: ${stale.map((p) => `track ${p.track_id}: its audio is back, or it is not a track any more`).join("; ")}`);
    runSteps("device-relink", labels, "copying", parts.join(". "), () => restoreMockAudio(ok.map((p) => p.track_id)), (d, t) => `${d} of ${t}`);
  },
};
