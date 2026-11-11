// Run: node src/lib/views/tracks/query.test.mjs  (Node >= 22.18 strips the types itself)
import assert from "node:assert/strict";
import { test } from "node:test";
import { compileQuery, dateSpan, parseDuration, parseSize, QUERY_HELP, tokenize } from "./query.ts";

const DAY = 86_400_000;
const NOW = new Date(2026, 5, 15, 12, 0, 0).getTime(); // 15 June 2026, local noon
const local = (y, m, d) => new Date(y, m - 1, d).getTime();

const T = (id, over = {}) => ({
  track_id: id, title: `Title ${id}`, artist: "", album: "", genre: "House", bpm: 126, key: "Am", play_time_secs: 240,
  rating: 0, location: "", file_size: 8 * 1024 * 1024, format: "MP3", analysed: true, times_played: 0, last_played: -1,
  year: 0, label: "", remixer: "", composer: "", comments: "", date_added: -1, bit_rate: 0, ...over,
});

const noLabels = () => undefined;
const cx = (over = {}) => ({ labelOf: noLabels, needs: new Set(), isFavourite: () => false, now: NOW, ...over });
const run = (query, list, context = cx()) => {
  const q = compileQuery(query);
  return list.filter((t) => q.test(t, context)).map((t) => t.track_id);
};

test("tokens: words, phrases, fields, operators, negation, OR", () => {
  const view = (s) => tokenize(s).map((t) => (t === null ? "OR" : `${t.negated ? "-" : ""}${t.field ?? ""}${t.field ? t.op : ""}${t.value}${t.quoted ? "(q)" : ""}`));
  assert.deepEqual(view("deep house"), ["deep", "house"]);
  assert.deepEqual(view('"summer night" x'), ["summer night(q)", "x"]);
  assert.deepEqual(view("bpm:120-128 rating>=4 plays>10 energy<=3 year=2012"), ["bpm:120-128", "rating>=4", "plays>10", "energy<=3", "year=2012"]);
  assert.deepEqual(view("bpm:>=128"), ["bpm>=128"], "an operator after the colon is the same as without it");
  assert.deepEqual(view('artist:"foo bar" -genre:techno -"a b" -x'), ['artist:foo bar(q)', "-genre:techno", "-a b(q)", "-x"]);
  assert.deepEqual(view("a OR b OR"), ["a", "OR", "b", "OR"]);
  assert.deepEqual(view('time:3:00-5:00 key:~8A'), ["time:3:00-5:00", "key:~8A"]);
  assert.deepEqual(view('"OR"'), ["OR(q)"], "a quoted OR is a word");
  assert.deepEqual(view("or"), ["or"], "only capitals separate");
  assert.deepEqual(view('title:"unclosed phrase'), ["title:unclosed phrase(q)"], "an unclosed quote runs to the end");
  assert.deepEqual(view("   "), []);
});

test("an empty or half-typed query matches everything and says nothing", () => {
  const list = [T(1), T(2)];
  for (const q of ["", "   ", "bpm:", "key:~", "artist:", "-", "OR", "tag:", "-bpm:"]) {
    const c = compileQuery(q);
    assert.equal(c.empty, true, JSON.stringify(q));
    assert.deepEqual(c.diagnostics, [], JSON.stringify(q));
    assert.deepEqual(run(q, list), [1, 2]);
  }
});

test("words match title, artist, album, genre, key, comments, label, remixer, composer, year and tags; each word separately", () => {
  const list = [
    T(1, { title: "Midnight Drive", artist: "Example Artist" }),
    T(2, { comments: "great for the sunset" }),
    T(3, { label: "Example Records" }),
    T(4, { remixer: "Remixer Name" }),
    T(5, { composer: "Composer Name" }),
    T(6, { year: 2012 }),
    T(7, { genre: "Techno", key: "F#m" }),
    T(8),
  ];
  const labels = { 8: { tags: ["Peak hour"], colour: null, energy: null } };
  const c = cx({ labelOf: (id) => labels[id] });
  assert.deepEqual(run("midnight", list, c), [1]);
  assert.deepEqual(run("DRIVE example", list, c), [1], "both words, any order, any case");
  assert.deepEqual(run("sunset", list, c), [2]);
  assert.deepEqual(run("records", list, c), [3]);
  assert.deepEqual(run("remixer", list, c), [4]);
  assert.deepEqual(run("composer", list, c), [5]);
  assert.deepEqual(run("2012", list, c), [6]);
  assert.deepEqual(run("f#m", list, c), [7]);
  assert.deepEqual(run("peak", list, c), [8], "a tag is searched too");
  assert.deepEqual(run('"peak hour"', list, c), [8]);
  assert.deepEqual(run('"hour peak"', list, c), [], "a phrase is in order");
  assert.deepEqual(run("-example", list, c), [2, 4, 5, 6, 7, 8]);
});

test("labels are only consulted when no text field matched", () => {
  let calls = 0;
  const c = cx({ labelOf: () => (calls++, undefined) });
  run("midnight", [T(1, { title: "Midnight" }), T(2)], c);
  assert.equal(calls, 1, "only the track whose text did not match asks for its tags");
});

test("field:text is a contains match on that field only", () => {
  const list = [T(1, { artist: "Alpha", title: "Beta" }), T(2, { artist: "Beta", title: "Alpha" }), T(3, { label: "Example Label", remixer: "Some Remixer", composer: "C. Poser", comments: "note", format: "FLAC" })];
  assert.deepEqual(run("artist:alp", list), [1]);
  assert.deepEqual(run("title:alp", list), [2]);
  assert.deepEqual(run("t:alp a:bet", list), [2], "aliases t: and a: are title and artist");
  assert.deepEqual(run("label:example", list), [3]);
  assert.deepEqual(run("remixer:some", list), [3]);
  assert.deepEqual(run("remix:some", list), [3]);
  assert.deepEqual(run("composer:poser", list), [3]);
  assert.deepEqual(run("comments:note", list), [3]);
  assert.deepEqual(run("comment:note", list), [3]);
  assert.deepEqual(run("format:flac", list), [3]);
  assert.deepEqual(run('artist:"alpha"', list), [1]);
  assert.deepEqual(run('label:""', list), [1, 2], "an empty quoted value finds the tracks where the field is empty");
  assert.deepEqual(run("-artist:alp", list), [2, 3]);
  assert.deepEqual(run("ARTIST:ALP", list), [1], "field names ignore case");
});

test("numbers: equals, ranges, comparisons; bpm equals within half a beat", () => {
  const list = [T(1, { bpm: 127.5 }), T(2, { bpm: 128 }), T(3, { bpm: 128.6 }), T(4, { bpm: 120 }), T(5, { bpm: 0 })];
  assert.deepEqual(run("bpm:128", list), [1, 2]);
  assert.deepEqual(run("bpm=128", list), [1, 2]);
  assert.deepEqual(run("bpm:120-128", list), [1, 2, 4], "both ends included");
  assert.deepEqual(run("bpm:120..128", list), [1, 2, 4]);
  assert.deepEqual(run("bpm:128-120", list), [1, 2, 4], "a backwards range is read forwards");
  assert.deepEqual(run("bpm>128", list), [3]);
  assert.deepEqual(run("bpm>=128", list), [2, 3]);
  assert.deepEqual(run("bpm<127.5", list), [4]);
  assert.deepEqual(run("bpm<=127.5", list), [1, 4]);
  assert.deepEqual(run("bpm:>=128", list), [2, 3]);
  assert.deepEqual(run("-bpm:120-128", list), [3, 5], "a track without a tempo is not in the range, so negation keeps it");
  assert.deepEqual(run("bpm:127.5-128.5", list), [1, 2]);

  const tracks = [T(1, { year: 2010, rating: 5, times_played: 12, bit_rate: 320 }), T(2, { year: 2015, rating: 3, times_played: 10, bit_rate: 192 }), T(3, { year: 0, rating: 0, times_played: 0, bit_rate: 0 })];
  assert.deepEqual(run("year:2010-2015", tracks), [1, 2]);
  assert.deepEqual(run("year:2010", tracks), [1]);
  assert.deepEqual(run("year>2010", tracks), [2], "no year never compares");
  assert.deepEqual(run("rating>=4", tracks), [1]);
  assert.deepEqual(run("rating:0", tracks), [3]);
  assert.deepEqual(run("stars>=3", tracks), [1, 2]);
  assert.deepEqual(run("plays>10", tracks), [1]);
  assert.deepEqual(run("plays>=10", tracks), [1, 2]);
  assert.deepEqual(run("plays:0", tracks), [3]);
  assert.deepEqual(run("bitrate>=256", tracks), [1]);
  assert.deepEqual(run("bitrate:190-200", tracks), [2]);
});

test("a track that needs analysis has no bpm to match, as the table shows none", () => {
  const list = [T(1, { bpm: 128 }), T(2, { bpm: 128 })];
  assert.deepEqual(run("bpm:128", list, cx({ needs: new Set([2]) })), [1]);
});

test("duration, size", () => {
  const list = [T(1, { play_time_secs: 150 }), T(2, { play_time_secs: 210 }), T(3, { play_time_secs: 330 }), T(4, { play_time_secs: 0 })];
  assert.deepEqual(run("time>3:30", list), [3]);
  assert.deepEqual(run("time>=3:30", list), [2, 3]);
  assert.deepEqual(run("time:3:00-5:00", list), [2]);
  assert.deepEqual(run("duration<200", list), [1]);
  assert.deepEqual(run("length:210", list), [2]);
  assert.equal(parseDuration("3:30"), 210);
  assert.equal(parseDuration("1:02:03"), 3723);
  assert.equal(parseDuration("90"), 90);
  assert.equal(parseDuration("3:75"), null);
  assert.equal(parseDuration("abc"), null);

  const MB = 1024 * 1024;
  const files = [T(1, { file_size: 5 * MB }), T(2, { file_size: 12 * MB }), T(3, { file_size: 700 * 1024 })];
  assert.deepEqual(run("size>10", files), [2], "a bare number is megabytes");
  assert.deepEqual(run("size>10m", files), [2]);
  assert.deepEqual(run("size<1m", files), [3]);
  assert.deepEqual(run("size:500k-800k", files), [3]);
  assert.equal(parseSize("1.5g"), 1.5 * 1024 ** 3);
  assert.equal(parseSize("12x"), null);
});

test("keys: exact, compatible, by number, by name, none", () => {
  const list = [T(1, { key: "Am" }), T(2, { key: "8B" }), T(3, { key: "Em" }), T(4, { key: "Dm" }), T(5, { key: "F#m" }), T(6, { key: "" }), T(7, { key: "C" })];
  assert.deepEqual(run("key:8A", list), [1]);
  assert.deepEqual(run("key:Am", list), [1], "musical names work");
  assert.deepEqual(run("key:~8A", list), [1, 2, 3, 4, 7], "same, +-1 step, relative major");
  assert.deepEqual(run("key:~Am", list), [1, 2, 3, 4, 7]);
  assert.deepEqual(run("key:8", list), [1, 2, 7], "just the number takes both letters");
  assert.deepEqual(run("key:none", list), [6]);
  assert.deepEqual(run("-key:~8A", list), [5, 6]);
  const bad = compileQuery("key:zz");
  assert.equal(bad.empty, true);
  assert.match(bad.diagnostics[0].message, /not a key/);
});

test("tags: whole tag, any case, wildcards, quoted spaces", () => {
  const labels = {
    1: { tags: ["Peak", "warm up"], colour: "red", energy: 8 },
    2: { tags: ["peaks"], colour: "blue", energy: 3 },
    3: { tags: [], colour: null, energy: null },
  };
  const c = cx({ labelOf: (id) => labels[id] });
  const list = [T(1), T(2), T(3)];
  assert.deepEqual(run("tag:peak", list, c), [1], "a whole tag, so peaks is another tag");
  assert.deepEqual(run("tag:PEAK", list, c), [1]);
  assert.deepEqual(run("tag:peak*", list, c), [1, 2]);
  assert.deepEqual(run("tag:*ak", list, c), [1]);
  assert.deepEqual(run('tag:"warm up"', list, c), [1]);
  assert.deepEqual(run("tag:warm", list, c), []);
  assert.deepEqual(run("tags:peaks", list, c), [2]);
  assert.deepEqual(run("-tag:peak*", list, c), [3]);
  assert.deepEqual(run("tag:a.c", list, c), [], "wildcard text is not a regular expression");
  assert.deepEqual(run("colour:red", list, c), [1]);
  assert.deepEqual(run("color:blue", list, c), [2]);
  assert.deepEqual(run("colour:none", list, c), [3]);
  assert.deepEqual(run("energy>=7", list, c), [1]);
  assert.deepEqual(run("energy:1-5", list, c), [2]);
  assert.deepEqual(run("is:untagged", list, c), [3]);
  assert.match(compileQuery("colour:mauve").diagnostics[0].message, /not one of red, orange/);
});

test("dates: absolute, spans, comparisons, relative ages, never", () => {
  const list = [
    T(1, { date_added: local(2026, 6, 14), last_played: local(2026, 6, 14) }),
    T(2, { date_added: local(2026, 6, 1), last_played: local(2026, 3, 2) }),
    T(3, { date_added: local(2025, 12, 31), last_played: -1 }),
    T(4, { date_added: -1, last_played: -1 }),
  ];
  assert.deepEqual(run("added:7d", list), [1], "within the last 7 days");
  assert.deepEqual(run("added:2w", list), [1], "two weeks back from noon on the 15th is noon on the 1st");
  assert.deepEqual(run("added:3w", list), [1, 2]);
  assert.deepEqual(run("added<30d", list), [1, 2], "newer than");
  assert.deepEqual(run("added>30d", list), [3], "older than");
  assert.deepEqual(run("added>=30d", list), [3]);
  assert.deepEqual(run("added:1y", list), [1, 2, 3]);
  assert.deepEqual(run("added:2026", list), [1, 2]);
  assert.deepEqual(run("added:2026-06", list), [1, 2]);
  assert.deepEqual(run("added:2026-06-14", list), [1]);
  assert.deepEqual(run("added>=2026-06-02", list), [1]);
  assert.deepEqual(run("added>2026-06-01", list), [1], "after the whole day");
  assert.deepEqual(run("added<2026-01-01", list), [3]);
  assert.deepEqual(run("added<=2025-12-31", list), [3]);
  assert.deepEqual(run("added:2026-03-01..2026-06-01", list), [2], "the span includes its last day");
  assert.deepEqual(run("played:never", list), [3, 4]);
  assert.deepEqual(run("-played:never", list), [1, 2]);
  assert.deepEqual(run("played:2026-03", list), [2]);
  assert.deepEqual(run("lastplayed:30d", list), [1]);
  assert.equal(dateSpan("2026-02-31"), null);
  assert.equal(dateSpan("2026-13"), null);
  assert.deepEqual(dateSpan("2026-02-28"), { start: local(2026, 2, 28), end: local(2026, 3, 1) });
  assert.deepEqual(dateSpan("2025-12"), { start: local(2025, 12, 1), end: local(2026, 1, 1) });
  assert.ok(Math.abs(dateSpan("2026").end - dateSpan("2026").start - 365 * DAY) <= DAY / 24 + 1, "a year is a year (give or take a daylight-saving hour)");
});

test("flags", () => {
  const list = [T(1, { rating: 4, times_played: 2 }), T(2), T(3, { last_played: local(2026, 1, 1) })];
  const c = cx({ isFavourite: (id) => id === 2, needs: new Set([3]) });
  assert.deepEqual(run("is:fav", list, c), [2]);
  assert.deepEqual(run("is:favourite", list, c), [2]);
  assert.deepEqual(run("is:needs", list, c), [3]);
  assert.deepEqual(run("is:analysed", list, c), [1, 2]);
  assert.deepEqual(run("is:unrated", list, c), [2, 3]);
  assert.deepEqual(run("is:unplayed", list, c), [2]);
  assert.match(compileQuery("is:cool").diagnostics[0].message, /not one of fav/);
});

test("AND binds tighter than OR; negation applies per term", () => {
  const list = [T(1, { genre: "House", bpm: 124 }), T(2, { genre: "House", bpm: 130 }), T(3, { genre: "Techno", bpm: 140 }), T(4, { genre: "Trance", bpm: 138 })];
  assert.deepEqual(run("genre:house OR genre:techno", list), [1, 2, 3]);
  assert.deepEqual(run("genre:house bpm>125 OR genre:techno", list), [2, 3], "(house and fast) or techno");
  assert.deepEqual(run("genre:house OR genre:techno bpm>125", list), [1, 2, 3]);
  assert.deepEqual(run("-genre:house bpm>=139", list), [3]);
  assert.deepEqual(run("genre:house OR", list), [1, 2], "a dangling OR is ignored");
  assert.deepEqual(run("OR genre:trance", list), [4]);
});

test("a term that cannot be read is skipped and explained", () => {
  const list = [T(1), T(2)];
  const c = compileQuery("bpm:fast genre:house");
  assert.equal(c.empty, false, "the readable term still applies");
  assert.equal(c.diagnostics.length, 1);
  assert.equal(c.diagnostics[0].term, "bpm:fast");
  assert.match(c.diagnostics[0].message, /^bpm: expected a number/);
  assert.deepEqual(run("bpm:fast", list), [1, 2], "a skipped term filters nothing");
  for (const q of ["year:20x0-2012", "rating>=", "added:soon", "time:abc", "size:big"]) {
    const d = compileQuery(q).diagnostics;
    assert.ok(q === "rating>=" ? d.length === 0 : d.length === 1, `${q} -> ${JSON.stringify(d)}`);
  }
  assert.match(compileQuery("added:soon").diagnostics[0].message, /expected a date/);
  assert.match(compileQuery("artist>x").diagnostics[0].message, /takes artist:text/);
});

test("an unknown field is searched as text, with a warning that says how to avoid it", () => {
  const list = [T(1, { title: "Re:mix" }), T(2)];
  const c = compileQuery("re:mix");
  assert.deepEqual(run("re:mix", list), [1]);
  assert.equal(c.diagnostics.length, 1);
  assert.match(c.diagnostics[0].message, /"re" is not a field/);
  assert.match(c.diagnostics[0].message, /quotes/);
  const quoted = compileQuery('"re:mix"');
  assert.deepEqual(quoted.diagnostics, [], "in quotes it is a phrase on purpose");
  assert.deepEqual(run('"re:mix"', list), [1]);
  assert.deepEqual(compileQuery("2:00").diagnostics, [], "a time-like word is just text");
  assert.deepEqual(compileQuery("bmp:").diagnostics, [], "nothing after the colon yet");
});

test("every example in the help compiles cleanly and says something", () => {
  assert.ok(QUERY_HELP.length >= 10);
  for (const row of QUERY_HELP) {
    const c = compileQuery(row.example);
    assert.deepEqual(c.diagnostics, [], row.example);
    assert.equal(c.empty, false, row.example);
    assert.ok(row.syntax && row.meaning, row.example);
  }
});

test("a long query over many tracks is quick", () => {
  const list = Array.from({ length: 20000 }, (_, i) => T(i + 1, { title: `Track ${i}`, bpm: 100 + (i % 60), key: i % 2 ? "Am" : "Cm", artist: `Artist ${i % 50}` }));
  const q = compileQuery("bpm:120-130 key:~8A -artist:7 deep OR track");
  const t0 = performance.now();
  const n = list.filter((t) => q.test(t, cx())).length;
  assert.ok(n > 0);
  assert.ok(performance.now() - t0 < 1500, `took ${performance.now() - t0} ms`);
});
