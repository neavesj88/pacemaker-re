// Run: node --experimental-strip-types src/lib/shell/bannerLogic.test.mjs
import assert from "node:assert/strict";
import { errorScope } from "./bannerLogic.ts";

let seen = { text: "", view: "" };
const step = (text, view) => {
  const r = errorScope(seen, text, view);
  seen = r.next;
  return r.clear;
};

assert.equal(step("", "tracks"), false, "no message, nothing to clear");
assert.equal(step("No tracks to detect a key for.", "analysis"), false, "a message stays on the view it came from");
assert.equal(step("No tracks to detect a key for.", "analysis"), false, "and on later checks of the same view");
assert.equal(step("No tracks to detect a key for.", "tracks"), true, "going to another view clears it");
seen = { text: "", view: "tracks" };
assert.equal(step("Another message", "tracks"), false, "a message that appears after the move stays");
assert.equal(step("", "mixes"), false);
assert.equal(step("Same words again", "mixes"), false, "the same words later are a new message on this view");

console.log("bannerLogic.test.mjs ok");
