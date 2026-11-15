// Run: node --experimental-strip-types src/lib/views/tracks/needsLabel.test.mjs   (from app/)
import assert from "node:assert/strict";
import { analysisLabel, untried } from "./needsLabel.ts";

assert.equal(analysisLabel("Sections", "Find sections", 0, 32), "Find sections for 32 tracks…");
assert.equal(analysisLabel("Sections", "Find sections", 31, 32), "Find sections for 1 track…");
assert.equal(analysisLabel("Energy", "Estimate energy", 32, 32), "Energy: all 32 done…");
assert.equal(analysisLabel("Energy", "Estimate energy", 40, 32), "Energy: all 32 done…", "ready past total is still done");
assert.equal(untried({ none: 0, stale: 0 }), 0, "flat and skipped tracks are not work left");
assert.equal(untried({ none: 3, stale: 2 }), 5);
console.log("needsLabel: ok");
