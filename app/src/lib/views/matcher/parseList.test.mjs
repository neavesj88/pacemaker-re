// Run: node --experimental-strip-types --test src/lib/views/matcher/parseList.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { MAX_ITEMS, parseCsv, parseRequestList, splitArtistTitle } from "./parseList.ts";

const pairs = (p) => p.items.map((i) => [i.artist, i.title]);

test("plain lines split at the first separator; the dash kinds and a tab all count", () => {
  const p = parseRequestList("Sample Artist - Demo Tune\nOther Act – Second Tune\nThird Act — Third Tune\nFourth Act -- Fourth Tune\nFifth Act\tFifth Tune\n");
  assert.equal(p.format, "lines");
  assert.deepEqual(pairs(p), [
    ["Sample Artist", "Demo Tune"], ["Other Act", "Second Tune"], ["Third Act", "Third Tune"], ["Fourth Act", "Fourth Tune"], ["Fifth Act", "Fifth Tune"],
  ]);
  assert.deepEqual(p.items.map((i) => i.line), [1, 2, 3, 4, 5]);
});

test("only the first separator splits; hyphens inside words do not", () => {
  assert.deepEqual(splitArtistTitle("Some Act - Tune - Extended Mix"), { artist: "Some Act", title: "Tune - Extended Mix" });
  assert.deepEqual(splitArtistTitle("Some-Act - Re-Tune"), { artist: "Some-Act", title: "Re-Tune" });
  assert.deepEqual(splitArtistTitle("Just A Title"), { artist: "", title: "Just A Title" });
  assert.deepEqual(splitArtistTitle('Some Act - "Quoted Tune"'), { artist: "Some Act", title: "Quoted Tune" });
  assert.deepEqual(splitArtistTitle("Tune Name - Some Act", "title-artist"), { artist: "Some Act", title: "Tune Name" });
});

test("list numbers, bullets and timestamps in front of a line are dropped; a number that is the name is kept", () => {
  const p = parseRequestList("1. A One - T One\n02) A Two - T Two\n- A Three - T Three\n* A Four - T Four\n03:45 A Five - T Five\n1:02:03 A Six - T Six\n2 Unlimited - No Limit Demo\n");
  assert.deepEqual(pairs(p), [
    ["A One", "T One"], ["A Two", "T Two"], ["A Three", "T Three"], ["A Four", "T Four"], ["A Five", "T Five"], ["A Six", "T Six"], ["2 Unlimited", "No Limit Demo"],
  ]);
});

test("blank lines vanish; comments, links and over-long lines are listed as skipped with their line number", () => {
  const long = "x".repeat(600);
  const p = parseRequestList(`# my list\n\nA - B\nhttps://example.invalid/track/123\n${long}\n// note\nspotify:track:abc\n`);
  assert.deepEqual(pairs(p), [["A", "B"]]);
  assert.deepEqual(p.skipped.map((s) => [s.line, s.reason.split(":")[0].split(" ")[0]]), [[1, "comment"], [4, "a"], [5, "longer"], [6, "comment"], [7, "a"]]);
});

test("a trailing link on a line is cut off and the rest kept", () => {
  assert.deepEqual(pairs(parseRequestList("A - B https://example.invalid/x")), [["A", "B"]]);
});

test("a repeated request is merged into the first and counted, whatever the case or punctuation", () => {
  const p = parseRequestList("Some Act - Tune\nsome act - TUNE!\nSome Act - Other\nSome Act - Tune\n");
  assert.equal(p.items.length, 2);
  assert.equal(p.merged, 2);
  assert.deepEqual(p.items.map((i) => [i.title, i.repeats, i.line]), [["Tune", 2, 1], ["Other", 0, 3]]);
  assert.deepEqual(p.items.map((i) => i.id), [0, 1]);
});

test("a CSV with an export-style header uses its title, artist and mix columns", () => {
  const csv = '\uFEFF"Track URI","Track Name","Album Name","Artist Name(s)","Mix"\r\n"u:1","Tune, Part 1","Demo Album","Act One;Act Two","Extended Mix"\r\n"u:2","Say ""Hi""","Demo Album","Act Three",""\r\n,,,,\r\n';
  const p = parseRequestList(csv);
  assert.equal(p.format, "csv");
  assert.deepEqual(p.columns, { title: "track name", artist: "artist name(s)", mix: "mix" });
  assert.deepEqual(pairs(p), [["Act One;Act Two", "Tune, Part 1 (Extended Mix)"], ["Act Three", 'Say "Hi"']]);
  assert.deepEqual(p.items.map((i) => i.line), [2, 3]);
});

test("a mix column is not added twice when the title already says it", () => {
  const p = parseRequestList("Title,Artist,Version\nTune (Radio Edit),Act,radio edit\n");
  assert.deepEqual(pairs(p), [["Act", "Tune (Radio Edit)"]]);
});

test("semicolon and tab separated files with a header work; a header with a title column only gives titles", () => {
  assert.deepEqual(pairs(parseRequestList("Artist;Title\nAct;Tune\n")), [["Act", "Tune"]]);
  assert.deepEqual(pairs(parseRequestList("Artist\tSong\nAct\tTune\n")), [["Act", "Tune"]]);
  assert.deepEqual(pairs(parseRequestList("Title\nOnly A Title\n")), [["", "Only A Title"]]);
});

test("a CSV row without a title is skipped, with its row number", () => {
  const p = parseRequestList("Title,Artist\n,Act\nTune,Act\n");
  assert.deepEqual(pairs(p), [["Act", "Tune"]]);
  assert.deepEqual(p.skipped, [{ line: 2, raw: "Act", reason: "no title" }]);
});

test("a first line that merely contains commas is not a header", () => {
  const p = parseRequestList("Act, Other Act - Tune\nAct - Tune Two\n");
  assert.equal(p.format, "lines");
  assert.deepEqual(pairs(p), [["Act, Other Act", "Tune"], ["Act", "Tune Two"]]);
});

test("M3U: EXTINF text first, else the file name; other comments and links are handled", () => {
  const m3u = "#EXTM3U\n#EXTINF:215,Act One - Tune One\nC:/Music/a.mp3\n#PLAYLIST:x\n/music/Act Two - Tune Two.flac\nhttps://example.invalid/stream\n#EXTINF:100,\nC:\x5cMusic\x5cAct Three - Tune Three.wav\n";
  const p = parseRequestList(m3u);
  assert.equal(p.format, "m3u");
  assert.deepEqual(pairs(p), [["Act One", "Tune One"], ["Act Two", "Tune Two"], ["Act Three", "Tune Three"]]);
  assert.equal(p.skipped.length, 1);
});

test("parseCsv handles quotes, doubled quotes, line breaks inside quotes and a last row with no newline", () => {
  assert.deepEqual(parseCsv('a,"b ""c"" d","e\nf"\r\ng,h,', ","), [["a", 'b "c" d', "e\nf"], ["g", "h", ""]]);
  assert.deepEqual(parseCsv("", ","), []);
  assert.deepEqual(parseCsv("\n\n", ","), []);
});

test("empty and whitespace-only text gives an empty list, not an error", () => {
  for (const t of ["", "   \n\n  "]) {
    const p = parseRequestList(t);
    assert.deepEqual([p.items.length, p.skipped.length, p.error], [0, 0, null]);
  }
});

test("a list over the limit is refused with a message, not cut", () => {
  const text = Array.from({ length: MAX_ITEMS + 1 }, (_, i) => `Act ${i} - Tune ${i}`).join("\n");
  const p = parseRequestList(text);
  assert.equal(p.items.length, 0);
  assert.match(p.error, /more than 10,000/);
  assert.equal(parseRequestList(text.split("\n").slice(0, MAX_ITEMS).join("\n")).items.length, MAX_ITEMS);
});

test("title-artist order applies to plain lines and playlists but a CSV keeps its columns", () => {
  assert.deepEqual(pairs(parseRequestList("Tune - Act\n", { lineOrder: "title-artist" })), [["Act", "Tune"]]);
  assert.deepEqual(pairs(parseRequestList("Title,Artist\nTune,Act\n", { lineOrder: "title-artist" })), [["Act", "Tune"]]);
});
