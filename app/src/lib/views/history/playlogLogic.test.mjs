// Run: node --experimental-strip-types --test src/lib/views/history/playlogLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { DAY_MS, dayStart, fmtBytes, groupByDay, hitNote, localDayKey, periodLabel, periodRange, retentionLabel } from "./playlogLogic.ts";

const at = (y, m, d, h = 0, min = 0) => new Date(y, m - 1, d, h, min).getTime();

test("rolling presets end now and look back whole days; all has no bounds", () => {
  const now = at(2023, 11, 20, 12);
  assert.deepEqual(periodRange("7", now), { from: now - 7 * DAY_MS, to: null, error: "" });
  assert.deepEqual(periodRange("90", now), { from: now - 90 * DAY_MS, to: null, error: "" });
  assert.deepEqual(periodRange("all", now), { from: null, to: null, error: "" });
});

test("custom dates are whole local days, end date included", () => {
  const r = periodRange("custom", 0, "2023-11-08", "2023-11-09");
  assert.equal(r.from, at(2023, 11, 8));
  assert.equal(r.to, at(2023, 11, 10) - 1);
  assert.equal(r.error, "");
  // a single day
  const one = periodRange("custom", 0, "2023-11-08", "2023-11-08");
  assert.equal(one.to - one.from + 1, at(2023, 11, 9) - at(2023, 11, 8));
  // open ended
  assert.equal(periodRange("custom", 0, "2023-11-08", "").to, null);
  assert.equal(periodRange("custom", 0, "", "2023-11-08").from, null);
  assert.deepEqual(periodRange("custom", 0, "", ""), { from: null, to: null, error: "" });
});

test("bad custom ranges report why and give no bounds", () => {
  for (const [a, b, why] of [["2023-13-01", "", "start"], ["2023-02-30", "", "start"], ["", "nope", "end"], ["2023-11-09", "2023-11-08", "after"]]) {
    const r = periodRange("custom", 0, a, b);
    assert.deepEqual([r.from, r.to], [null, null], `${a} ${b}`);
    assert.ok(r.error.includes(why), r.error);
  }
});

test("dayStart rejects impossible dates and local keys round trip", () => {
  assert.equal(dayStart("2024-02-29"), at(2024, 2, 29));
  assert.equal(dayStart("2023-02-29"), null);
  assert.equal(dayStart("8/11/2023"), null);
  const t = at(2023, 11, 8, 23, 59);
  assert.equal(localDayKey(t), "2023-11-08");
  assert.equal(localDayKey(dayStart("2023-11-08")), "2023-11-08");
});

test("sessions group under the day they started, keeping the given order", () => {
  const s = (start, plays) => ({ start, plays });
  const late = s(at(2023, 11, 9, 23, 30), 4); // runs past midnight, stays on the 9th
  const early = s(at(2023, 11, 9, 20), 3);
  const older = s(at(2023, 11, 2, 21), 5);
  const g = groupByDay([late, early, older]);
  assert.deepEqual(g.map((x) => [x.key, x.sessions.length, x.plays]), [["2023-11-09", 2, 7], ["2023-11-02", 1, 5]]);
  assert.deepEqual(groupByDay([]), []);
});

test("notes say why a play is not exact and are empty when it is", () => {
  const ok = { plays: 1, approx: false, time_known: true, reset: false };
  assert.equal(hitNote(ok), "");
  assert.match(hitNote({ ...ok, plays: 3, approx: true }), /3 plays were seen at once/);
  assert.match(hitNote({ ...ok, approx: true }), /count did not change/);
  assert.match(hitNote({ ...ok, approx: true, time_known: false }), /when the library was read/);
  assert.match(hitNote({ ...ok, reset: true }), /reset/);
  assert.ok(hitNote({ plays: 2, approx: true, time_known: true, reset: true }).includes("; "));
});

test("labels", () => {
  assert.equal(fmtBytes(900), "900 B");
  assert.equal(fmtBytes(2048), "2.0 KB");
  assert.equal(fmtBytes(50 * 1024), "50 KB");
  assert.equal(fmtBytes(3 * 1024 * 1024), "3.0 MB");
  assert.equal(retentionLabel(0), "keeps everything");
  assert.equal(retentionLabel(1), "keeps 1 day");
  assert.equal(retentionLabel(730), "keeps 730 days");
  assert.equal(periodLabel("30"), "the last 30 days");
  assert.equal(periodLabel("custom", "2023-11-01", "2023-11-30"), "2023-11-01 to 2023-11-30");
  assert.equal(periodLabel("custom", "", ""), "everything logged");
});
