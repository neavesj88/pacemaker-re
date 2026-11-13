// Mock of src-tauri/src/commands/export/setlist.rs (the printable setlist's notes store and file
// save). The store is a Map in memory, so a refresh of the preview starts empty again; nothing is
// written anywhere. The checks mirror the Rust ones closely enough for the UI to meet its errors.
import type { Handlers } from "./types";

const sheets = new Map<string, Record<string, unknown>>();

const line = (v: unknown, max: number) => String(v ?? "").replace(/[\x00-\x1f]/g, " ").trim().slice(0, max);

export const handlers: Handlers = {
  setlist_sheet_load: (a) => {
    if (!/^(case|mix):\d{1,18}$/.test(String(a.key))) throw `'${a.key}' is not a setlist key (expected case:<id> or mix:<id>).`;
    return { sheet: sheets.get(`${a.root}|${a.key}`) ?? { title: "", event: "", venue: "", date: "", dj: "", footer: "", start_clock: "", entries: [] }, warning: null };
  },
  setlist_sheet_save: (a) => {
    if (!/^(case|mix):\d{1,18}$/.test(String(a.key))) throw `'${a.key}' is not a setlist key (expected case:<id> or mix:<id>).`;
    const s = a.sheet as Record<string, any>;
    const clock = String(s.start_clock ?? "").trim();
    if (clock && !/^([01]?\d|2[0-3]):[0-5]\d$/.test(clock)) throw `'${clock}' is not a start time (use HH:MM on a 24-hour clock).`;
    const entries = new Map<number, { track_id: number; note: string; energy: number }>();
    for (const e of (s.entries ?? []) as { track_id: number; note: string; energy: number }[]) {
      if (e.energy > 10) throw "Energy is a rating from 1 to 10.";
      const note = line(e.note, 300);
      if (note === "" && !e.energy) entries.delete(e.track_id);
      else entries.set(e.track_id, { track_id: e.track_id, note, energy: e.energy });
    }
    const clean = {
      title: line(s.title, 200),
      event: line(s.event, 200),
      venue: line(s.venue, 200),
      date: line(s.date, 200),
      dj: line(s.dj, 200),
      footer: String(s.footer ?? "").trim().slice(0, 1000),
      start_clock: clock ? clock.padStart(5, "0") : "",
      entries: [...entries.values()].sort((x, y) => x.track_id - y.track_id),
    };
    const blank = Object.values(clean).every((v) => v === "" || (Array.isArray(v) && v.length === 0));
    if (blank) sheets.delete(`${a.root}|${a.key}`);
    else sheets.set(`${a.root}|${a.key}`, clean);
    return clean;
  },
  setlist_save_html: (a) => {
    const dest = String(a.dest ?? "");
    if (!/\.html?$/i.test(dest)) throw "The file name must end in .html.";
    if (/[\/]device[\/]/i.test(dest)) throw "Choose a folder outside the device: a setlist page is not part of its library.";
    return { path: dest, bytes: String(a.html ?? "").length };
  },
};
