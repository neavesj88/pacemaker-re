// Mock of src-tauri/src/commands/metadata.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts. Placeholder data only.
import { tracks } from "./db";
import { recordMockUndo } from "./device-trust";
import type { Handlers } from "./types";

const MAX_TEXT = 255;
const MAX_NUM = 9999;
const TEXT = ["title", "artist", "album_artist", "album", "composer", "genre", "label", "remixer", "producer", "comments"];
const NUMS = ["year", "track_number", "number_of_tracks", "disc_number", "number_of_discs"];

// fields the shared fixture does not carry, created on first use
type Extra = Record<string, string | number>;
const extras = new Map<number, Extra>();
const extraFor = (id: number): Extra => {
  let e = extras.get(id);
  if (!e) {
    e = {
      album_artist: id % 4 === 0 ? "Various Artists" : "",
      composer: id % 5 === 0 ? "Example Composer" : "",
      label: id % 3 === 0 ? "Sample Records" : "",
      remixer: id % 6 === 0 ? "Example Remixer" : "",
      producer: "",
      comments: id % 7 === 0 ? "Demo comment" : "",
      year: 2000 + (id % 20),
      track_number: (id % 12) + 1,
      number_of_tracks: 12,
      disc_number: 1,
      number_of_discs: 1,
    };
    extras.set(id, e);
  }
  return e;
};

type Row = (typeof tracks)[number];
const find = (id: number): Row => {
  const t = tracks.find((x) => x.track_id === id);
  if (!t) throw `invalid: no track ${id}`;
  return t;
};
const get = (t: Row, k: string): string | number => ((t as Record<string, unknown>)[k] ?? extraFor(t.track_id)[k]) as string | number;
const set = (t: Row, k: string, v: string | number) => {
  if (k in t) (t as Record<string, unknown>)[k] = v;
  else extraFor(t.track_id)[k] = v;
};

const detail = (t: Row) => ({
  track_id: t.track_id,
  ...Object.fromEntries([...TEXT, ...NUMS].map((k) => [k, get(t, k)])),
  rating: t.rating,
  bpm: t.bpm,
  key: t.key,
});

export const handlers: Handlers = {
  metadata_set_rating: (a) => {
    const t = find(a.trackId);
    if (a.stars < 0 || a.stars > 5) throw `invalid: rating ${a.stars} is above 5`;
    const was = t.rating;
    t.rating = a.stars;
    // the device area's undo history (devmock/device-trust.ts) is fed by every edit
    if (was !== a.stars) {
      const stars = (n: number) => (n === 0 ? "unrated" : `${n} star${n === 1 ? "" : "s"}`);
      const who = `${t.artist} - ${t.title}`;
      recordMockUndo("rating", `Rated ${who} ${stars(a.stars)} (was ${stars(was)})`,
        [{ track_id: t.track_id, label: who, what: "Rating", current: stars(a.stars), becomes: stars(was), skip: null }],
        () => { t.rating = was; });
    }
  },
  metadata_track_details: (a) => (a.trackIds as number[]).map((id) => detail(find(id))),
  metadata_save: (a) => {
    const edits = a.edits as Record<string, any>[];
    // validate everything first so nothing is written on an error (the real command is one transaction)
    for (const e of edits) {
      find(e.track_id);
      for (const k of TEXT) {
        if (e[k] === undefined) continue;
        if ([...String(e[k])].length > MAX_TEXT) throw `invalid: ${k} is longer than ${MAX_TEXT} characters`;
      }
      for (const k of NUMS) {
        if (e[k] === undefined) continue;
        if (!Number.isInteger(e[k]) || e[k] < 0 || e[k] > MAX_NUM) throw `invalid: ${k} must be between 0 and ${MAX_NUM}`;
      }
      if (e.rating !== undefined && (!Number.isInteger(e.rating) || e.rating < 0 || e.rating > 5)) {
        throw `invalid: rating ${e.rating} is above 5`;
      }
    }
    let changed = 0;
    const undone: { row: Row; key: string; value: string | number }[] = [];
    const lines: Parameters<typeof recordMockUndo>[2] = [];
    for (const e of edits) {
      const t = find(e.track_id);
      let any = false;
      for (const k of [...TEXT, ...NUMS, "rating"]) {
        if (e[k] === undefined || get(t, k) === e[k]) continue;
        const was = get(t, k);
        undone.push({ row: t, key: k, value: was });
        lines.push({ track_id: t.track_id, label: `${t.artist} - ${t.title}`, what: k[0].toUpperCase() + k.slice(1).replace(/_/g, " "), current: String(e[k]), becomes: String(was), skip: null });
        set(t, k, e[k]);
        any = true;
      }
      if (any) changed++;
    }
    // the device area's undo history (devmock/device-trust.ts) is fed by every edit, except the
    // clean-up page's invented demo names (metacleanup.ts), which the user never made
    if (changed > 0 && !a.seed) {
      const names = [...new Set(lines.map((l) => l.what.toLowerCase()))].join(", ");
      const only = lines.length === 1 ? lines[0] : null;
      recordMockUndo(lines.every((l) => l.what === "Rating") ? "rating" : "metadata",
        changed === 1 ? `Edited ${only?.label ?? `${edits.length} track`}: ${names}` : `Edited ${changed} tracks: ${names}`,
        lines, () => { for (const u of undone.reverse()) set(u.row, u.key, u.value); });
    }
    return changed;
  },
};
