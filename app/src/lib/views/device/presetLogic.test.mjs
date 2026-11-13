// Run: node --experimental-strip-types --test src/lib/views/device/presetLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { cleanName, defaultTicks, groupRows, headline, idsToWrite, nameProblem, presetMeta, rowText, sameName, writeLabel } from "./presetLogic.ts";

const row = (id, group, status, current, wanted, detail = null) => ({ id, group, title: `Setting ${id}`, status, current, wanted, detail });
const rows = [
  row(1, "Power", "change", "5 min", "1 min"),
  row(2, "Mix control", "same", "On", "On"),
  row(3, "Power", "change", "Off", "On"),
  row(4, "Audio", "absent", null, "Bright", "the settings file does not hold it yet"),
  row(5, "Other", "unknown", null, "On", "this editor does not know a setting with this number"),
];

test("rows are grouped in the order the groups first appear", () => {
  const g = groupRows(rows);
  assert.deepEqual(g.map((x) => x.group), ["Power", "Mix control", "Audio", "Other"]);
  assert.deepEqual(g[0].rows.map((r) => r.id), [1, 3]);
  assert.deepEqual(groupRows([]), []);
});

test("only change rows start ticked, and only ticked change rows are sent", () => {
  assert.deepEqual(defaultTicks(rows), [1, 3]);
  assert.deepEqual(idsToWrite(rows, [1, 3]), [1, 3]);
  assert.deepEqual(idsToWrite(rows, [3]), [3]);
  assert.deepEqual(idsToWrite(rows, new Set([1, 2, 4, 5, 99])), [1], "ticks on same, blocked or unknown ids are dropped");
  assert.deepEqual(idsToWrite(rows, []), []);
});

test("each row says what happens in words", () => {
  assert.equal(rowText(rows[0]), "5 min → 1 min");
  assert.equal(rowText(rows[1]), "already On");
  assert.equal(rowText(rows[3]), "not applied: the settings file does not hold it yet");
  assert.equal(rowText(row(9, "x", "invalid", "a", "b")), "not applied: it cannot be applied to this card");
  assert.equal(rowText(row(9, "x", "change", null, "On")), "not set → On");
});

test("the headline counts what applying does", () => {
  assert.equal(
    headline({ change: 2, same: 1, blocked: 2, untouched: 7 }, 2),
    "2 settings will change; 1 already matches; 2 entries cannot be applied; 7 other settings on the card stay as they are.",
  );
  assert.equal(headline({ change: 1, same: 0, blocked: 0, untouched: 1 }, 1), "1 setting will change; 1 other setting on the card stays as it is.");
  assert.equal(headline({ change: 3, same: 0, blocked: 0, untouched: 0 }, 1), "1 of 3 changes ticked.");
  assert.equal(headline({ change: 0, same: 4, blocked: 0, untouched: 0 }, 0), "Nothing to change: this card already matches the preset; 4 already match.");
});

test("the write button names the count", () => {
  assert.equal(writeLabel(0, false), "Nothing ticked");
  assert.equal(writeLabel(1, false), "Write 1 setting");
  assert.equal(writeLabel(5, false), "Write 5 settings");
  assert.equal(writeLabel(5, true), "Writing…");
});

test("names are cleaned and compared the way the backend does", () => {
  assert.equal(cleanName("  Club   night \n"), "Club night");
  assert.ok(sameName("Club night", "  CLUB  NIGHT"));
  assert.ok(!sameName("Club night", "Club nights"));
  assert.equal(nameProblem("Club night"), "");
  assert.equal(nameProblem("   "), "Give the preset a name.");
  assert.match(nameProblem("x".repeat(81)), /at most 80/);
  assert.equal(nameProblem("x".repeat(80)), "");
  assert.match(nameProblem("bad\u0007name"), /control/);
});

test("the list line gives the count, the time and the source version when known", () => {
  assert.equal(
    presetMeta({ settings: 5, firmware: "180096069" }, "11 Nov 2026, 09:45 UTC"),
    "5 settings · saved 11 Nov 2026, 09:45 UTC · from software 180096069",
  );
  assert.equal(presetMeta({ settings: 1, firmware: null }, ""), "1 setting");
});
