// Tests for the set builder's pure logic: the solver (solver.ts) and the mix layout (buildMix.ts).
// A plain node script like the other tests; it is run by scripts/test-mixedit.mjs (npm run
// test:mixedit), or alone:
//
//   node --experimental-strip-types src/lib/views/setbuilder/solver.test.mjs      (from app/)
//
// All data is invented. Exits non-zero on the first failed assertion.
import assert from "node:assert/strict";
import * as E from "../../state/mixedit.ts";
import * as R from "../../state/transitionRecipes.ts";
import * as B from "./buildMix.ts";
import * as S from "./solver.ts";

let n = 0;
const test = async (name, fn) => {
  await fn();
  n++;
  console.log(`ok  ${name}`);
};
const close = (a, b, eps = 1e-6, msg = "") => assert.ok(Math.abs(a - b) <= eps, `${msg} ${a} vs ${b}`);

const key = (s) => (s ? { num: Number(s.slice(0, -1)), letter: s.slice(-1) } : null);
const tr = (id, bpm, k, secs = 240, energy = null) => ({ id, bpm, key: key(k), secs, energy });
const O = (extra = {}) => ({ ...S.DEFAULT_SOLVER, ...extra });
const byIdOf = (tracks) => new Map(tracks.map((t) => [t.id, t]));
const costOfOrder = (tracks, order, o) => S.evaluate(byIdOf(tracks), order, o).cost;

/** Cost of the best order by trying every permutation (n <= 7). */
function bruteBest(tracks, o) {
  const ids = tracks.map((t) => t.id);
  let best = Infinity;
  const rec = (rest, acc) => {
    if (rest.length === 0) {
      best = Math.min(best, costOfOrder(tracks, acc, o));
      return;
    }
    rest.forEach((x, i) => rec([...rest.slice(0, i), ...rest.slice(i + 1)], [...acc, x]));
  };
  rec(ids, []);
  return best;
}

// ---- keys, tempo, transitions ------------------------------------------------------------------

await test("key moves on the Camelot wheel", () => {
  const m = (a, b) => S.keyMove(key(a), key(b));
  const table = [
    ["8A", "8A", "same"], ["8A", "8B", "relative"], ["8B", "8A", "relative"],
    ["8A", "9A", "adjacent"], ["8A", "7A", "adjacent"], ["12A", "1A", "adjacent"], ["1B", "12B", "adjacent"],
    ["8A", "10A", "twoSteps"], ["8A", "6A", "twoSteps"], ["12A", "2A", "twoSteps"],
    ["8A", "9B", "diagonal"], ["8A", "7B", "diagonal"], ["1B", "12A", "diagonal"],
    ["8A", "3A", "semitone"], ["8A", "1A", "semitone"],
    ["8A", "2A", "clash"], ["8A", "3B", "clash"], ["8A", "11B", "clash"], ["8A", "5A", "clash"],
  ];
  for (const [a, b, want] of table) assert.equal(m(a, b), want, `${a} to ${b}`);
});

await test("moves read back in words", () => {
  const t = (a, b) => S.moveText(key(a), key(b));
  assert.equal(t("8A", "8A"), "same key");
  assert.equal(t("8A", "9A"), "+1");
  assert.equal(t("8A", "7A"), "−1");
  assert.equal(t("8A", "6A"), "−2");
  assert.equal(t("8A", "8B"), "relative (A to B)");
  assert.equal(t("8A", "3A"), "+7 (semitone up)");
  assert.equal(t("8A", "1A"), "−7 (semitone down)");
  assert.equal(t("8A", "9B"), "diagonal (+1, A to B)");
  assert.match(t("8A", "2A"), /clash/);
});

await test("tempo match: percent change, half and double time, unknown", () => {
  const a = S.bpmMatch(120, 124, true);
  assert.equal(a.relation, "same");
  close(a.deltaPct, (4 / 120) * 100);
  const d = S.bpmMatch(140, 70, true);
  assert.equal(d.relation, "double");
  close(d.deltaPct, 0);
  const h = S.bpmMatch(70, 140, true);
  assert.equal(h.relation, "half");
  close(h.deltaPct, 0);
  close(S.bpmMatch(140, 70, false).deltaPct, -50);
  assert.equal(S.bpmMatch(140, 70, false).relation, "same");
  assert.equal(S.bpmMatch(0, 70, true).relation, "unknown");
  assert.equal(S.bpmMatch(70, NaN, true).relation, "unknown");
  // 128 -> 64 + 3 % is closer to double time than to the plain tempo
  const e = S.bpmMatch(128, 66, true);
  assert.equal(e.relation, "double");
  close(e.used, 132);
});

await test("a transition's score, limits and concerns", () => {
  const o = O();
  const perfect = S.transitionBetween(tr(1, 124, "8A"), tr(2, 124, "8A"), o);
  assert.equal(perfect.score, 100);
  assert.equal(perfect.keyOk, true);
  assert.equal(perfect.bpmOk, true);
  assert.deepEqual(perfect.concerns, []);

  const plus1 = S.transitionBetween(tr(1, 124, "8A"), tr(2, 124, "9A"), o);
  assert.equal(plus1.keyScore, 90);
  assert.equal(plus1.score, 95); // half key 90, half tempo 100

  const clash = S.transitionBetween(tr(1, 124, "8A"), tr(2, 124, "3B"), o);
  assert.equal(clash.keyOk, false);
  assert.equal(clash.keyScore, S.CLASH_SCORE);
  assert.match(clash.concerns[0], /clash/);

  // a move that exists but is not ticked is refused in words
  const notAllowed = S.transitionBetween(tr(1, 124, "8A"), tr(2, 124, "10A"), o);
  assert.equal(notAllowed.move, "twoSteps");
  assert.equal(notAllowed.keyOk, false);
  assert.match(notAllowed.concerns[0], /not allowed/);
  assert.equal(S.transitionBetween(tr(1, 124, "8A"), tr(2, 124, "10A"), O({ allowedMoves: ["same", "twoSteps"] })).keyOk, true);

  const jump = S.transitionBetween(tr(1, 100, "8A"), tr(2, 110, "8A"), o);
  assert.equal(jump.bpmOk, false);
  assert.ok(jump.bpmScore < 60);
  assert.match(jump.concerns[0], /tempo jumps 10\.0 %/);
  // exactly at the limit is allowed and scores 60
  const edge = S.transitionBetween(tr(1, 100, "8A"), tr(2, 106, "8A"), o);
  assert.equal(edge.bpmOk, true);
  close(edge.bpmScore, 60);

  const unknown = S.transitionBetween(tr(1, 0, null), tr(2, 124, "8A"), o);
  assert.equal(unknown.keyOk, null);
  assert.equal(unknown.bpmOk, null);
  assert.equal(unknown.keyScore, S.UNKNOWN_SCORE);
  assert.equal(unknown.score, 50);
  assert.equal(unknown.concerns.length, 2);

  // the key / tempo balance
  const k = S.transitionBetween(tr(1, 100, "8A"), tr(2, 110, "9A"), O({ keyWeight: 1 }));
  assert.equal(k.score, 90);
  const t = S.transitionBetween(tr(1, 100, "8A"), tr(2, 110, "9A"), O({ keyWeight: 0 }));
  assert.equal(t.score, Math.round(t.bpmScore));
  // half time counts as matching
  assert.equal(S.transitionBetween(tr(1, 140, "8A"), tr(2, 70, "8A"), O({ halfDouble: true })).bpmOk, true);
  assert.equal(S.transitionBetween(tr(1, 140, "8A"), tr(2, 70, "8A"), O({ halfDouble: false })).bpmOk, false);
  // energy change is reported, not scored
  assert.equal(S.transitionBetween(tr(1, 124, "8A", 240, 4), tr(2, 124, "8A", 240, 7), o).energyDelta, 3);
  assert.equal(S.transitionBetween(tr(1, 124, "8A", 240, 4), tr(2, 124, "8A"), o).energyDelta, null);
});

await test("the energy curve: warm-up, peak, cool-down", () => {
  close(S.arcTarget(0, 0.65), 0.25);
  close(S.arcTarget(0.65, 0.65), 1);
  close(S.arcTarget(1, 0.65), 0.4);
  assert.ok(S.arcTarget(0.3, 0.65) < S.arcTarget(0.5, 0.65));
  assert.ok(S.arcTarget(0.8, 0.65) > S.arcTarget(0.95, 0.65));
  close(S.energyNorm(1), 0);
  close(S.energyNorm(10), 1);
  close(S.energyNorm(55), 1);
  const o = O({ curve: "arc", energyWeight: 1 });
  assert.equal(S.energyCost(tr(1, 0, null), 3, 7, o), 0, "no energy, no cost");
  assert.equal(S.energyCost(tr(1, 0, null, 240, 5), 3, 7, O()), 0, "curve off");
  assert.equal(S.curveActive([tr(1, 0, null)], o), false);
  assert.equal(S.curveActive([tr(1, 0, null, 240, 5)], o), true);
  assert.equal(S.curveActive([tr(1, 0, null, 240, 5)], O()), false);
  // a calm track at the peak costs more than a strong one
  assert.ok(S.energyCost(tr(1, 0, null, 240, 2), 4, 7, o) > S.energyCost(tr(1, 0, null, 240, 10), 4, 7, o));
});

// ---- pins, moving, inserting -------------------------------------------------------------------

await test("pinned positions are clamped, ordered and never share a slot", () => {
  assert.deepEqual(S.resolvePins([[1, 0], [2, 3]], 5), [[1, 0], [2, 3]]);
  assert.deepEqual(S.resolvePins([[1, 10], [2, 10]], 4), [[1, 2], [2, 3]]);
  assert.deepEqual(S.resolvePins([[7, 2], [8, 2], [9, 2]], 5), [[7, 2], [8, 3], [9, 4]]);
  assert.deepEqual(S.resolvePins([[5, -3]], 3), [[5, 0]]);
  assert.deepEqual(S.resolvePins([], 3), []);
});

await test("moveBlock keeps the block's order and reports no change", () => {
  const order = [1, 2, 3, 4, 5, 6];
  assert.deepEqual(S.moveBlock(order, new Set([2]), 5), [1, 3, 4, 5, 2, 6]);
  assert.deepEqual(S.moveBlock(order, new Set([5, 3]), 0), [3, 5, 1, 2, 4, 6]);
  assert.deepEqual(S.moveBlock(order, new Set([6]), 0), [6, 1, 2, 3, 4, 5]);
  assert.equal(S.moveBlock(order, new Set([3]), 3), order, "dropping on its own place is no change");
  assert.equal(S.moveBlock(order, new Set([3]), 2), order);
  assert.equal(S.moveBlock(order, new Set([1, 2]), 2), order);
});

await test("a left-out track goes back where it costs least", () => {
  const tracks = [tr(1, 120, "1A"), tr(2, 121, "2A"), tr(3, 122, "3A"), tr(4, 123, "4A"), tr(9, 121.5, "2A")];
  const at = S.bestInsert(byIdOf(tracks), [1, 2, 3, 4], tracks[4], O());
  assert.ok(at === 1 || at === 2, `placed at ${at}`);
  assert.equal(S.bestInsert(byIdOf(tracks), [], tracks[4], O()), 0);
});

// ---- solving -----------------------------------------------------------------------------------

/** A set that has one clean harmonic and tempo chain, listed scrambled. */
function chainTracks() {
  return [
    tr(1, 120, "1A"), tr(2, 121, "2A"), tr(3, 122, "3A"), tr(4, 123, "4A"), tr(5, 124, "5A"), tr(6, 125, "6A"),
    tr(7, 126, "7A"), tr(8, 127, "8A"),
  ];
}
const scramble = (tracks) => [4, 0, 6, 2, 7, 1, 5, 3].map((i) => tracks[i]);

await test("solve orders a scrambled chain into the chain and is deterministic", () => {
  const tracks = chainTracks();
  const pool = scramble(tracks);
  const o = O();
  const r = S.solve(pool, new Map(), o);
  assert.deepEqual([...r.order].sort(), [1, 2, 3, 4, 5, 6, 7, 8], "a permutation");
  const asc = [1, 2, 3, 4, 5, 6, 7, 8];
  const desc = [...asc].reverse();
  assert.ok(JSON.stringify(r.order) === JSON.stringify(asc) || JSON.stringify(r.order) === JSON.stringify(desc), `got ${r.order}`);
  assert.equal(S.evaluate(byIdOf(tracks), r.order, o).violations, 0);
  const again = S.solve(pool, new Map(), o);
  assert.deepEqual(again.order, r.order);
  assert.ok(costOfOrder(tracks, r.order, o) < costOfOrder(tracks, pool.map((t) => t.id), o));
  assert.deepEqual(r.dropped, []);
  assert.equal(r.short, false);
});

await test("solve reaches the best cost on small sets (checked by trying every order)", () => {
  // a mixed bag with some awkward tracks: 7 tracks, 5040 orders
  const tracks = [
    tr(1, 126, "8A"), tr(2, 128, "9A"), tr(3, 140, "9B"), tr(4, 70, "8B"), tr(5, 127, "2A"), tr(6, 125, "7A"), tr(7, 0, null),
  ];
  for (const extra of [{}, { halfDouble: false }, { keyWeight: 0.2 }, { maxBpmJumpPct: 2 }]) {
    const o = O(extra);
    const r = S.solve(tracks, new Map(), o);
    const got = costOfOrder(tracks, r.order, o);
    const best = bruteBest(tracks, o);
    assert.ok(got <= best + 1e-6, `${JSON.stringify(extra)}: solver ${got} vs best ${best}`);
  }
});

await test("pinned tracks keep their positions", () => {
  const tracks = chainTracks();
  const o = O();
  const pins = new Map([[8, 0], [1, 5]]);
  const r = S.solve(scramble(tracks), pins, o);
  assert.equal(r.order[0], 8);
  assert.equal(r.order[5], 1);
  assert.deepEqual([...r.order].sort(), [1, 2, 3, 4, 5, 6, 7, 8]);
  // every other track is still placed to suit them: no worse than leaving the pinned order as is
  const naive = [8, 2, 3, 4, 5, 1, 6, 7];
  assert.ok(costOfOrder(tracks, r.order, o) <= costOfOrder(tracks, naive, o));
  // a pin past the end is clamped
  const r2 = S.solve(tracks, new Map([[3, 99]]), o);
  assert.equal(r2.order[7], 3);
  // pins on a single track change nothing about the rest being a permutation
  assert.equal(new Set(r2.order).size, 8);
});

await test("a target length drops the tracks that fit the set least", () => {
  const chain = chainTracks();
  const odd = tr(20, 160, "3B"); // wrong tempo and a clashing key
  const pool = [...chain, odd];
  const o = O({ targetSecs: 8 * 240 });
  const r = S.solve(pool, new Map(), o);
  assert.deepEqual(r.dropped, [20]);
  assert.equal(r.order.length, 8);
  assert.equal(r.secs, 8 * 240);
  assert.equal(r.short, false);

  // the whole pool fits: nothing dropped, and a target above the total says the pool is short
  const big = S.solve(chain, new Map(), O({ targetSecs: 100 * 240 }));
  assert.deepEqual(big.dropped, []);
  assert.equal(big.short, true);

  // a pinned track is never the one dropped
  const pinned = S.solve(pool, new Map([[20, 0]]), o);
  assert.ok(pinned.order.includes(20));
  assert.equal(pinned.order[0], 20);
  assert.equal(pinned.dropped.length, 1);

  // stops where the total is closest to the target (here 6 tracks of 240 s = 1440 vs 1500)
  const closest = S.solve(chain, new Map(), O({ targetSecs: 1500 }));
  assert.equal(closest.order.length, 6);
  assert.equal(closest.dropped.length, 2);
  // deterministic
  assert.deepEqual(S.solve(chain, new Map(), O({ targetSecs: 1500 })).order, closest.order);
});

await test("the longest a track plays caps its time in the total", () => {
  const tracks = [tr(1, 120, "1A", 900), tr(2, 121, "2A", 900), tr(3, 122, "3A", 100), tr(4, 123, "4A", 900)];
  const o = O({ maxTrackSecs: 300 });
  assert.equal(S.playSecs(tracks[0], o), 300);
  assert.equal(S.playSecs(tracks[2], o), 100);
  assert.equal(S.playSecs(tracks[0], O()), 900);
  assert.equal(S.evaluate(byIdOf(tracks), [1, 2, 3, 4], o).secs, 300 + 300 + 100 + 300);
  // 1000 s of music in a 700 s target: drop one 300 s track, leaving 700
  const r = S.solve(tracks, new Map(), O({ maxTrackSecs: 300, targetSecs: 700 }));
  assert.equal(r.secs, 700);
  assert.equal(r.order.length, 3);
});

await test("the energy curve pulls strong tracks to the peak", () => {
  // seven tracks that mix equally well with each other; only energy tells them apart
  const energies = [5, 9, 2, 7, 3, 10, 4];
  const tracks = energies.map((e, i) => tr(i + 1, 124, "8A", 240, e));
  const off = S.solve(tracks, new Map(), O());
  assert.deepEqual(off.order, [1, 2, 3, 4, 5, 6, 7], "no curve: nothing to choose between, the order given stands");
  const o = O({ curve: "arc", energyWeight: 1, peakAt: 0.65 });
  const r = S.solve(tracks, new Map(), o);
  const e = r.order.map((id) => tracks[id - 1].energy);
  const peak = e.indexOf(10);
  assert.ok(peak >= 3 && peak <= 5, `the strongest track is at ${peak}: ${e}`);
  assert.ok(e[0] < 7 && e[e.length - 1] < 8, `opens and closes lower: ${e}`);
  // the curve costs less than the unsorted order did
  assert.ok(costOfOrder(tracks, r.order, o) < costOfOrder(tracks, [1, 2, 3, 4, 5, 6, 7], o));
  // tracks without energy are not pulled anywhere
  const some = tracks.map((t, i) => (i < 3 ? { ...t, energy: null } : t));
  const r2 = S.solve(some, new Map(), o);
  assert.deepEqual([...r2.order].sort(), [1, 2, 3, 4, 5, 6, 7]);
});

await test("a summary of the set: average, weakest hop, limits broken", () => {
  const tracks = [tr(1, 120, "8A"), tr(2, 120, "8A"), tr(3, 150, "3B"), tr(4, 150, "3B")];
  const s = S.evaluate(byIdOf(tracks), [1, 2, 3, 4], O());
  assert.equal(s.transitions.length, 3);
  assert.equal(s.transitions[0].score, 100);
  assert.equal(s.minAt, 1);
  assert.equal(s.violations, 1);
  assert.equal(s.secs, 960);
  close(s.avgScore, (100 + s.transitions[1].score + 100) / 3);
  const none = S.evaluate(byIdOf(tracks), [1], O());
  assert.equal(none.avgScore, null);
  assert.equal(none.minScore, null);
  assert.deepEqual(S.evaluate(byIdOf(tracks), [], O()).transitions, []);
});

await test("solving tiny and empty sets", () => {
  assert.deepEqual(S.solve([], new Map(), O()).order, []);
  assert.deepEqual(S.solve([tr(1, 120, "1A")], new Map(), O()).order, [1]);
  assert.deepEqual(S.solve([tr(1, 120, "1A"), tr(2, 121, "1A")], new Map(), O()).order, [1, 2]);
  // everything pinned: the order is exactly the pins
  const t = chainTracks();
  const r = S.solve(t, new Map(t.map((x, i) => [x.id, 7 - i])), O());
  assert.deepEqual(r.order, [8, 7, 6, 5, 4, 3, 2, 1]);
  // a pin for a track not in the pool is ignored
  assert.deepEqual(S.solve([tr(1, 120, "1A"), tr(2, 121, "1A")], new Map([[99, 0]]), O()).order, [1, 2]);
});

await test("a bigger pool solves within bounds and stays deterministic", () => {
  const tracks = Array.from({ length: 60 }, (_, i) => tr(i + 1, 118 + ((i * 7) % 17), `${1 + ((i * 5) % 12)}${i % 3 === 0 ? "B" : "A"}`));
  const o = O();
  const t0 = Date.now();
  const a = S.solve(tracks, new Map(), o);
  const b = S.solve(tracks, new Map(), o);
  assert.deepEqual(a.order, b.order);
  assert.equal(new Set(a.order).size, 60);
  assert.ok(costOfOrder(tracks, a.order, o) < costOfOrder(tracks, tracks.map((t) => t.id), o));
  assert.ok(Date.now() - t0 < 20000, "two solves of 60 tracks took too long");
});

await test("words for lengths", () => {
  assert.equal(S.clock(0), "0:00");
  assert.equal(S.clock(59.6), "1:00");
  assert.equal(S.clock(3510), "58:30");
  assert.equal(S.clock(3730), "1:02:10");
  assert.equal(S.lengthLine(3510, { targetSecs: null }), "58:30 of music");
  assert.equal(S.lengthLine(3510, { targetSecs: 3600 }), "58:30, 1:30 under the 1:00:00 target");
  assert.equal(S.lengthLine(3600, { targetSecs: 3600 }), "1:00:00, exactly the 1:00:00 target");
  assert.match(S.lengthLine(600, { targetSecs: 3600 }, true), /not enough music/);
});

await test("settings: defaults, merging, and bad saved values", () => {
  assert.deepEqual(S.mergePrefs(null), S.DEFAULT_PREFS);
  assert.deepEqual(S.mergePrefs("junk"), S.DEFAULT_PREFS);
  assert.deepEqual(S.mergePrefs({}), S.DEFAULT_PREFS);
  const p = S.mergePrefs({ allowedMoves: ["same", "nonsense", "twoSteps"], maxBpmJumpPct: 3, halfDouble: false, keyWeight: 0.8, curve: "arc", peakAt: 0.5, maxTrackMins: 5, targetMins: 60, solveOnLoad: false, lockOnDrag: false, beatmatch: false, mixTempo: 126 });
  assert.deepEqual(p.allowedMoves, ["same", "twoSteps"]);
  assert.equal(p.maxBpmJumpPct, 3);
  assert.equal(p.halfDouble, false);
  assert.equal(p.curve, "arc");
  assert.equal(p.targetMins, 60);
  assert.equal(p.mixTempo, 126);
  const bad = S.mergePrefs({ maxBpmJumpPct: -4, keyWeight: 7, curve: "zigzag", peakAt: 0, maxTrackMins: "x", targetMins: 0, mixTempo: 5, halfDouble: "yes" });
  assert.deepEqual(bad, S.DEFAULT_PREFS);
  assert.equal(S.mergePrefs({ maxTrackMins: null, targetMins: null, mixTempo: null }).targetMins, null);
  // the defaults are not shared with the merged copy
  const copy = S.mergePrefs({});
  copy.allowedMoves.push("diagonal");
  assert.ok(!S.DEFAULT_PREFS.allowedMoves.includes("diagonal"));
  const o = S.solverOptions(S.mergePrefs({ maxTrackMins: 5, targetMins: 60 }));
  assert.equal(o.maxTrackSecs, 300);
  assert.equal(o.targetSecs, 3600);
  assert.equal(S.solverOptions(S.DEFAULT_PREFS).maxTrackSecs, null);
});

// ---- planning and building a mix ---------------------------------------------------------------

await test("the mix plan: blends, sequences and the tempo each track plays at", () => {
  const all = () => true;
  const po = { beatmatch: true, mixTempo: null, halfDouble: true };
  const t = [tr(1, 124, "8A"), tr(2, 126, "8A"), tr(3, 128, "9A")];
  const p = S.planMix(t, all, po);
  assert.deepEqual(p.pairs.map((x) => x.mode), ["blend", "blend"]);
  // with no tempo given, the mix runs at the middle tempo of the tracks (126), which keeps the biggest stretch smallest
  assert.deepEqual(p.tempos, [126, 126, 126]);
  close(p.stretchPct[0], (126 / 124 - 1) * 100);
  close(p.stretchPct[1], 0);
  close(p.stretchPct[2], (126 / 128 - 1) * 100);
  assert.equal(S.middleTempo(t, all, true), 126);
  assert.equal(S.middleTempo([tr(1, 120, "8A"), tr(2, 130, "8A")], all, true), 125);
  assert.equal(S.middleTempo([tr(1, 120, "8A"), tr(2, 60, "8A"), tr(3, 122, "8A")], all, true), 120, "a half-time track is counted at the first one's octave");
  assert.equal(S.middleTempo([tr(1, 0, "8A")], all, true), null);
  assert.equal(S.middleTempo(t, () => false, true), null);

  // a tempo the user sets applies to the whole mix
  const m = S.planMix(t, all, { ...po, mixTempo: 130 });
  assert.deepEqual(m.tempos, [130, 130, 130]);
  close(m.stretchPct[0], (130 / 124 - 1) * 100);
  close(m.stretchPct[2], (130 / 128 - 1) * 100);
  // ... but not without beat-matching: tracks keep their own tempo
  const nb = S.planMix(t, all, { ...po, beatmatch: false, mixTempo: 126 });
  assert.deepEqual(nb.tempos, [124, 126, 128]);
  assert.deepEqual(nb.stretchPct, [0, 0, 0]);

  // no grid: the hop is a sequence, whichever side lacks it
  const g = S.planMix(t, (id) => id !== 2, po);
  assert.deepEqual(g.pairs.map((x) => [x.mode, x.why, x.culprit]), [["sequence", "noGrid", 2], ["sequence", "noGrid", 2]]);
  assert.deepEqual(g.tempos, [124, 126, 128], "tracks that are not blended keep their own tempo");
  // a run of its own tempo after a break: the first two blend, the third has no grid
  const r2 = S.planMix([tr(1, 124, "8A"), tr(2, 128, "8A"), tr(3, 100, "8A")], (id) => id !== 3, po);
  assert.deepEqual(r2.pairs.map((x) => x.mode), ["blend", "sequence"]);
  assert.deepEqual(r2.tempos, [126, 126, 100]);

  // half time is not stretched to double: the hop is a sequence
  const h = S.planMix([tr(1, 140, "8A"), tr(2, 70, "8A"), tr(3, 72, "8A")], all, po);
  assert.deepEqual(h.pairs.map((x) => [x.mode, x.why]), [["sequence", "halfDouble"], ["blend", null]]);
  assert.deepEqual(h.tempos, [140, 71, 71], "the first track is a run of its own; the other two run at their middle tempo");
  close(h.stretchPct[0], 0);
  // ... but with half / double switched off it is a plain (huge) stretch
  const h2 = S.planMix([tr(1, 140, "8A"), tr(2, 70, "8A")], all, { ...po, halfDouble: false });
  assert.equal(h2.pairs[0].mode, "blend");
  close(h2.stretchPct[0], -25);
  close(h2.stretchPct[1], 50);

  // unknown tempo
  const u = S.planMix([tr(1, 124, "8A"), tr(2, 0, "8A")], all, po);
  assert.deepEqual([u.pairs[0].mode, u.pairs[0].why, u.pairs[0].culprit], ["sequence", "noTempo", 2]);
  assert.equal(u.tempos[1], null);
  const first = S.planMix([tr(1, 0, "8A"), tr(2, 124, "8A")], all, po);
  assert.deepEqual([first.pairs[0].why, first.pairs[0].culprit], ["noTempo", 1]);
  assert.deepEqual(S.planMix([], all, po), { pairs: [], tempos: [], stretchPct: [] });
});

// A fake backend made of the same pure functions the editor uses. Beat alignment is not
// simulated (the Rust code owns it); tempo, length and overlap are.
const SR = E.SR;
function fakeClip(id, bpm, secs, grid = true) {
  const length = secs * SR;
  const step = (60 / bpm) * SR;
  const beats = Math.floor((length - 2000) / step);
  const g = grid ? { sourceRate: SR, beatStartOffset: 2000, beatFrames: Array.from({ length: beats }, (_, i) => Math.round(2000 + i * step)), beatBpm: Array.from({ length: beats }, () => bpm), firstDownbeat: 0 } : null;
  const z = { start: 0, length, trimStart: 0, trimEnd: 0, limitSize: true, startSec: 0, lengthSec: secs };
  return {
    ...z, id: `clip-${id}`, trackId: id, channel: 0, visibleStart: 0, visibleEnd: length, visibleStartSec: 0, visibleEndSec: secs,
    bpm, tempo: [{ pos: 0, speed: 1 }], pitchMode: false, key: [{ pos: 0, cents: 0, flags: 1 }],
    gain: [{ pos: 0, value: 0 }, { pos: length - 1, value: 0 }], processes: [],
    filePath: "x", offset: 0, clipName: "AudioClip", clipBypassed: false, toniumId: "", otherIds: {}, fuzzyId: { artist: "", album: "", title: `T${id}`, genre: "" },
    volume: { ...z, limitSize: false, name: "noname", bypassed: false, effects: [], sessionEventId: "", otherParams: {}, gainKeyPresent: true },
    track: { trackId: id, available: true, problem: null, title: `T${id}`, artist: "", album: "", genre: "", key: "", bpm, durationSecs: secs, sourceRate: SR, location: "", grid: g, gridProblem: grid ? null : "no .str file" },
  };
}
const emptyDoc = () => E.refresh({ id: 5, name: "Set mix", genre: "", artist: "", deviceInfo: "", sampleRate: SR, tempoBpm: null, clips: [], transitions: [], length: 0, lengthSec: 0, meta: null });

function fakeDeps(specs, { failTransitionFor = null } = {}) {
  return {
    makeClip: async (trackId) => {
      const s = specs[trackId];
      return fakeClip(trackId, s.bpm, s.secs, s.grid !== false);
    },
    beatmatch: async (doc, clipId) => {
      const sorted = E.sortedClips(doc);
      const b = E.clipOf(doc, clipId);
      const a = sorted[sorted.indexOf(b) - 1];
      if (!a) throw "there is no earlier clip to match to";
      const entry = Math.min(Math.max(b.visibleStart - a.start, a.trimStart), a.length - a.trimEnd);
      const target = E.effectiveBpm(a, entry);
      const tempo = [{ pos: 0, speed: target / b.bpm }];
      const r = E.retempoClip(b, tempo);
      return { clipId, referenceClipId: a.id, tempo, start: r.start, length: r.length, trimStart: r.trimStart, trimEnd: r.trimEnd, targetBpm: target, speed: target / b.bpm, shift: 0, anchorBeat: 0, referenceBeat: 0, barAligned: false, overlaps: true, notes: [] };
    },
    autoTransition: async (doc, fromId, toId, bars) => {
      if (failTransitionFor === toId) throw "the clips do not overlap (move the later clip earlier)";
      const a = E.clipOf(doc, fromId);
      const b = E.clipOf(doc, toId);
      const bpm = E.effectiveBpm(b, b.trimStart);
      const bar = (60 / bpm) * 4 * SR;
      const t0 = b.visibleStart;
      const avail = Math.min(a.visibleEnd, b.visibleEnd) - t0;
      if (avail < bar / 8) throw "the clips do not overlap (move the later clip earlier)";
      const length = Math.min(bars * bar, Math.floor(avail / bar) * bar || avail);
      return { transition: E.newTransition(a.id, b.id, t0, length), tools: [], bars: length / bar, beat: bar / 4, barsAvailable: avail / bar, notes: [] };
    },
  };
}

const kit = {
  SR,
  addClip: E.addClip,
  setClipBpm: E.setClipBpm,
  resizeClip: (d, id, edge, pos) => E.resizeClip(d, id, edge, pos),
  applyBeatmatch: E.applyBeatmatch,
  applyTransition: E.applyTransition,
  clipOf: E.clipOf,
  effectiveBpm: E.effectiveBpm,
  recipe: (doc, base, recipe, cutAt) => R.buildRecipe(doc, base, recipe, { cutAt }, E.recipeKit),
};

const bopts = (extra = {}) => ({ beatmatch: true, maxTrackSecs: null, bars: 8, recipe: "bassSwap", cutAt: "end", ...extra });

/** What mix_validate checks that matters here: clips on one deck do not overlap. */
function overlapsOnADeck(doc) {
  const out = [];
  for (const a of doc.clips) for (const b of doc.clips) {
    if (a.id < b.id && a.channel === b.channel && a.visibleStart < b.visibleEnd - 1 && b.visibleStart < a.visibleEnd - 1) out.push([a.id, b.id]);
  }
  return out;
}

await test("building a mix: clips overlap, alternate decks, are joined and beat-matched to the tempo plan", async () => {
  const specs = { 1: { bpm: 124, secs: 200 }, 2: { bpm: 126, secs: 210 }, 3: { bpm: 128, secs: 190 }, 4: { bpm: 125, secs: 220 } };
  const t = [tr(1, 124, "8A"), tr(2, 126, "8A"), tr(3, 128, "9A"), tr(4, 125, "9A")];
  const plan = S.planMix(t, () => true, { beatmatch: true, mixTempo: null, halfDouble: true });
  const seen = [];
  const r = await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts(), fakeDeps(specs), kit, (d, tot, title) => seen.push([d, tot, title]));
  assert.equal(r.doc.clips.length, 4);
  assert.equal(r.doc.transitions.length, 3);
  assert.deepEqual(r.clipIds.map((id) => E.clipOf(r.doc, id).trackId), [1, 2, 3, 4], "in play order");
  assert.deepEqual(r.clipIds.map((id) => E.clipOf(r.doc, id).channel), [0, 1, 0, 1]);
  assert.deepEqual(overlapsOnADeck(r.doc), []);
  // each transition joins two neighbours, and the later clip starts before the earlier one ends
  for (const tr_ of r.doc.transitions) {
    const a = E.clipOf(r.doc, tr_.between.from);
    const b = E.clipOf(r.doc, tr_.between.to);
    assert.ok(b.visibleStart < a.visibleEnd, "overlap");
    assert.equal(r.clipIds.indexOf(b.id), r.clipIds.indexOf(a.id) + 1);
  }
  // the whole mix runs at the first track's tempo, as planned
  r.clipIds.forEach((id, i) => close(E.effectiveBpm(E.clipOf(r.doc, id), E.clipOf(r.doc, id).trimStart), plan.tempos[i], 1e-3, `tempo ${i}`));
  r.stretchPct.forEach((p, i) => close(p, plan.stretchPct[i], 1e-3, `stretch ${i}`));
  assert.equal(r.toolsSkipped, 0);
  assert.deepEqual(seen[0], [0, 4, "T1"]);
  assert.equal(seen.at(-1)[0], 4);
  // the first clip begins at 0 on deck 0
  assert.equal(E.clipOf(r.doc, r.clipIds[0]).visibleStart, 0);
});

await test("building a mix: the first track's tempo and the longest play time", async () => {
  const specs = { 1: { bpm: 120, secs: 300 }, 2: { bpm: 122, secs: 300 }, 3: { bpm: 124, secs: 300 } };
  const t = [tr(1, 120, "8A"), tr(2, 122, "8A"), tr(3, 124, "8A")];
  const plan = S.planMix(t, () => true, { beatmatch: true, mixTempo: 125, halfDouble: true });
  const r = await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts({ maxTrackSecs: 100 }), fakeDeps(specs), kit);
  const first = E.clipOf(r.doc, r.clipIds[0]);
  close(E.effectiveBpm(first, first.trimStart), 125, 1e-3);
  for (const id of r.clipIds) {
    const c = E.clipOf(r.doc, id);
    const dur = (c.visibleEnd - c.visibleStart) / SR;
    assert.ok(dur <= 100 + 1e-6, `plays for ${dur} s`);
    const bar = (60 / E.effectiveBpm(c, c.trimStart)) * 4 * SR;
    close((c.visibleEnd - c.visibleStart) / bar, Math.round((c.visibleEnd - c.visibleStart) / bar), 1e-6, "a whole number of bars");
  }
  assert.deepEqual(overlapsOnADeck(r.doc), []);
  assert.equal(r.doc.transitions.length, 2);
});

await test("building a mix: hops that cannot blend start after the last track and say why", async () => {
  const specs = { 1: { bpm: 124, secs: 200 }, 2: { bpm: 125, secs: 200, grid: false }, 3: { bpm: 126, secs: 200 }, 4: { bpm: 126, secs: 200 }, 5: { bpm: 63, secs: 200 } };
  const t = [tr(1, 124, "8A"), tr(2, 125, "8A"), tr(3, 126, "8A"), tr(4, 126, "8A"), tr(5, 63, "8A")];
  const plan = S.planMix(t, (id) => specs[id].grid !== false, { beatmatch: true, mixTempo: null, halfDouble: true });
  assert.deepEqual(plan.pairs.map((p) => p.mode), ["sequence", "sequence", "blend", "sequence"]);
  const failing = fakeDeps(specs, { failTransitionFor: "clip-4" });
  const r = await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts(), failing, kit);
  // hops 1-2 and 2-3 (no grid) and 4-5 (half time) are sequences; 3-4 should have blended but the backend refused
  assert.equal(r.doc.transitions.length, 0);
  assert.equal(r.doc.clips.length, 5);
  assert.deepEqual(overlapsOnADeck(r.doc), []);
  r.clipIds.slice(1).forEach((id, i) => {
    const prev = E.clipOf(r.doc, r.clipIds[i]);
    assert.equal(E.clipOf(r.doc, id).visibleStart, prev.visibleEnd, "starts when the last one ends");
  });
  assert.equal(r.notes.length, 4);
  assert.match(r.notes[0], /"T2".*no beat grid/);
  assert.match(r.notes[2], /"T4".*could not be blended.*do not overlap/);
  assert.match(r.notes[3], /"T5".*half or double/);
  // the half-time track kept its own tempo and the refused one was not stretched
  close(E.effectiveBpm(E.clipOf(r.doc, r.clipIds[4]), 0), 63, 1e-3);
  close(E.effectiveBpm(E.clipOf(r.doc, r.clipIds[3]), 0), 126, 1e-3);
});

await test("building a mix: a run that starts after a break is set to its own tempo", async () => {
  const specs = { 1: { bpm: 100, secs: 200, grid: false }, 2: { bpm: 120, secs: 240 }, 3: { bpm: 124, secs: 240 } };
  const t = [tr(1, 100, "8A"), tr(2, 120, "8A"), tr(3, 124, "8A")];
  const plan = S.planMix(t, (id) => specs[id].grid !== false, { beatmatch: true, mixTempo: null, halfDouble: true });
  assert.deepEqual(plan.tempos, [100, 122, 122]);
  const r = await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts(), fakeDeps(specs), kit);
  r.clipIds.forEach((id, i) => close(E.effectiveBpm(E.clipOf(r.doc, id), E.clipOf(r.doc, id).trimStart), plan.tempos[i], 1e-3, `tempo ${i}`));
  assert.equal(r.doc.transitions.length, 1);
  assert.equal(E.clipOf(r.doc, r.clipIds[1]).visibleStart, E.clipOf(r.doc, r.clipIds[0]).visibleEnd);
});

await test("building a mix without beat-matching leaves tempos alone", async () => {
  const specs = { 1: { bpm: 120, secs: 240 }, 2: { bpm: 126, secs: 240 } };
  const t = [tr(1, 120, "8A"), tr(2, 126, "8A")];
  const plan = S.planMix(t, () => true, { beatmatch: false, mixTempo: null, halfDouble: true });
  const r = await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts({ beatmatch: false }), fakeDeps(specs), kit);
  close(E.effectiveBpm(E.clipOf(r.doc, r.clipIds[1]), 0), 126, 1e-6);
  assert.equal(r.doc.transitions.length, 1);
  assert.deepEqual(r.stretchPct.map((x) => Math.round(x)), [0, 0]);
});

await test("building a one-track mix and an empty one", async () => {
  const specs = { 1: { bpm: 120, secs: 240 } };
  const plan = S.planMix([tr(1, 120, "8A")], () => true, { beatmatch: true, mixTempo: null, halfDouble: true });
  const r = await B.buildSetDoc(emptyDoc(), [{ id: 1, title: "T1" }], plan, bopts(), fakeDeps(specs), kit);
  assert.equal(r.doc.clips.length, 1);
  assert.equal(r.doc.transitions.length, 0);
  const e = await B.buildSetDoc(emptyDoc(), [], { pairs: [], tempos: [], stretchPct: [] }, bopts(), fakeDeps(specs), kit);
  assert.equal(e.doc.clips.length, 0);
});

// ---- review fixes (E3 verifier findings) -------------------------------------------------------

/** A repeatable pool of `count` invented tracks: tempos 110..140, any key, 2 to 7 minutes. */
function bigPool(count, withEnergy = false) {
  let x = 12345;
  const rnd = () => ((x = (x * 1103515245 + 12345) % 2147483648) / 2147483648);
  return Array.from({ length: count }, (_, i) => tr(i + 1, 110 + Math.floor(rnd() * 30), `${1 + Math.floor(rnd() * 12)}${rnd() < 0.5 ? "A" : "B"}`, 120 + Math.floor(rnd() * 300), withEnergy ? 1 + Math.floor(rnd() * 10) : null));
}

await test("a target length on a large pool finishes quickly and lands near the target", () => {
  for (const [count, energy] of [[600, false], [250, true]]) {
    const pool = bigPool(count, energy);
    const target = 3600;
    const o = O({ targetSecs: target, curve: energy ? "arc" : "off" });
    const t0 = performance.now();
    const r = S.solve(pool, new Map(), o);
    const ms = performance.now() - t0;
    assert.ok(ms < 12000, `${count} tracks took ${Math.round(ms)} ms`);
    assert.equal(r.order.length + r.dropped.length, count);
    assert.deepEqual([...r.order, ...r.dropped].sort((a, b) => a - b), pool.map((t) => t.id));
    // the drop loop stops when no further drop gets closer, so the total is within one track of the target
    assert.ok(Math.abs(r.secs - target) <= 420, `total ${r.secs}`);
  }
});

await test("dropping for length is deterministic and keeps pinned tracks", () => {
  const pool = bigPool(80);
  const o = O({ targetSecs: 40 * 60 });
  const pins = new Map([[3, 0], [41, 4]]);
  const a = S.solve(pool, pins, o);
  const b = S.solve(pool, pins, o);
  assert.deepEqual(a, b);
  assert.ok(a.order.includes(3) && a.order.includes(41));
  assert.equal(a.order[0], 3);
  assert.equal(a.order[4], 41);
});

await test("pin positions are counted over the tracks the pool has", () => {
  const order = [10, 11, 12, 13, 14];
  const pinned = new Set([13, 14]);
  const all = S.pinPositions(order, pinned, () => true);
  assert.deepEqual([...all], [[13, 3], [14, 4]]);
  // 11 is no longer in the library: the pins after it move up one, matching the pool the solver gets
  const lost = S.pinPositions(order, pinned, (id) => id !== 11);
  assert.deepEqual([...lost], [[13, 2], [14, 3]]);
  const pool = [10, 12, 13, 14].map((id, i) => tr(id, 120 + i, `${1 + i}A`));
  const r = S.solve(pool, lost, O());
  assert.equal(r.order[2], 13);
  assert.equal(r.order[3], 14);
});

await test("a rejected setting keeps the value it had; number settings clamp", () => {
  const cur = S.mergePrefs({ maxBpmJumpPct: 3, maxTrackMins: 5, targetMins: 60, mixTempo: 126, blendJumpPct: 4 });
  const bad = S.mergePrefs({ ...cur, maxBpmJumpPct: 60, maxTrackMins: 0.1, targetMins: 0, mixTempo: 5, blendJumpPct: 99, overlapCapPct: 1 }, cur);
  assert.equal(bad.maxBpmJumpPct, 3);
  assert.equal(bad.maxTrackMins, 5);
  assert.equal(bad.targetMins, 60);
  assert.equal(bad.mixTempo, 126);
  assert.equal(bad.blendJumpPct, 4);
  assert.equal(bad.overlapCapPct, 40);
  // null still unsets a setting that may be unset
  assert.equal(S.mergePrefs({ ...cur, maxTrackMins: null }, cur).maxTrackMins, null);
  // saved values are still checked against the defaults
  assert.deepEqual(S.mergePrefs({ maxBpmJumpPct: 60 }), S.DEFAULT_PREFS);
  assert.equal(S.mergePrefs({ onToolConflict: "ask" }).onToolConflict, "skip");
  assert.equal(S.mergePrefs({ onToolConflict: "replace" }).onToolConflict, "replace");
  // clamping for the inputs
  assert.equal(S.clampPref("maxBpmJumpPct", 60), 50);
  assert.equal(S.clampPref("maxTrackMins", 0.1), 0.25);
  assert.equal(S.clampPref("mixTempo", 5), 40);
  assert.equal(S.clampPref("targetMins", 90), 90);
  for (const k of Object.keys(S.PREF_RANGES)) {
    const [lo, hi] = S.PREF_RANGES[k];
    assert.ok(S.mergePrefs({ [k]: S.clampPref(k, 1e9) })[k] === hi && S.mergePrefs({ [k]: S.clampPref(k, -1e9) })[k] === lo, k);
  }
});

await test("the mix plan does not beat-match a hop beyond the tempo limit", () => {
  const all = () => true;
  const t = [tr(1, 80, "8A"), tr(2, 82, "8A"), tr(3, 140, "8A"), tr(4, 143, "8A")];
  const free = S.planMix(t, all, { beatmatch: true, mixTempo: null, halfDouble: false });
  assert.deepEqual(free.pairs.map((p) => p.mode), ["blend", "blend", "blend"], "no limit: the old behaviour");
  const capped = S.planMix(t, all, { beatmatch: true, mixTempo: null, halfDouble: false, maxJumpPct: 6 });
  assert.deepEqual(capped.pairs.map((p) => [p.mode, p.why]), [["blend", null], ["sequence", "bigJump"], ["blend", null]]);
  assert.equal(capped.pairs[1].culprit, 3);
  close(capped.pairs[1].jumpPct, ((140 - 82) / 82) * 100);
  // each side of the break runs at its own tempo; nothing is stretched across the jump
  assert.deepEqual(capped.tempos, [81, 81, 141.5, 141.5]);
  assert.ok(capped.stretchPct.every((x) => Math.abs(x) < 2), capped.stretchPct.join());
  // exactly at the limit still blends
  const edge = S.planMix([tr(1, 100, "8A"), tr(2, 106, "8A")], all, { beatmatch: true, mixTempo: null, halfDouble: false, maxJumpPct: 6 });
  assert.equal(edge.pairs[0].mode, "blend");
  // without beat-matching nothing is stretched, so the limit does not apply
  const nb = S.planMix(t, all, { beatmatch: false, mixTempo: null, halfDouble: false, maxJumpPct: 6 });
  assert.deepEqual(nb.pairs.map((p) => p.mode), ["blend", "blend", "blend"]);
  // a half-time hop keeps its own reason
  const hd = S.planMix([tr(1, 140, "8A"), tr(2, 70, "8A")], all, { beatmatch: true, mixTempo: null, halfDouble: true, maxJumpPct: 6 });
  assert.equal(hd.pairs[0].why, "halfDouble");
});

await test("building a mix: a hop beyond the limit starts after the last track and says why", async () => {
  const specs = { 1: { bpm: 80, secs: 200 }, 2: { bpm: 140, secs: 200 } };
  const t = [tr(1, 80, "8A"), tr(2, 140, "8A")];
  const plan = S.planMix(t, () => true, { beatmatch: true, mixTempo: null, halfDouble: false, maxJumpPct: 6 });
  const r = await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts(), fakeDeps(specs), kit);
  assert.equal(r.doc.transitions.length, 0);
  assert.equal(E.clipOf(r.doc, r.clipIds[1]).visibleStart, E.clipOf(r.doc, r.clipIds[0]).visibleEnd);
  assert.match(r.notes[0], /"T2".*75\.0 % above.*limit for beat-matching/);
  close(r.stretchPct[1], 0, 1e-6);
});

await test("building a mix: the overlap cap and the tool-conflict choice are honoured", async () => {
  const specs = { 1: { bpm: 124, secs: 200 }, 2: { bpm: 124, secs: 200 } };
  const t = [tr(1, 124, "8A"), tr(2, 124, "8A")];
  const plan = S.planMix(t, () => true, { beatmatch: false, mixTempo: null, halfDouble: true });
  const overlapOf = async (extra) => {
    const r = await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts({ beatmatch: false, bars: 64, ...extra }), fakeDeps(specs), kit);
    return (E.clipOf(r.doc, r.clipIds[0]).visibleEnd - E.clipOf(r.doc, r.clipIds[1]).visibleStart) / SR;
  };
  close(await overlapOf({}), 0.4 * 200, 1e-3, "default 40 %");
  close(await overlapOf({ overlapMaxFrac: 0.1 }), 0.1 * 200, 1e-3, "10 %");
  const flags = [];
  const spy = { ...kit, applyTransition: (d, s, replace) => (flags.push(replace), kit.applyTransition(d, s, replace)) };
  await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts({ beatmatch: false }), fakeDeps(specs), spy);
  await B.buildSetDoc(emptyDoc(), t.map((x) => ({ id: x.id, title: `T${x.id}` })), plan, bopts({ beatmatch: false, replaceConflicts: true }), fakeDeps(specs), spy);
  assert.deepEqual(flags, [false, true]);
});

// ---- sections: leave on an outro, arrive on an intro --------------------------------------------

/** A track with the section flags the set builder derives (no flags = no sections detected). */
const withSides = (t, sides) => (sides ? { ...t, intro: sides.intro, outro: sides.outro } : t);

await test("section preference: off by default, never changes a score, says what a hop does", () => {
  assert.equal(S.DEFAULT_SOLVER.sectionPref, false);
  assert.equal(S.DEFAULT_PREFS.preferSections, false);
  const none = { intro: false, outro: false };
  const a = withSides(tr(1, 124, "8A"), { intro: false, outro: true });
  const b = withSides(tr(2, 124, "8A"), { intro: true, outro: false });
  const plain = S.transitionBetween(a, b, O());
  assert.equal(plain.sectionFit, null, "preference off");
  const on = S.transitionBetween(a, b, O({ sectionPref: true }));
  assert.deepEqual(on.sectionFit, { out: true, in: true });
  assert.equal(on.score, plain.score, "sections never change the score");
  assert.deepEqual(on.concerns, plain.concerns);
  assert.equal(S.sectionFitText(on), "outro into intro");
  assert.equal(S.sectionFitText(S.transitionBetween(a, withSides(tr(3, 124, "8A"), none), O({ sectionPref: true }))), "leaves on an outro");
  assert.equal(S.sectionFitText(S.transitionBetween(withSides(tr(3, 124, "8A"), none), b, O({ sectionPref: true }))), "arrives on an intro");
  assert.equal(S.sectionFitText(S.transitionBetween(withSides(tr(3, 124, "8A"), none), withSides(tr(4, 124, "8A"), none), O({ sectionPref: true }))), "");
  // tracks without sections: nothing to say, on or off
  const bare = S.transitionBetween(tr(5, 124, "8A"), tr(6, 124, "8A"), O({ sectionPref: true }));
  assert.equal(bare.sectionFit, null);
  assert.equal(S.sectionFitText(bare), "");
  assert.equal(S.sectionFitText({ sectionFit: null }), "");
});

await test("section preference: a tie-breaker between hops that are otherwise alike, no penalty without sections", () => {
  // four tracks that match each other perfectly: only sections tell the orders apart
  const A = withSides(tr(1, 124, "8A"), { intro: false, outro: true });
  const B = withSides(tr(2, 124, "8A"), { intro: true, outro: false });
  const C = tr(3, 124, "8A");
  const D = tr(4, 124, "8A");
  const tracks = [A, B, C, D];
  const off = O();
  const on = O({ sectionPref: true });
  // preference off: every order costs the same, with or without section data
  assert.equal(costOfOrder(tracks, [1, 2, 3, 4], off), costOfOrder(tracks, [2, 1, 3, 4], off));
  // on: outro into intro is cheaper by the bonus of both sides, and nothing else is penalised
  const good = costOfOrder(tracks, [1, 2, 3, 4], on);
  const bad = costOfOrder(tracks, [2, 1, 3, 4], on);
  // (A still leaves on its outro in the second order, but B has nothing to arrive from: one bonus is lost)
  assert.equal(good, costOfOrder(tracks, [1, 2, 3, 4], off) - 2 * S.SECTION_BONUS);
  assert.equal(bad - good, S.SECTION_BONUS);
  assert.equal(costOfOrder([C, D], [3, 4], on), costOfOrder([C, D], [3, 4], off), "no sections: same cost either way");
  // the solver puts the outro track right before the intro track
  const r = S.solve(tracks, new Map(), on);
  assert.equal(r.order.length, 4);
  assert.equal(r.order[r.order.indexOf(1) + 1], 2, `order ${r.order}`);
});

await test("section preference never trades a key or tempo limit for an outro", () => {
  const on = O({ sectionPref: true });
  // 1 (outro) -> 2 (intro) is two steps on the wheel (not allowed); 1 -> 3 -> 2 keeps every limit
  const A = withSides(tr(1, 124, "8A"), { intro: false, outro: true });
  const B = withSides(tr(2, 124, "10A"), { intro: true, outro: false });
  const C = tr(3, 124, "9A");
  const clean = S.solve([A, B, C], new Map(), on);
  assert.equal(S.evaluate(byIdOf([A, B, C]), clean.order, on).violations, 0, `order ${clean.order}`);
  // both bonuses together are far below the cost of a broken limit
  assert.ok(2 * S.SECTION_BONUS < 60);
  // tempo limit likewise
  const E1 = withSides(tr(1, 124, "8A"), { intro: false, outro: true });
  const E2 = withSides(tr(2, 132, "8A"), { intro: true, outro: false });
  const E3 = tr(3, 128, "8A");
  const r2 = S.solve([E1, E2, E3], new Map(), on);
  assert.equal(S.evaluate(byIdOf([E1, E2, E3]), r2.order, on).violations, 0, `order ${r2.order}`);
});

await test("section preference: saved setting and the options built from it", () => {
  assert.equal(S.mergePrefs({ preferSections: true }).preferSections, true);
  assert.equal(S.mergePrefs({ preferSections: "yes" }).preferSections, false);
  assert.equal(S.mergePrefs(undefined).preferSections, false);
  assert.equal(S.solverOptions({ ...S.DEFAULT_PREFS, preferSections: true }).sectionPref, true);
  assert.equal(S.solverOptions(S.DEFAULT_PREFS).sectionPref, false);
});

console.log(`\n${n} set builder tests passed`);
