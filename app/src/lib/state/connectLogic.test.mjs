// Run: node --experimental-strip-types src/lib/state/connectLogic.test.mjs   (from app/)
import assert from "node:assert/strict";
import { CONNECT_ASK_DEFAULT, CONNECT_SECS_DEFAULT, CONNECT_SECS_MAX, EMPTY_STATE, answered, dismiss, driveText, normaliseAsk, normaliseSecs, step } from "./connectLogic.ts";

// settings
assert.equal(CONNECT_ASK_DEFAULT, true);
assert.equal(CONNECT_SECS_DEFAULT, 3);
assert.equal(normaliseAsk(null), true, "nothing saved: on");
assert.equal(normaliseAsk("false"), true, "junk: on");
assert.equal(normaliseAsk(false), false);
assert.equal(normaliseAsk(true), true);
assert.equal(normaliseSecs(null), 3);
assert.equal(normaliseSecs("5"), 3, "text is not a number");
assert.equal(normaliseSecs(NaN), 3);
assert.equal(normaliseSecs(0), 0, "0 = off");
assert.equal(normaliseSecs(2.6), 3, "whole seconds");
assert.equal(normaliseSecs(-4), 0);
assert.equal(normaliseSecs(99999), CONNECT_SECS_MAX);

// the first check only takes a baseline: a drive already plugged in is not "connected"
let s = step(EMPTY_STATE, ["E:\\"], null, true);
assert.deepEqual(s.pending, []);
assert.deepEqual(s.known, ["E:\\"]);

// a drive that was not there at the previous check is new
s = step(s, ["E:\\", "F:\\"], null, true);
assert.deepEqual(s.pending, ["F:\\"]);
// and is asked about once, not on every check
s = step(s, ["E:\\", "F:\\"], null, true);
assert.deepEqual(s.pending, ["F:\\"]);

// the open library is never asked about (spellings of one root are the same drive)
let t = step(step(EMPTY_STATE, [], "e:/", true), ["E:\\"], "e:/", true);
assert.deepEqual(t.pending, []);
// the library opened while its question was waiting: the question goes
t = step(step(step(EMPTY_STATE, [], null, true), ["G:\\"], null, true), ["G:\\"], "G:", true);
assert.deepEqual(t.pending, []);

// Not now: gone, and not asked again while the drive stays plugged in (E:, e:/ and E:\ are one drive)
s = dismiss(s, "f:");
assert.deepEqual(s.pending, []);
s = step(s, ["E:\\", "F:\\"], null, true);
assert.deepEqual(s.pending, []);
assert.deepEqual(s.dismissed, ["f:"]);
// unplugged: the dismissal is forgotten, so plugging it in again asks again
s = step(s, ["E:\\"], null, true);
assert.deepEqual(s.dismissed, []);
s = step(s, ["E:\\", "F:\\"], null, true);
assert.deepEqual(s.pending, ["F:\\"]);

// two drives at once queue in the order found; answering one keeps the other
s = step(step(EMPTY_STATE, [], null, true), ["G:\\", "H:\\"], null, true);
assert.deepEqual(s.pending, ["G:\\", "H:\\"]);
s = answered(s, "G:\\");
assert.deepEqual(s.pending, ["H:\\"]);
// an answered drive is not asked about again while it stays plugged in
s = step(s, ["G:\\", "H:\\"], null, true);
assert.deepEqual(s.pending, ["H:\\"]);
// a queued drive that is pulled out before the answer leaves the queue
s = step(s, ["G:\\"], null, true);
assert.deepEqual(s.pending, []);

// the setting off: nothing is queued, a waiting question goes, and the baseline still follows the drives
s = step(step(EMPTY_STATE, [], null, true), ["K:\\"], null, true);
assert.deepEqual(s.pending, ["K:\\"]);
s = step(s, ["K:\\"], null, false);
assert.deepEqual(s.pending, []);
s = step(s, ["K:\\", "L:\\"], null, false);
assert.deepEqual(s.pending, []);
// turned back on: drives plugged in meanwhile are not "new"
s = step(s, ["K:\\", "L:\\"], null, true);
assert.deepEqual(s.pending, []);

// the check list is not changed in place
const now = ["M:\\"];
const before = step(EMPTY_STATE, [], null, true);
const after = step(before, now, null, true);
now.push("N:\\");
assert.deepEqual(after.known, ["M:\\"]);
assert.deepEqual(before.pending, []);

// how a drive is shown
assert.equal(driveText("E:"), "E:\\");
assert.equal(driveText("e:/"), "e:\\");
assert.equal(driveText("/Volumes/PACEMAKER"), "/Volumes/PACEMAKER");
console.log("connectLogic: ok");
