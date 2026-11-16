// Run: node --experimental-strip-types src/lib/state/appearanceLogic.test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PLAYHEAD_FPS_DEFAULT, normalisePlayheadFps, playheadDue, WAVE_COLOURS, WAVE_THEME_DEFAULT, normaliseWaveTheme, addToast, DONE_SECS_DEFAULT, normaliseDoneSecs, ERROR_SECS_DEFAULT, GRID_MIN_DEFAULT, MAX_TOASTS, normaliseGridMin, clampScale, normaliseErrorSecs, normaliseScale, scaleKey, stepScale, toastMs, zoomFactor } from "./appearanceLogic.ts";

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

// the same message again is one card with a count; different ones stack, oldest dropped past the limit
{
  let items = [];
  items = addToast(items, { id: 1, text: "Track added", kind: "ok" });
  items = addToast(items, { id: 2, text: "Kept at its own tempo", kind: "info" });
  items = addToast(items, { id: 3, text: "Track added", kind: "ok" });
  assert.deepEqual(items.map((t) => [t.id, t.count]), [[2, 1], [3, 2]], "the repeat replaces the old card and counts it");
  assert.equal(addToast(items, { id: 4, text: "Track added", kind: "bad" }).length, 3, "another kind is another message");
  let many = [];
  for (let i = 0; i < 10; i++) many = addToast(many, { id: i, text: `m${i}`, kind: "info" });
  assert.equal(many.length, MAX_TOASTS);
  assert.equal(many.at(-1).text, "m9");
}

console.log("appearanceLogic.test.mjs ok");

// how long a finished job's note stays, and the beat grid cutoff: only the offered choices, junk gives the default
assert.equal(normaliseDoneSecs(30), 30);
assert.equal(normaliseDoneSecs(0), 0, "0 = until dismissed");
assert.equal(normaliseDoneSecs(7), DONE_SECS_DEFAULT);
assert.equal(normaliseDoneSecs(null), DONE_SECS_DEFAULT);
assert.equal(normaliseGridMin(0), 0);
assert.equal(normaliseGridMin(4), 4);
assert.equal(normaliseGridMin(3), GRID_MIN_DEFAULT);
assert.equal(normaliseGridMin(null), GRID_MIN_DEFAULT);

// waveform colours: calm matrix green is the default, neon stays as the other choice, junk gives the default
assert.equal(WAVE_THEME_DEFAULT, "matrix");
assert.equal(normaliseWaveTheme(undefined), "matrix");
assert.equal(normaliseWaveTheme("neon"), "neon");
assert.equal(normaliseWaveTheme("matrix"), "matrix");
for (const junk of [null, "Neon", 1, {}, "purple"]) assert.equal(normaliseWaveTheme(junk), "matrix", String(junk));
assert.deepEqual(WAVE_COLOURS.neon, { low: "#ff48a8", mid: "#e070ff", high: "#00ff46" }, "neon is the original colours");
// calm: all three bands are greens (green is the strongest channel), darkest on the outside
const ch = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
for (const k of ["low", "mid", "high"]) {
  const [r, g, b] = ch(WAVE_COLOURS.matrix[k]);
  assert.ok(g > r && g > b, `matrix ${k} is green`);
}
const lum = (h) => ch(h).reduce((a, c) => a + c, 0);
assert.ok(lum(WAVE_COLOURS.matrix.low) < lum(WAVE_COLOURS.matrix.mid) && lum(WAVE_COLOURS.matrix.mid) < lum(WAVE_COLOURS.matrix.high));
// the stylesheet's own values (what shows before the app sets them) are the default theme
const css = readFileSync(new URL("../styles/tokens.css", import.meta.url), "utf8");
for (const k of ["low", "mid", "high"]) assert.match(css, new RegExp(`--wave-${k}: ${WAVE_COLOURS.matrix[k]};`), `tokens.css --wave-${k}`);

// playhead smoothness: 60 a second by default, 30 or every refresh as the other choices, junk gives the default
assert.equal(PLAYHEAD_FPS_DEFAULT, 60);
for (const ok of [60, 30, 0]) assert.equal(normalisePlayheadFps(ok), ok);
for (const junk of [null, undefined, "30", 45, -1, NaN, {}]) assert.equal(normalisePlayheadFps(junk), 60, String(junk));
// how many frames of a screen draw: simulate 1 s of refreshes at each rate
const drawn = (hz, cap) => {
  let last = -Infinity, n = 0;
  for (let f = 0; f < hz; f++) {
    const now = (f * 1000) / hz;
    if (playheadDue(now, last, cap)) { last = now; n++; }
  }
  return n;
};
assert.equal(drawn(60, 60), 60, "a 60 Hz screen under the 60 cap loses no frame");
assert.equal(drawn(60, 30), 30);
assert.equal(drawn(144, 60), 72);
assert.equal(drawn(166, 60), 56);
assert.equal(drawn(166, 0), 166, "0 = every refresh, as before");
assert.equal(drawn(30, 60), 30, "a slow screen is never throttled further");
assert.equal(playheadDue(0, -Infinity, 30), true, "the first frame always draws");
