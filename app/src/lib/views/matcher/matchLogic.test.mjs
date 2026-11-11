// Run: node --experimental-strip-types --test src/lib/views/matcher/matchLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  buildIndex, caseTrackIds, cleanOptions, countStatuses, DEFAULT_OPTIONS, fold, matchItem, missingAsCsv, missingAsText, missingList,
  ownedList, parseTitle, prepare, resolve, scorePair, searchTracks, similarity, splitArtists, TITLE_ONLY_CAP, versionKey,
} from "./matchLogic.ts";

const O = DEFAULT_OPTIONS;
const track = (track_id, artist, title, location = `/pmdb_tracks/0/0/f${track_id}.mp3`) => ({ track_id, artist, title, location });
const score = (ra, rt, la, lt, mode = "strict") => scorePair(prepare(ra, rt), prepare(la, lt), mode).score;

test("fold removes accents, case, apostrophes and punctuation, and reads & as and", () => {
  assert.equal(fold("  Béla  Fleck & The  Flecktones! "), "bela fleck and the flecktones");
  assert.equal(fold("Don’t Stop (Me Now)"), "dont stop me now");
  assert.equal(fold("Sigur Rós – Ágætis"), "sigur ros agaetis");
  assert.equal(fold("Straße Ærø"), "strasse aero");
  assert.equal(fold("ЗВУК — Тест"), "звук тест");
  assert.equal(fold(""), "");
});

test("similarity: equal is 1, typos stay high, word order is ignored, numbers and single letters must agree", () => {
  assert.equal(similarity("demo tune", "demo tune"), 1);
  assert.ok(similarity("demo tune", "demo tunne") > 0.85);
  assert.equal(similarity("tune demo", "demo tune"), 1);
  assert.ok(similarity("demo track 01", "demo track 02") <= 0.55);
  assert.equal(similarity("demo track 1", "demo track 01"), 1, "leading zeros are not a difference");
  assert.ok(similarity("example artist a", "example artist b") <= 0.55);
  assert.ok(similarity("love is a drug", "love is drug") > 0.8, "a missing article is not a letter mismatch");
  assert.equal(similarity("", "x"), 0);
  assert.equal(similarity("", ""), 0);
});

test("artists split on the usual joiners, drop 'the', and keep the same name once", () => {
  assert.deepEqual(splitArtists("Act One feat. Act Two"), ["act one", "act two"]);
  assert.deepEqual(splitArtists("Act One, Act Two & Act Three"), ["act one", "act two", "act three"]);
  assert.deepEqual(splitArtists("The Sample Band"), ["sample band"]);
  assert.deepEqual(splitArtists("Act One x Act One"), ["act one"]);
  assert.deepEqual(splitArtists("Act One;Act Two/Act Three"), ["act one", "act two", "act three"]);
  assert.deepEqual(splitArtists(""), []);
});

test("titles lose guests, mix names and video tags, and keep unlabelled brackets", () => {
  assert.deepEqual(parseTitle("Demo Tune (feat. Guest Act)"), { core: "demo tune", feat: ["guest act"], versionRaw: "", version: "" });
  assert.deepEqual(parseTitle("Demo Tune ft. Guest Act"), { core: "demo tune", feat: ["guest act"], versionRaw: "", version: "" });
  const x = parseTitle("Demo Tune (Extended Mix) [Some Label]");
  assert.deepEqual([x.core, x.versionRaw, x.version], ["demo tune", "Extended Mix", "extended"]);
  assert.equal(parseTitle("Demo Tune (Original Mix)").version, "");
  assert.equal(parseTitle("Demo Tune - 2011 Remastered Version").version, "");
  assert.equal(parseTitle("Demo Tune - 2011 Remastered Version").core, "demo tune");
  assert.equal(parseTitle("Demo Tune (Official Video)").core, "demo tune");
  assert.equal(parseTitle("Demo Tune (Official Video)").version, "");
  assert.equal(parseTitle("Demo Tune - Radio Edit").version, "radio edit");
  assert.equal(parseTitle("Demo Tune (Part 2)").core, "demo tune part 2");
  assert.equal(parseTitle("Demo - Tune").core, "demo tune", "a dash that is not a mix name stays part of the title");
  assert.equal(parseTitle("(Intro)").core, "intro", "a title that is all brackets is kept");
  assert.equal(parseTitle("(Radio Edit)").core, "radio edit");
  assert.equal(versionKey(["Club Mix", "2011 Remaster"]), "club");
});

test("scores: exact is 100; an identical re-spelling is 100; case, accents and punctuation do not matter", () => {
  assert.equal(score("Sample Artist", "Demo Tune", "Sample Artist", "Demo Tune"), 100);
  assert.equal(score("SAMPLE ARTIST", "demo tune!", "Sämple Artist", "Demo Tune"), 100);
  assert.equal(score("The Sample Band", "Demo Tune", "Sample Band", "Demo Tune"), 100);
  assert.equal(score("Sample Artist & Other Act", "Demo Tune", "Other Act, Sample Artist", "Demo Tune"), 100);
});

test("scores: a guest credited in the title equals the same guest in the artist field", () => {
  assert.ok(score("Sample Artist feat. Guest Act", "Demo Tune", "Sample Artist", "Demo Tune (feat. Guest Act)") >= 95);
  assert.ok(score("Sample Artist", "Demo Tune (feat. Guest Act)", "Sample Artist feat. Guest Act", "Demo Tune") >= 95);
});

test("scores: a typo in the title or artist stays an accepted match, a wrong artist or title does not", () => {
  assert.ok(score("Sample Artist", "Demo Tunne", "Sample Artist", "Demo Tune") >= O.acceptAt);
  assert.ok(score("Sampel Artist", "Demo Tune", "Sample Artist", "Demo Tune") >= O.acceptAt);
  const coverVersion = score("Other Band", "Demo Tune", "Sample Artist", "Demo Tune");
  assert.ok(coverVersion < O.acceptAt && coverVersion >= O.reviewAt, `same title, other artist asks the user (${coverVersion})`);
  assert.ok(score("Sample Artist", "Completely Different", "Sample Artist", "Demo Tune") < O.reviewAt);
  assert.ok(score("Sample Artist", "Demo Track 01", "Sample Artist", "Demo Track 02") < O.reviewAt, "numbered titles are different songs");
});

test("scores: mix names. Original mix equals none; one-sided and differing mix names cost score by mode", () => {
  assert.equal(score("A Act", "Tune (Original Mix)", "A Act", "Tune"), 100);
  assert.equal(score("A Act", "Tune", "A Act", "Tune (Extended Mix)"), 90);
  assert.equal(score("A Act", "Tune", "A Act", "Tune (Extended Mix)", "lenient"), 97);
  assert.equal(score("A Act", "Tune", "A Act", "Tune (Extended Mix)", "ignore"), 100);
  const differ = score("A Act", "Tune (Radio Edit)", "A Act", "Tune (Extended Mix)");
  assert.ok(differ >= 75 && differ < 85, `strict, two different mixes: ${differ}`);
  assert.equal(score("A Act", "Tune (Extended Mix)", "A Act", "Tune - Extended Mix"), 100);
  assert.equal(score("A Act", "Tune (Club Mix)", "A Act", "Tune (Club Version)"), 100);
});

test("scores: with no artist on either side the score never passes the title-only cap", () => {
  assert.equal(score("", "Demo Tune", "Sample Artist", "Demo Tune"), TITLE_ONLY_CAP);
  assert.equal(score("Sample Artist", "Demo Tune", "", "Demo Tune"), TITLE_ONLY_CAP);
  assert.ok(TITLE_ONLY_CAP < O.acceptAt, "so a title alone is asked about with the default settings");
});

test("matchItem: statuses against a small library", () => {
  const lib = [
    track(1, "Sample Artist", "Demo Tune"),
    track(2, "Sample Artist", "Demo Tune (Extended Mix)"),
    track(3, "Other Act", "Second Tune"),
    track(4, "Third Act", "Third Tune"),
    track(5, "Third Act", "Third Tune"),
    track(6, "Fourth Act", "Quiet Song"),
  ];
  const ix = buildIndex(lib);
  const row = (a, t, opts = O) => matchItem(ix, { id: 0, artist: a, title: t }, opts);

  const exact = row("Sample Artist", "Demo Tune");
  assert.deepEqual([exact.status, exact.candidates[0].track_id, exact.candidates[0].score], ["owned", 1, 100]);
  assert.equal(exact.candidates[1].track_id, 2, "the extended mix is the second candidate");
  assert.match(exact.candidates[1].note, /mix differs/);

  const typo = row("Other Act", "Secnd Tune");
  assert.deepEqual([typo.status, typo.candidates[0].track_id], ["owned", 3]);

  const dup = row("Third Act", "Third Tune");
  assert.equal(dup.status, "review");
  assert.equal(dup.reason, "two library tracks match about equally");
  assert.equal(row("Third Act", "Third Tune", { ...O, margin: 0 }).status, "owned", "margin 0 turns the ambiguity check off");

  const titleOnly = row("", "Quiet Song");
  assert.deepEqual([titleOnly.status, titleOnly.candidates[0].track_id], ["review", 6]);
  assert.match(titleOnly.reason, /no artist/);

  const none = row("Nobody Known", "Not In The Library");
  assert.deepEqual([none.status, none.candidates.length], ["missing", 0]);

  const lowBar = row("Other Band", "Second Tune", { ...O, acceptAt: 70, reviewAt: 50 });
  assert.equal(lowBar.status, "owned", "thresholds are settings");
});

test("a track with no artist tag is also found by the artist and title in its file name", () => {
  const ix = buildIndex([track(1, "", "Untitled", "/pmdb_tracks/1/2/Sample Artist - Demo Tune.mp3"), track(2, "", "Plain", "/pmdb_tracks/1/2/plain.mp3")]);
  const m = matchItem(ix, { id: 0, artist: "Sample Artist", title: "Demo Tune" }, O);
  assert.deepEqual([m.status, m.candidates[0].track_id, m.candidates[0].viaFile], ["owned", 1, true]);
  assert.match(m.candidates[0].note, /file name/);
  assert.equal(ix.entries.length, 3);
});

test("a track listed twice in the candidates (tags and file name) appears once", () => {
  const ix = buildIndex([track(1, "", "Demo Tune", "/x/Sample Artist - Demo Tune.mp3")]);
  const m = matchItem(ix, { id: 0, artist: "Sample Artist", title: "Demo Tune" }, O);
  assert.equal(m.candidates.filter((c) => c.track_id === 1).length, 1);
});

test("decisions: resolve, counts, owned list, case ids and missing list come from the same rows", () => {
  const lib = [track(1, "Act A", "Tune A"), track(2, "Act B", "Tune B"), track(3, "Act C", "Tune C"), track(4, "Act B", "Tune B Reprise Of Something")];
  const ix = buildIndex(lib);
  const items = [
    { id: 0, line: 1, raw: "Act A - Tune A", artist: "Act A", title: "Tune A", repeats: 0 },
    { id: 1, line: 2, raw: "Act B - Tune B", artist: "Act B", title: "Tune B", repeats: 0 },
    { id: 2, line: 3, raw: "Act A - Tune A (Club Mix)", artist: "Act A", title: "Tune A Elsewhere", repeats: 0 },
    { id: 3, line: 4, raw: "Nobody - Nothing", artist: "Nobody", title: "Nothing At All", repeats: 0 },
    { id: 4, line: 5, raw: "Act C", artist: "", title: "Tune C", repeats: 0 },
  ];
  const rows = items.map((i) => matchItem(ix, i, O));
  assert.deepEqual(rows.map((r) => r.status), ["owned", "owned", "missing", "missing", "review"]);
  assert.deepEqual(countStatuses(rows, new Map()), { owned: 2, review: 1, missing: 2 });

  // the user confirms row 4, rejects row 1 and picks track 3 for row 3
  const choices = new Map([
    [4, { kind: "track", trackId: 3 }],
    [0, { kind: "missing" }],
    [2, { kind: "track", trackId: 3 }],
  ]);
  assert.deepEqual(resolve(rows[4], choices.get(4)), { status: "owned", trackId: 3, how: "confirmed" });
  assert.deepEqual(resolve(rows[2], choices.get(2)), { status: "owned", trackId: 3, how: "picked" });
  assert.deepEqual(resolve(rows[0], choices.get(0)), { status: "missing", trackId: null, how: "rejected" });
  assert.deepEqual(resolve(rows[1], undefined), { status: "owned", trackId: 2, how: "auto" });
  assert.deepEqual(countStatuses(rows, choices), { owned: 3, review: 0, missing: 2 });

  const owned = ownedList(items, rows, choices);
  assert.deepEqual(owned.map((o) => [o.item.line, o.trackId]), [[2, 2], [3, 3], [5, 3]]);
  assert.deepEqual(caseTrackIds(owned), [2, 3], "request order, a track asked for twice once");
  assert.deepEqual(missingList(items, rows, choices).map((m) => m.line), [1, 4]);
  assert.deepEqual(missingList(items, rows, new Map(), true).map((m) => m.line), [3, 4, 5], "undecided lines can be included");
  assert.equal(owned.length + missingList(items, rows, choices).length, items.length, "every line is in exactly one list when none is undecided");
});

test("choices can be a plain object as well as a Map", () => {
  const ix = buildIndex([track(1, "Act A", "Tune A")]);
  const items = [{ id: 0, line: 1, raw: "x", artist: "Act A", title: "Tune A", repeats: 0 }];
  const rows = items.map((i) => matchItem(ix, i, O));
  assert.deepEqual(countStatuses(rows, { 0: { kind: "missing" } }), { owned: 0, review: 0, missing: 1 });
  assert.equal(ownedList(items, rows, {}).length, 1);
});

test("missing list text and CSV", () => {
  const m = [
    { id: 0, line: 3, raw: "Act A - Tune, A", artist: "Act A", title: "Tune, A", repeats: 0 },
    { id: 1, line: 9, raw: "Only Title", artist: "", title: 'Say "Hi"', repeats: 0 },
  ];
  assert.equal(missingAsText(m), 'Act A - Tune, A\nSay "Hi"\n');
  assert.equal(missingAsText([]), "");
  assert.equal(missingAsCsv(m), 'Artist,Title,Line,As written\r\nAct A,"Tune, A",3,"Act A - Tune, A"\r\n,"Say ""Hi""",9,Only Title\r\n');
});

test("searchTracks needs every typed word, ignoring case and accents, and stops at the limit", () => {
  const lib = [track(1, "Béla Act", "Demo Tune"), track(2, "Other", "Demo Other"), ...Array.from({ length: 10 }, (_, i) => track(10 + i, "Many", "Demo"))];
  assert.deepEqual(searchTracks(lib, "bela demo").map((t) => t.track_id), [1]);
  assert.deepEqual(searchTracks(lib, "  ").map((t) => t.track_id), []);
  assert.equal(searchTracks(lib, "demo", 4).length, 4);
});

test("options are clamped and always consistent", () => {
  assert.deepEqual(cleanOptions(undefined), DEFAULT_OPTIONS);
  assert.deepEqual(cleanOptions({ acceptAt: 500, reviewAt: -3, margin: "7", versionMode: "lenient", maxCandidates: 99 }), { acceptAt: 100, reviewAt: 1, margin: 7, versionMode: "lenient", maxCandidates: 20 });
  assert.equal(cleanOptions({ acceptAt: 70, reviewAt: 90 }).reviewAt, 70, "review never above accept");
  assert.equal(cleanOptions({ versionMode: "wild" }).versionMode, "strict");
  assert.equal(cleanOptions({ acceptAt: "x" }).acceptAt, 85);
  assert.equal(cleanOptions(null).margin, 3);
});

// ---- the indexed lookup must find what a full scan finds ----------------------------------

function rng(seed) {
  let s = seed >>> 0;
  return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 2 ** 32);
}
const WORDS = "amber bridge candle dawn ember field glass harbor island jungle kettle lantern meadow nectar orbit prairie quartz river signal timber umbra velvet willow xenon yonder zephyr arc brook cinder dusk echo flint grove haze ivory jade knoll lumen moss nova opal pine quill ridge sable tide urban vista wren".split(" ");

test("the indexed lookup agrees with scoring every track, on a generated library with typos", () => {
  const r = rng(7);
  const pick = () => WORDS[Math.floor(r() * WORDS.length)];
  const lib = Array.from({ length: 1500 }, (_, i) => track(i + 1, `${pick()} ${pick()}`, `${pick()} ${pick()}${r() < 0.4 ? " " + pick() : ""}${r() < 0.2 ? " (Extended Mix)" : ""}`));
  const ix = buildIndex(lib);
  const typo = (s) => {
    if (s.length < 4) return s;
    const i = 1 + Math.floor(r() * (s.length - 2));
    return r() < 0.5 ? s.slice(0, i) + s.slice(i + 1) : s.slice(0, i) + s[i + 1] + s[i] + s.slice(i + 2);
  };
  let disagreements = 0;
  for (let n = 0; n < 150; n++) {
    const t = lib[Math.floor(r() * lib.length)];
    const req = { id: n, artist: r() < 0.5 ? typo(t.artist) : t.artist, title: r() < 0.6 ? typo(t.title) : t.title };
    const fast = matchItem(ix, req, O);
    const full = matchItem(ix, req, O, true);
    // compare the best score and status; ties between different tracks may order differently only by id
    if (fast.status !== full.status || fast.candidates[0]?.score !== full.candidates[0]?.score) disagreements++;
  }
  assert.equal(disagreements, 0, "the index never hides the best candidate");
});

test("a 20,000 track library and 300 requests match in a few seconds", () => {
  const r = rng(11);
  const pick2 = () => String.fromCharCode(97 + Math.floor(r() * 26)) + String.fromCharCode(97 + Math.floor(r() * 26));
  const pick = () => WORDS[Math.floor(r() * WORDS.length)] + (r() < 0.5 ? pick2() : "");
  const lib = Array.from({ length: 20000 }, (_, i) => track(i + 1, `${pick()} ${pick()}`, `${pick()} ${pick()} ${pick()}`));
  const t0 = performance.now();
  const ix = buildIndex(lib);
  const tIndex = performance.now() - t0;
  const t1 = performance.now();
  let owned = 0;
  for (let n = 0; n < 300; n++) {
    const t = lib[Math.floor(r() * lib.length)];
    if (matchItem(ix, { id: n, artist: t.artist, title: t.title }, O).status === "owned") owned++;
  }
  const tMatch = performance.now() - t1;
  console.log(`index ${tIndex.toFixed(0)} ms, 300 requests ${tMatch.toFixed(0)} ms`);
  assert.ok(owned >= 295, `exact requests are owned (${owned}/300)`);
  assert.ok(tIndex + tMatch < 15000);
});
