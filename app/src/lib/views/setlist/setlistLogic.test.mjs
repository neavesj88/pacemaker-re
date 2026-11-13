// Run: node --experimental-strip-types --test src/lib/views/setlist/setlistLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  COLUMNS, DEFAULT_OPTIONS, EMPTY_SHEET, buildDocument, cleanOptions, entryOf, esc, fileNameFor, fmtBpm, fmtClock, fmtElapsed, fmtKey,
  parseClock, rowsFromCase, rowsFromMix, visibleColumns, withEntry,
} from "./setlistLogic.ts";

const O = DEFAULT_OPTIONS;
const parseKey = (k) => (k === "Am" ? { code: "8A", name: "A minor" } : k === "8B" ? { code: "8B", name: "C major" } : null);
const lib = new Map([
  [1, { title: "Demo Track 01", artist: "Example Artist A", bpm: 124, key: "Am", play_time_secs: 200 }],
  [2, { title: "Demo Track 02", artist: "Example Artist B", bpm: 127.54, key: "", play_time_secs: 185 }],
  [3, { title: "Demo Track 03", artist: "", bpm: 0, key: "8B", play_time_secs: 3700 }],
]);

test("fmtElapsed cuts seconds and switches to hours", () => {
  assert.equal(fmtElapsed(0), "0:00");
  assert.equal(fmtElapsed(59.9), "0:59");
  assert.equal(fmtElapsed(185), "3:05");
  assert.equal(fmtElapsed(3599), "59:59");
  assert.equal(fmtElapsed(3600), "1:00:00");
  assert.equal(fmtElapsed(3723), "1:02:03");
  assert.equal(fmtElapsed(-5), "0:00");
  assert.equal(fmtElapsed(NaN), "0:00");
});

test("clock times add the offset to the start, cut seconds and wrap past midnight", () => {
  assert.equal(fmtClock("21:00", 0), "21:00");
  assert.equal(fmtClock("21:00", 299), "21:04");
  assert.equal(fmtClock("21:00", 300), "21:05");
  assert.equal(fmtClock("9:05", 60), "09:06");
  assert.equal(fmtClock("23:50", 1200), "00:10");
  assert.equal(fmtClock("23:50", 86400 * 2), "23:50");
  assert.equal(fmtClock("", 10), null);
  assert.equal(fmtClock("25:00", 10), null);
  assert.equal(fmtClock("12:5", 10), null);
});

test("parseClock accepts only HH:MM on a 24-hour clock", () => {
  assert.deepEqual(parseClock(" 07:30 "), { h: 7, m: 30 });
  assert.deepEqual(parseClock("23:59"), { h: 23, m: 59 });
  for (const bad of ["", "24:00", "12:60", "1230", "12:3", "a:bc", "-1:00"]) assert.equal(parseClock(bad), null, bad);
});

test("fmtBpm rounds to one decimal and hides unknown tempos", () => {
  assert.equal(fmtBpm(128), "128");
  assert.equal(fmtBpm(127.54), "127.5");
  assert.equal(fmtBpm(127.56), "127.6");
  assert.equal(fmtBpm(0), "");
  assert.equal(fmtBpm(NaN), "");
  assert.equal(fmtBpm(-3), "");
});

test("fmtKey converts recognised keys and keeps the stored text for the rest", () => {
  assert.equal(fmtKey("Am", "stored", parseKey), "Am");
  assert.equal(fmtKey("Am", "camelot", parseKey), "8A");
  assert.equal(fmtKey("Am", "name", parseKey), "A minor");
  assert.equal(fmtKey("odd key", "camelot", parseKey), "odd key", "an unknown key is not dropped");
  assert.equal(fmtKey("  ", "camelot", parseKey), "");
});

test("fileNameFor removes characters Windows refuses and never returns an empty name", () => {
  assert.equal(fileNameFor('Late: set / "2"?'), "Late set 2 setlist.html");
  assert.equal(fileNameFor("   "), "Setlist setlist.html");
  assert.equal(fileNameFor("trailing dot."), "trailing dot setlist.html");
  assert.ok(fileNameFor("x".repeat(300)).length < 100);
});

test("esc escapes every HTML special character", () => {
  assert.equal(esc(`<a href="x">T&C's</a>`), "&lt;a href=&quot;x&quot;&gt;T&amp;C&#39;s&lt;/a&gt;");
});

test("cleanOptions repairs bad stored values and keeps the title column on", () => {
  const o = cleanOptions({ columns: { title: false, bpm: false, key: "yes" }, paper: "tabloid", size: "large", landscape: 1, hideEmpty: false });
  assert.equal(o.columns.title, true);
  assert.equal(o.columns.bpm, false);
  assert.equal(o.columns.key, true, "a non-boolean falls back to the default");
  assert.equal(o.paper, "auto");
  assert.equal(o.size, "large");
  assert.equal(o.landscape, false);
  assert.equal(o.hideEmpty, false);
  assert.deepEqual(cleanOptions(null), O);
  assert.deepEqual(cleanOptions("junk"), O);
  assert.deepEqual(cleanOptions(JSON.parse(JSON.stringify(O))), O);
});

test("withEntry adds, changes and removes entries", () => {
  let s = withEntry(EMPTY_SHEET, 5, { note: "intro" });
  assert.deepEqual(s.entries, [{ track_id: 5, note: "intro", energy: 0 }]);
  s = withEntry(s, 5, { energy: 7 });
  assert.deepEqual(entryOf(s, 5), { track_id: 5, note: "intro", energy: 7 });
  s = withEntry(s, 6, { energy: 2 });
  s = withEntry(s, 5, { note: "" });
  assert.equal(s.entries.length, 2);
  s = withEntry(s, 5, { energy: 0 });
  assert.deepEqual(s.entries, [{ track_id: 6, note: "", energy: 2 }], "an entry with nothing in it is removed");
  assert.deepEqual(entryOf(s, 99), { track_id: 99, note: "", energy: 0 });
  assert.deepEqual(EMPTY_SHEET.entries, [], "the shared empty sheet is never changed");
});

test("rowsFromCase keeps case order, lists a repeat once and runs a total", () => {
  const b = rowsFromCase([2, 1, 2, 3], lib);
  assert.deepEqual(b.rows.map((r) => r.trackId), [2, 1, 3]);
  assert.deepEqual(b.rows.map((r) => r.startSec), [0, 185, 385]);
  assert.deepEqual(b.rows.map((r) => r.lengthSec), [185, 200, 3700]);
  assert.equal(b.totalSec, 4085);
  assert.deepEqual(b.notes, []);
});

test("rowsFromCase marks a track missing from the library and says so; analysis-pending tempos are dropped", () => {
  const b = rowsFromCase([1, 77, 2], lib, new Set([1]));
  assert.equal(b.rows[1].missing, true);
  assert.equal(b.rows[1].lengthSec, null);
  assert.equal(b.rows[2].startSec, 200, "a missing track adds no time");
  assert.equal(b.rows[0].bpm, 0);
  assert.equal(b.rows[2].bpm, 127.54);
  assert.match(b.notes[0], /1 track in this case is no longer in the library/);
});

const clip = (id, trackId, startSec, endSec, extra = {}) => ({
  id, trackId, channel: 0, visibleStartSec: startSec, visibleEndSec: endSec, bpm: 120, clipBypassed: false, track: null, ...extra,
});
const info = (title, artist, bpm, key) => ({ title, artist, bpm, key });

test("rowsFromMix orders by start, starts at the first audible frame and keeps overlaps", () => {
  const doc = {
    clips: [
      clip("b", 2, 190, 400, { channel: 1 }),
      clip("a", 1, 10, 210, { track: info("Mix Tune One", "Mix Artist", 124, "Am") }),
    ],
  };
  const b = rowsFromMix(doc, lib, false);
  assert.deepEqual(b.rows.map((r) => r.trackId), [1, 2]);
  assert.deepEqual(b.rows.map((r) => r.startSec), [0, 180]);
  assert.deepEqual(b.rows.map((r) => r.lengthSec), [200, 210]);
  assert.equal(b.totalSec, 390);
  assert.equal(b.rows[0].title, "Mix Tune One", "the mix's own track info wins");
  assert.equal(b.rows[1].title, "Demo Track 02", "else the library row");
});

test("rowsFromMix leaves out muted clips unless asked, and says how many", () => {
  const doc = { clips: [clip("a", 1, 0, 100), clip("b", 2, 100, 200, { clipBypassed: true }), clip("c", 3, 200, 300)] };
  const out = rowsFromMix(doc, lib, false);
  assert.deepEqual(out.rows.map((r) => r.trackId), [1, 3]);
  assert.match(out.notes[0], /1 muted clip left out/);
  const inc = rowsFromMix(doc, lib, true);
  assert.deepEqual(inc.rows.map((r) => [r.trackId, r.muted]), [[1, false], [2, true], [3, false]]);
  assert.deepEqual(inc.notes, []);
});

test("rowsFromMix: a muted first clip does not shift the times, a removed track is a gap, a repeat is two rows", () => {
  const doc = { clips: [clip("m", 1, 0, 50, { clipBypassed: true }), clip("a", 2, 60, 160), clip("g", -1, 160, 200), clip("r", 2, 200, 260)] };
  const b = rowsFromMix(doc, lib, false);
  assert.deepEqual(b.rows.map((r) => r.startSec), [0, 100, 140]);
  assert.equal(b.rows[1].missing, true);
  assert.equal(b.rows[2].trackId, 2);
  assert.equal(b.totalSec, 200);
  assert.equal(b.notes.length, 2);
  assert.deepEqual(rowsFromMix({ clips: [] }, lib, false), { rows: [], totalSec: 0, notes: [] });
});

const input = (over = {}) => {
  const built = rowsFromCase([1, 2, 3], lib);
  return { name: "Warm-up", kind: "case", rows: built.rows, totalSec: built.totalSec, sheet: EMPTY_SHEET, options: O, parseKey, screenHint: false, ...over };
};

test("visibleColumns: empty columns are left out only when asked, title is always there", () => {
  assert.deepEqual(visibleColumns(input()), ["no", "time", "artist", "title", "bpm", "key"], "no energy and no notes yet");
  const sheet = withEntry(withEntry(EMPTY_SHEET, 1, { energy: 4 }), 2, { note: "drop at 2:00" });
  assert.deepEqual(visibleColumns(input({ sheet })), ["no", "time", "artist", "title", "bpm", "key", "energy", "note"]);
  const show = { ...O, hideEmpty: false };
  assert.equal(visibleColumns(input({ options: show })).includes("energy"), true);
  const off = cleanOptions({ columns: { artist: false, key: false, no: false } });
  assert.deepEqual(visibleColumns(input({ options: off })), ["time", "title", "bpm"]);
  assert.deepEqual(COLUMNS.map((c) => c.id).sort(), ["artist", "bpm", "energy", "key", "length", "no", "note", "time", "title"]);
});

test("the page has the heading, header details, rows in order, totals and no script", () => {
  const sheet = { ...EMPTY_SHEET, event: "Example Night", venue: "Example Room", dj: "Example DJ", footer: "Line one\nLine two" };
  const html = buildDocument(input({ sheet }));
  assert.match(html, /^<!doctype html>/);
  assert.match(html, /<title>Warm-up<\/title>/);
  assert.match(html, /<h1>Warm-up<\/h1>/);
  assert.match(html, /<b>Venue<\/b> Example Room/);
  assert.ok(!html.includes("<b>Date</b>"), "an empty detail is not printed");
  assert.ok(html.indexOf("Demo Track 01") < html.indexOf("Demo Track 02") && html.indexOf("Demo Track 02") < html.indexOf("Demo Track 03"));
  assert.match(html, /<td class="c-time num">0:00<\/td>/);
  assert.match(html, /<td class="c-time num">3:20<\/td>/);
  assert.match(html, /<td class="c-bpm num">127\.5<\/td>/);
  assert.match(html, /3 tracks · 1:08:05/);
  assert.match(html, /<p class="footer">Line one\nLine two<\/p>/);
  assert.ok(!/<script/i.test(html));
  assert.ok(!html.includes("Print command"), "no screen hint in the preview/print version");
});

test("a sheet title replaces the name; clock mode prints clock times; an invalid start falls back to elapsed", () => {
  const clock = { ...O, timeMode: "clock" };
  const ok = buildDocument(input({ options: clock, sheet: { ...EMPTY_SHEET, title: "Friday late", start_clock: "21:30" } }));
  assert.match(ok, /<h1>Friday late<\/h1>/);
  assert.match(ok, /<th class="c-time num">Time<\/th>/);
  assert.match(ok, /<td class="c-time num">21:30<\/td>/);
  assert.match(ok, /<td class="c-time num">21:33<\/td>/);
  const no = buildDocument(input({ options: clock }));
  assert.match(no, /<th class="c-time num">Start<\/th>/);
  assert.match(no, /<td class="c-time num">3:20<\/td>/);
});

test("every piece of text from the library or the user is escaped", () => {
  const evil = '<img src=x onerror="alert(1)">&';
  const rows = [{ trackId: 9, missing: false, muted: false, startSec: 0, lengthSec: 60, artist: evil, title: evil, bpm: 120, key: evil }];
  const sheet = { ...EMPTY_SHEET, title: evil, event: evil, venue: evil, date: evil, dj: evil, footer: evil, entries: [{ track_id: 9, note: evil, energy: 3 }] };
  const html = buildDocument(input({ name: evil, rows, totalSec: 60, sheet }));
  assert.ok(!html.includes("<img"), "no raw tag from the data");
  assert.ok(!html.includes('onerror="'), "no raw quote from the data");
  assert.ok(html.includes("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;&amp;"));
});

test("the page text matches the options: paper, orientation, size, zebra, muted tag, missing gap", () => {
  const o = cleanOptions({ paper: "letter", landscape: true, size: "xlarge", zebra: false });
  const html = buildDocument(input({ options: o }));
  assert.match(html, /@page \{ size: letter landscape; margin: 14mm; \}/);
  assert.match(html, /font: 18pt/);
  assert.ok(!html.includes("nth-child(even)"));
  assert.match(buildDocument(input({ options: { ...O, paper: "a4" } })), /size: A4 portrait/);
  assert.match(buildDocument(input()), /size: portrait/);
  const rows = [
    { trackId: 1, missing: false, muted: true, startSec: 0, lengthSec: 10, artist: "A", title: "T", bpm: 0, key: "" },
    { trackId: 2, missing: true, muted: false, startSec: 10, lengthSec: null, artist: "", title: "", bpm: 0, key: "" },
  ];
  const h = buildDocument(input({ rows, totalSec: 10 }));
  assert.match(h, /T<span class="tag">muted<\/span>/);
  assert.match(h, /<tr class="gap">.*Track no longer in the library/);
  assert.match(h, /2 tracks/);
  assert.match(buildDocument(input({ rows: [], totalSec: 0 })), /This setlist has no tracks/);
});

test("the saved version only adds the screen-only hint", () => {
  const preview = buildDocument(input());
  const saved = buildDocument(input({ screenHint: true }));
  assert.ok(saved.includes('class="hint"'));
  assert.equal(saved.replace(/<p class="hint">.*?<\/p>/, ""), preview);
});
