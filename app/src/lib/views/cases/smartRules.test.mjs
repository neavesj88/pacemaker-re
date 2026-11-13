// Run: node --experimental-strip-types src/lib/views/cases/smartRules.test.mjs
// Tests the pure smart-case logic (state/smartRules.ts): validation, descriptions, durations, and
// the browser twin of the Rust evaluation. The evaluation cases use the same fixture and expected
// orders as the Rust tests in src-tauri/src/commands/cases/smart/rules_tests.rs.
import assert from "node:assert/strict";
import { test } from "node:test";
import { resolveLevel } from "../../state/energyResolve.ts";
import {
  MAX_DEPTH, MAX_LIMIT, MAX_NODES, MS_PER_DAY, KINDS, camelot, dateLabel, defaultRule, describeQuery, describeRule, directionLabels,
  evaluateQuery, formatDuration, kindId, newQuery, parseDuration, referencedCases, ruleMatches, validateQuery,
  NO_CASE, caseNameDrift, defaultCaseId, stampCaseNames, NO_COLOUR,
} from "../../state/smartRules.ts";

const NOW = 1000 * MS_PER_DAY;
const row = (id, extra = {}) => ({
  track_id: id, title: `T${id}`, artist: "A", album: "", genre: "House", bpm: 128, key: "8A", play_time_secs: 200, rating: 3,
  file_size: 1000, format: "MP3", analysed: true, times_played: 0, last_played: -1, year: 2001, date_added: NOW - 10 * MS_PER_DAY,
  label: "Synthetic Label", comments: "synthetic comment", remixer: "Synthetic Remixer", ...extra,
});
const env = (extra = {}) => ({ favourites: new Set([1]), labelOf: () => undefined, energyOf: () => null, cases: new Map([[7, new Set([1])], [8, new Set()]]), now: NOW, ...extra });
const q = (rules, extra = {}) => ({ mode: "all", rules, sort: null, limit: null, ...extra });
const text = (field, op, t) => ({ kind: "text", field, op, text: t });
const grp = (mode, items) => ({ kind: "group", mode, items });

// the same five tracks as lib_data() in rules_tests.rs
function lib() {
  const rows = [
    row(1, { artist: "zed", title: "b", rating: 5, bpm: 140, key: "8A", year: 1999 }),
    row(2, { artist: "Alpha", title: "second", rating: 5, bpm: 0, key: "", year: 2005 }),
    row(3, { artist: "alpha", title: "First", rating: 5, bpm: 120, key: "9A", year: 0 }),
    row(4, { rating: 2, bpm: 130, key: "1B", year: 2010 }),
    row(5, { rating: 4, bpm: 100, key: "8B", year: 2001 }),
  ];
  return { rows, env: env({ favourites: new Set([1, 2, 3]), cases: new Map([[9, new Set([1, 4])]]) }) };
}
const ids = (query, l = lib()) => evaluateQuery(query, l.rows, l.env).ids;

test("durations parse like the box shows them", () => {
  assert.equal(parseDuration("3:30"), 210);
  assert.equal(parseDuration(" 210 "), 210);
  assert.equal(parseDuration("1:02:03"), 3723);
  assert.equal(parseDuration(""), null);
  for (const bad of ["abc", "3:75", "1:2:3:4", "-5", "3.5", "3:"]) assert.ok(Number.isNaN(parseDuration(bad)), bad);
  assert.equal(formatDuration(210), "3:30");
  assert.equal(formatDuration(3723), "1:02:03");
  assert.equal(formatDuration(null), "");
  for (const s of [0, 59, 60, 3599, 3600, 86399]) assert.equal(parseDuration(formatDuration(s)), s, String(s));
});

test("camelot parsing matches the backend's notations", () => {
  assert.deepEqual(camelot("8A"), { n: 8, minor: true });
  assert.deepEqual(camelot("Am"), { n: 8, minor: true });
  assert.deepEqual(camelot("C"), { n: 8, minor: false });
  assert.deepEqual(camelot("F#m"), { n: 11, minor: true });
  assert.deepEqual(camelot("Ab Minor"), { n: 1, minor: true });
  assert.deepEqual(camelot("Bb"), { n: 6, minor: false });
  assert.equal(camelot("13A"), null);
  assert.equal(camelot(""), null);
  assert.equal(camelot("Amajor7"), null);
});

test("every kind in the menu makes a rule of that kind that validates", () => {
  for (const k of KINDS) {
    const r = defaultRule(k.id, 3);
    if (r.kind === "text") r.text = "x";
    if (r.kind === "tags") r.tags = ["x"]; // a new tag rule waits for a tag (checked below)
    assert.equal(kindId(r), k.id);
    assert.equal(validateQuery(q([r])), "", k.id);
  }
  const g = defaultRule("group");
  assert.equal(g.kind, "group");
  assert.equal(validateQuery(q([g])), "");
  assert.equal(validateQuery(newQuery()), "");
});

test("validation says what the backend says", () => {
  assert.match(validateQuery(q([])), /Add at least one rule/);
  assert.equal(validateQuery(q([], { limit: 10 })), "", "no rules is fine with a limit");
  for (const bad of [
    { kind: "rating_min", stars: 0 }, { kind: "plays_min", n: 0 }, { kind: "played_within", days: 0 },
    { kind: "bpm_range", min: null, max: null }, { kind: "bpm_range", min: 140, max: 120 }, { kind: "key", key: "x" },
    text("label", "contains", "  "), { kind: "year_range", min: null, max: null }, { kind: "year_range", min: 0, max: null },
    { kind: "year_range", min: 2010, max: 2000 }, { kind: "added_within", days: 0 }, { kind: "added_before", days: -3 },
    { kind: "duration_range", min_secs: null, max_secs: null }, { kind: "duration_range", min_secs: 300, max_secs: 200 },
    { kind: "duration_range", min_secs: Number.NaN, max_secs: null }, { kind: "plays_range", min: 5, max: 2 },
    { kind: "plays_range", min: -1, max: null }, { kind: "last_played_before", days: 0, include_never: true },
    { kind: "format", value: " " }, { kind: "in_case", case_id: -1, value: true }, grp("all", []), grp("any", [{ kind: "plays_min", n: 0 }]),
  ]) assert.notEqual(validateQuery(q([bad])), "", JSON.stringify(bad));
  assert.equal(validateQuery(q([{ kind: "plays_range", min: 0, max: 0 }, { kind: "year_range", min: 1990, max: 1990 }])), "");
  for (const limit of [0, MAX_LIMIT + 1, 1.5, Number.NaN]) assert.notEqual(validateQuery(q([{ kind: "plays_zero" }], { limit })), "", String(limit));
  assert.equal(validateQuery(q([{ kind: "plays_zero" }], { limit: MAX_LIMIT })), "");
});

test("nesting and size are bounded like the backend", () => {
  const nest = (n) => {
    let r = { kind: "plays_zero" };
    for (let i = 0; i < n; i++) r = grp("all", [r]);
    return r;
  };
  assert.equal(validateQuery(q([nest(MAX_DEPTH - 1)])), "");
  assert.notEqual(validateQuery(q([nest(MAX_DEPTH)])), "");
  const many = Array.from({ length: MAX_NODES }, () => ({ kind: "plays_zero" }));
  assert.equal(validateQuery(q(many)), "");
  assert.notEqual(validateQuery(q([...many, { kind: "plays_zero" }])), "");
  const groups = Array.from({ length: MAX_NODES / 2 }, () => grp("all", [{ kind: "plays_zero" }]));
  assert.equal(validateQuery(q(groups)), "", "100 groups + their 100 rules is exactly the cap");
  assert.notEqual(validateQuery(q([...groups, { kind: "plays_zero" }])), "");
});

test("rules match what they say (new kinds)", () => {
  const t = row(1);
  const m = (r, over = {}, e = env()) => ruleMatches(r, { ...t, ...over }, e);
  assert.ok(m(text("label", "starts_with", "synthetic l")));
  assert.ok(m(text("remixer", "is", " SYNTHETIC remixer ")));
  assert.ok(!m(text("remixer", "is", "remixer")));
  assert.ok(m(text("comments", "contains", "COMMENT")));
  assert.ok(!m(text("album", "contains", "x")), "an empty column contains nothing");
  assert.ok(m(text("title", "is", "étÉ"), { title: "Été" }), "unicode folds");
  assert.ok(m({ kind: "year_range", min: 2001, max: 2001 }));
  assert.ok(!m({ kind: "year_range", min: null, max: 2000 }));
  assert.ok(!m({ kind: "year_range", min: null, max: 2000 }, { year: 0 }), "no year never matches");
  assert.ok(m({ kind: "added_within", days: 10 }));
  assert.ok(!m({ kind: "added_within", days: 9 }));
  assert.ok(m({ kind: "added_before", days: 9 }));
  assert.ok(!m({ kind: "added_before", days: 10 }));
  assert.ok(!m({ kind: "added_before", days: 1 }, { date_added: 0 }), "unknown date never matches");
  assert.ok(m({ kind: "duration_range", min_secs: 200, max_secs: 200 }));
  assert.ok(!m({ kind: "duration_range", min_secs: null, max_secs: 500 }, { play_time_secs: 0 }));
  assert.ok(m({ kind: "plays_range", min: 0, max: 1 }));
  assert.ok(!m({ kind: "plays_range", min: 3, max: null }));
  assert.ok(m({ kind: "format", value: " mp3 " }));
  assert.ok(!m({ kind: "format", value: "FLAC" }));
  const stale = (days, include_never) => ({ kind: "last_played_before", days, include_never });
  assert.ok(m(stale(30, true)));
  assert.ok(!m(stale(30, false)));
  assert.ok(m(stale(39, false), { last_played: NOW - 40 * MS_PER_DAY }));
  assert.ok(!m(stale(40, false), { last_played: NOW - 40 * MS_PER_DAY }), "exactly N days ago is not older");
  assert.ok(m(stale(30, true), { last_played: 0 }));
  assert.ok(m({ kind: "in_case", case_id: 7, value: true }));
  assert.ok(!m({ kind: "in_case", case_id: 8, value: true }));
  assert.ok(m({ kind: "in_case", case_id: 8, value: false }));
});

test("groups combine all, any and none and nest", () => {
  const t = row(1);
  const yes = { kind: "rating_min", stars: 3 };
  const no = { kind: "rating_min", stars: 5 };
  const m = (r, row = t) => ruleMatches(r, row, env());
  assert.ok(m(grp("all", [yes, yes])) && !m(grp("all", [yes, no])));
  assert.ok(m(grp("any", [no, yes])) && !m(grp("any", [no, no])));
  assert.ok(m(grp("none", [no, no])) && !m(grp("none", [no, yes])));
  const tree = grp("all", [
    grp("any", [no, grp("all", [text("genre", "is", "house"), { kind: "bpm_range", min: 120, max: 130 }])]),
    grp("none", [{ kind: "key", key: "9A" }, text("label", "starts_with", "z")]),
  ]);
  assert.ok(m(tree));
  assert.ok(!m(tree, row(1, { key: "9A" })));
  assert.ok(!m(tree, row(1, { bpm: 100 })));
});

test("order, missing values last, limit and the matched count (same expectations as the Rust tests)", () => {
  const by = (field, descending) => q([{ kind: "plays_zero" }], { sort: { field, descending } });
  assert.deepEqual(ids(q([{ kind: "favourite", value: true }])), [3, 2, 1], "default: artist, title, id");
  assert.deepEqual(ids(q([{ kind: "plays_zero" }])), [4, 5, 3, 2, 1]);
  assert.deepEqual(ids(by("bpm", false)), [5, 3, 4, 1, 2]);
  assert.deepEqual(ids(by("bpm", true)), [1, 4, 3, 5, 2]);
  assert.deepEqual(ids(by("year", false)), [1, 5, 2, 4, 3]);
  assert.deepEqual(ids(by("year", true)), [4, 2, 5, 1, 3]);
  assert.deepEqual(ids(by("key", false)), [4, 1, 5, 3, 2]);
  assert.deepEqual(ids(by("key", true)), [3, 5, 1, 4, 2]);
  assert.deepEqual(ids(by("rating", true)), [3, 2, 1, 5, 4]);
  assert.deepEqual(ids(by("rating", false)), [4, 5, 3, 2, 1]);
  assert.deepEqual(ids(by("title", false)), [1, 3, 2, 4, 5]);
  assert.deepEqual(ids(by("album", true)), [4, 5, 3, 2, 1]);

  const top = q([{ kind: "plays_zero" }], { sort: { field: "bpm", descending: true }, limit: 2 });
  const r = evaluateQuery(top, lib().rows, lib().env);
  assert.deepEqual([r.ids, r.matched], [[1, 4], 5]);
  const all = q([], { limit: 3, sort: { field: "year", descending: true } });
  assert.deepEqual(ids(all), [4, 2, 5]);
  assert.equal(evaluateQuery({ ...all, mode: "none" }, lib().rows, lib().env).matched, 5, "an empty rule list is no filter whatever the mode");
});

test("top-level mode and case membership; a deleted case is an error, not match-all", () => {
  const rules = [{ kind: "in_case", case_id: 9, value: true }, { kind: "rating_min", stars: 4 }];
  assert.deepEqual(ids(q(rules)), [1]);
  assert.deepEqual(ids(q(rules, { mode: "any" })), [4, 5, 3, 2, 1]);
  assert.deepEqual(ids(q(rules, { mode: "none" })), []);
  assert.deepEqual(ids(q([{ kind: "in_case", case_id: 9, value: false }])), [5, 3, 2]);
  const l = lib();
  l.env.cases.set(77, null);
  assert.throws(() => evaluateQuery(q([{ kind: "in_case", case_id: 77, value: false }]), l.rows, l.env), /no longer exists/);
  assert.throws(() => evaluateQuery(q([grp("any", [{ kind: "plays_zero" }, { kind: "in_case", case_id: 1234, value: true }])]), l.rows, l.env), /no longer exists/);
  assert.deepEqual([...referencedCases([{ kind: "in_case", case_id: 3, value: true }, grp("any", [{ kind: "in_case", case_id: 4, value: true }])])].sort(), [3, 4]);
});

test("the preview shows the value an order is based on, for columns the table lacks", () => {
  const l = lib();
  const run = (field) => evaluateQuery(q([{ kind: "plays_zero" }], { sort: { field, descending: false }, limit: 3 }), l.rows, l.env);
  assert.deepEqual(run("year").values, ["1999", "2001", "2005"]);
  assert.deepEqual(run("rating").values, ["★★", "★★★★", "★★★★★"]);
  assert.deepEqual(run("last_played").values, ["never", "never", "never"]);
  assert.deepEqual(run("date_added").values, ["1972-09-17", "1972-09-17", "1972-09-17"]);
  assert.deepEqual(run("bpm").values, [], "BPM is a column of the table already");
  assert.equal(dateLabel(1_700_000_000_000), "2023-11-14");
  assert.equal(dateLabel(1_709_251_199_999), "2024-02-29");
  assert.equal(dateLabel(0), "–");
});

test("descriptions read as sentences", () => {
  const name = (id) => (id === 3 ? "Synthetic case" : `missing case ${id}`);
  assert.equal(describeRule({ kind: "rating_min", stars: 4 }, name), "rated 4+ stars");
  assert.equal(describeRule({ kind: "in_case", case_id: 3, value: false }, name), "not in case “Synthetic case”");
  assert.equal(describeRule({ kind: "in_case", case_id: 9, value: true }, name), "in case “missing case 9”");
  assert.equal(describeRule({ kind: "duration_range", min_secs: 180, max_secs: null }, name), "length at least 3:00");
  assert.equal(describeRule({ kind: "year_range", min: 1990, max: 1999 }, name), "year between 1990 and 1999");
  assert.equal(describeRule({ kind: "played_within", days: 1 }, name), "played in the last 1 day");
  assert.equal(describeRule(text("artist", "starts_with", " x "), name), "artist starts with “x”");
  const query = q([{ kind: "rating_min", stars: 4 }, grp("none", [{ kind: "plays_zero" }, { kind: "favourite", value: true }])], {
    sort: { field: "bpm", descending: true }, limit: 50, mode: "any",
  });
  assert.equal(
    describeQuery(query, name),
    "Tracks that match any of: rated 4+ stars; none of (never played; in Favourites). Ordered by bpm, high to low. Only the first 50 tracks.",
  );
  assert.equal(describeQuery(q([], { limit: 1 }), name), "Every track. Only the first 1 track.");
  assert.deepEqual(directionLabels("date_added"), ["oldest first", "newest first"]);
  assert.deepEqual(directionLabels("title"), ["A to Z", "Z to A"]);
});

test("a case rule remembers the case name and says when the id now means another case", () => {
  const infos = [{ id: 3, name: "Synthetic case" }, { id: 4, name: "Other synthetic case" }];
  const rule = (extra = {}) => ({ kind: "in_case", case_id: 3, value: true, ...extra });
  assert.equal(caseNameDrift(rule(), infos), null, "older rule files carry no name");
  assert.equal(caseNameDrift(rule({ case_name: "Synthetic case" }), infos), null);
  assert.deepEqual(caseNameDrift(rule({ case_name: "Renamed since" }), infos), { saved: "Renamed since", now: "Synthetic case" });
  assert.equal(caseNameDrift(rule({ case_id: 99, case_name: "Gone" }), infos), null, "a missing case has its own message");
  assert.equal(caseNameDrift({ kind: "plays_zero" }, infos), null);
  // stamping fills only the rules that have no name, groups included, and skips unknown cases
  const rules = [rule(), rule({ case_id: 4, case_name: "Kept" }), rule({ case_id: 99 }), grp("any", [rule({ case_id: 4 })])];
  stampCaseNames(rules, infos);
  assert.equal(rules[0].case_name, "Synthetic case");
  assert.equal(rules[1].case_name, "Kept");
  assert.equal(rules[2].case_name, undefined);
  assert.equal(rules[3].items[0].case_name, "Other synthetic case");
  // the name never changes what matches or what validates
  const l = lib();
  assert.ok(ruleMatches(rule({ case_id: 9, case_name: "Anything" }), l.rows[0], l.env));
  assert.ok(!ruleMatches(rule({ case_id: 9, case_name: "Anything" }), l.rows[1], l.env));
});

test("a case rule with no cases to choose from starts on no case and is refused until one is picked", () => {
  assert.equal(defaultCaseId([]), NO_CASE);
  assert.equal(defaultRule("in_case").case_id, NO_CASE);
  assert.match(validateQuery(q([defaultRule("in_case", defaultCaseId([]))])), /needs a case/);
  assert.equal(validateQuery(q([defaultRule("in_case", defaultCaseId([{ id: 1, name: "Synthetic case" }]))])), "");
  assert.equal(defaultCaseId([{ id: 2, name: "★ Synthetic smart" }, { id: 5, name: "Synthetic case" }]), 5);
});

// ---- label rules: the same fixture and answers as `label_rules` in rules_tests.rs ----------------

// 1: tags Peak + Warm up, red, my energy 4, estimate 8.2. 2: tag peak, nothing else, estimate 6.5.
// 3: nothing at all. 4: only my energy 9.
function labelLib(source) {
  const labels = new Map([
    [1, { tags: ["Peak", "Warm up"], colour: "red", energy: 4 }],
    [2, { tags: ["peak"], colour: null, energy: null }],
    [4, { tags: [], colour: null, energy: 9 }],
  ]);
  const auto = new Map([[1, 8.2], [2, 6.5]]);
  const rows = [row(1), row(2), row(3), row(4)];
  return { rows, env: env({ labelOf: (id) => labels.get(id), energyOf: (id) => resolveLevel(labels.get(id)?.energy ?? null, auto.get(id) ?? null, source) }) };
}
const found = (rule, source = "mine_else_auto") => {
  const l = labelLib(source);
  return evaluateQuery(q([rule]), l.rows, l.env).ids.sort((a, b) => a - b);
};
const tagsRule = (how, tags) => ({ kind: "tags", how, tags });
const colourRule = (colours, value) => ({ kind: "colour", colours, value });
const energyRule = (min, max) => ({ kind: "energy_range", min, max });

test("tag rules ignore case and cover any, all and none", () => {
  assert.deepEqual(found(tagsRule("any", ["PEAK"])), [1, 2]);
  assert.deepEqual(found(tagsRule("any", ["peak", "warm  up"])), [1, 2], "whitespace is normalised like a typed tag");
  assert.deepEqual(found(tagsRule("all", ["peak", "warm up"])), [1]);
  assert.deepEqual(found(tagsRule("all", ["peak"])), [1, 2]);
  assert.deepEqual(found(tagsRule("none", ["warm up"])), [2, 3, 4], "a track with no tags has none of them");
  assert.deepEqual(found(tagsRule("none", ["peak", "warm up"])), [3, 4]);
  assert.deepEqual(found(tagsRule("any", ["absent"])), []);
});

test("colour rules: is, is not, and none for no colour", () => {
  assert.deepEqual(found(colourRule(["red"], true)), [1]);
  assert.deepEqual(found(colourRule(["red"], false)), [2, 3, 4], "is not red includes uncoloured tracks");
  assert.deepEqual(found(colourRule([NO_COLOUR], true)), [2, 3, 4]);
  assert.deepEqual(found(colourRule(["red", NO_COLOUR], true)), [1, 2, 3, 4]);
  assert.deepEqual(found(colourRule([NO_COLOUR], false)), [1]);
  assert.deepEqual(found(colourRule(["blue"], true)), []);
});

test("energy rules follow the energy source", () => {
  assert.deepEqual(found(energyRule(7, null)), [2, 4]);
  assert.deepEqual(found(energyRule(null, 4)), [1]);
  assert.deepEqual(found(energyRule(4, 7)), [1, 2]);
  assert.deepEqual(found(energyRule(1, 10), "mine"), [1, 4]);
  assert.deepEqual(found(energyRule(1, 10), "auto"), [1, 2]);
  assert.deepEqual(found(energyRule(8, null), "auto"), [1]);
  assert.deepEqual(found({ kind: "group", mode: "none", items: [tagsRule("any", ["peak"])] }), [3, 4]);
});

test("the label rules are validated like the Rust ones, listed in the menu and described in words", () => {
  for (const ok of [tagsRule("any", ["x"]), colourRule(["red", NO_COLOUR], true), energyRule(1, 10), energyRule(null, 3)]) assert.equal(validateQuery(q([ok])), "", JSON.stringify(ok));
  const refused = [
    [tagsRule("any", []), /at least one tag/],
    [tagsRule("all", ["a,b"]), /comma/],
    [tagsRule("none", [" "]), /empty/],
    [colourRule([], true), /at least one colour/],
    [colourRule(["mauve"], false), /not one of the colour labels/],
    [energyRule(null, null), /An energy rule needs a minimum/],
    [energyRule(0, null), /Energy values must be 1 or more/],
    [energyRule(7, 3), /minimum is above the maximum/],
    [energyRule(null, 11), /10 or less/],
    [energyRule(2.5, null), /whole numbers/],
  ];
  for (const [rule, why] of refused) assert.match(validateQuery(q([rule])), why, JSON.stringify(rule));

  for (const id of ["tags", "colour", "energy_range"]) {
    assert.ok(KINDS.some((k) => k.id === id && k.group === "Labels"), id);
    const r = defaultRule(id);
    assert.equal(kindId(r), id);
  }
  assert.equal(validateQuery(q([defaultRule("colour")])), "", "a new colour rule is ready to use");
  assert.match(validateQuery(q([defaultRule("tags")])), /at least one tag/, "a new tag rule waits for a tag");
  assert.equal(validateQuery(q([defaultRule("energy_range")])), "");
  assert.equal(describeRule(tagsRule("all", ["Peak", "warm up"]), () => ""), "has all of the tags “Peak”, “warm up”");
  assert.equal(describeRule(tagsRule("none", ["x"]), () => ""), "has none of the tags “x”");
  assert.equal(describeRule(colourRule(["red", NO_COLOUR], false), () => ""), "colour label is not red or no colour");
  assert.equal(describeRule(energyRule(7, null), () => ""), "energy at least 7");
  assert.equal(describeRule(energyRule(3, 5), () => ""), "energy between 3 and 5");
});

test("a query round-trips through JSON in the shape the Rust rules read", () => {
  const rules = [tagsRule("any", ["a"]), colourRule(["red"], true), energyRule(3, null)];
  assert.deepEqual(JSON.parse(JSON.stringify(rules)), rules);
  assert.deepEqual(Object.keys(rules[0]).sort(), ["how", "kind", "tags"]);
  assert.deepEqual(Object.keys(rules[2]).sort(), ["kind", "max", "min"]);
});
