// Run: node --experimental-strip-types src/lib/state/energyResolve.test.mjs   (also part of `npm test`)
// The parity vectors are the same file the Rust tests of commands/tags/energy_source.rs run.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import * as R from "./energyResolve.ts";

const parity = JSON.parse(readFileSync(fileURLToPath(new URL("./energy.parity.json", import.meta.url)), "utf8"));

test("resolveLevel gives the same answer as the Rust resolver on every shared vector", () => {
  assert.ok(parity.length >= 12);
  for (const c of parity) assert.equal(R.resolveLevel(c.mine, c.auto, c.source), c.level, c.name);
});

test("autoLevel rounds, clamps and refuses unusable numbers", () => {
  assert.equal(R.autoLevel(7.4), 7);
  assert.equal(R.autoLevel(7.5), 8);
  assert.equal(R.autoLevel(0.4), null);
  assert.equal(R.autoLevel(0), null);
  assert.equal(R.autoLevel(99), 10);
  assert.equal(R.autoLevel(NaN), null);
  assert.equal(R.autoLevel(Infinity), null);
  assert.equal(R.autoLevel(null), null);
  assert.equal(R.autoLevel(undefined), null);
});

test("a rating outside 1 to 10 or not whole is not a rating", () => {
  for (const bad of [0, 11, 2.5, NaN, -1]) assert.equal(R.resolveLevel(bad, 6, "mine"), null, String(bad));
  assert.equal(R.resolveLevel(0, 6, "mine_else_auto"), 6, "falls through to the estimate");
});

test("energyUsed names the value in effect, matching resolveLevel", () => {
  assert.equal(R.energyUsed(4, 8, "mine_else_auto"), "mine");
  assert.equal(R.energyUsed(null, 8, "mine_else_auto"), "auto");
  assert.equal(R.energyUsed(null, null, "mine_else_auto"), null);
  assert.equal(R.energyUsed(4, 8, "auto"), "auto");
  assert.equal(R.energyUsed(4, null, "auto"), null);
  assert.equal(R.energyUsed(4, 8, "mine"), "mine");
  assert.equal(R.energyUsed(null, 8, "mine"), null);
  assert.equal(R.energyUsed(null, 0.2, "auto"), null, "an estimate that rounds below 1 is no energy");
  for (const c of parity) assert.equal(R.energyUsed(c.mine, c.auto, c.source) !== null, c.level !== null, c.name);
});

test("the source names", () => {
  assert.equal(R.DEFAULT_ENERGY_SOURCE, "mine_else_auto");
  assert.ok(R.ENERGY_SOURCES.every((s) => R.isEnergySource(s.id) && s.label && s.help));
  assert.equal(R.isEnergySource("both"), false);
  assert.equal(R.energySourceLabel("auto"), "Auto only");
});
