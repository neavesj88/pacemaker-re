// Run: node --experimental-strip-types src/lib/state/volumePref.test.mjs   (from app/)
import assert from "node:assert/strict";
import { DEFAULT_VOLUME, normaliseVolume } from "./volumePref.ts";

assert.equal(normaliseVolume(null), DEFAULT_VOLUME, "nothing saved");
assert.equal(normaliseVolume(undefined), DEFAULT_VOLUME);
assert.equal(normaliseVolume("0.3"), DEFAULT_VOLUME, "text is not a level");
assert.equal(normaliseVolume(NaN), DEFAULT_VOLUME);
assert.equal(normaliseVolume(Infinity), DEFAULT_VOLUME);
assert.equal(normaliseVolume(0.35), 0.35, "a remembered level comes back as it was");
assert.equal(normaliseVolume(0), 0, "muted stays muted");
assert.equal(normaliseVolume(1), 1);
assert.equal(normaliseVolume(7), 1, "out of range is held to the slider");
assert.equal(normaliseVolume(-2), 0);
console.log("volumePref: ok");
