// Run: node --experimental-strip-types src/lib/state/appearanceLogic.test.mjs
import assert from "node:assert/strict";
import { ERROR_SECS_DEFAULT, GRID_MIN_DEFAULT, MAX_TOASTS, normaliseGridMin, clampScale, normaliseErrorSecs, normaliseScale, scaleKey, stepScale, toastMs, zoomFactor } from "./appearanceLogic.ts";

// size: 80 to 200 %, default 100, junk gives the default
assert.equal(clampScale(100), 100);
assert.equal(clampScale(50), 80);
assert.equal(clampScale(500), 200);
assert.equal(clampScale(133.4), 133);
for (const junk of [null, undefined, "150", NaN, Infinity, {}]) assert.equal(normaliseScale(junk), 100, String(junk));
assert.equal(zoomFactor(150), 1.5);
assert.equal(zoomFactor(9999), 2, "the factor is held to the largest size too");

// steps land on multiples of 10 and stop at the ends
assert.equal(stepScale(100, 1), 110);
assert.equal(stepScale(100, -1), 90);
assert.equal(stepScale(105, 1), 110);
assert.equal(stepScale(105, -1), 100);
assert.equal(stepScale(200, 1), 200);
assert.equal(stepScale(80, -1), 80);
assert.equal(stepScale(133, -1), 130);

// the shortcuts
const k = (key, extra = {}) => scaleKey({ key, ctrlKey: true, metaKey: false, altKey: false, ...extra });
assert.equal(k("="), "bigger");
assert.equal(k("+"), "bigger");
assert.equal(k("-"), "smaller");
assert.equal(k("_"), "smaller");
assert.equal(k("0"), "reset");
assert.equal(k("a"), null);
assert.equal(k("=", { ctrlKey: false }), null, "the mix editor's plain = and - are not touched");
assert.equal(k("=", { altKey: true }), null);
assert.equal(k("=", { ctrlKey: false, metaKey: true }), "bigger");

// error messages: a setting, 0 = until closed, never shorter than the caller asked
assert.equal(normaliseErrorSecs(null), ERROR_SECS_DEFAULT);
assert.equal(normaliseErrorSecs(7), ERROR_SECS_DEFAULT, "only the offered choices");
assert.equal(normaliseErrorSecs(0), 0);
assert.equal(normaliseErrorSecs(60), 60);
assert.equal(toastMs("ok", 4000, 20), 4000);
assert.equal(toastMs("info", 6000, 0), 6000);
assert.equal(toastMs("bad", 4000, 20), 20000, "an error toast outlasts the 4 s default");
assert.equal(toastMs("bad", 30000, 20), 30000, "a caller that asked for longer keeps it");
assert.equal(toastMs("bad", 4000, 0), null, "until closed");
assert.ok(MAX_TOASTS >= 3);

console.log("appearanceLogic.test.mjs ok");

// beat grid cutoff: only the offered choices, junk gives the default
assert.equal(normaliseGridMin(0), 0);
assert.equal(normaliseGridMin(4), 4);
assert.equal(normaliseGridMin(3), GRID_MIN_DEFAULT);
assert.equal(normaliseGridMin(null), GRID_MIN_DEFAULT);
