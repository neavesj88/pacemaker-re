// Run: node --experimental-strip-types src/lib/state/prefStore.test.mjs
import assert from "node:assert/strict";
import { loadPref, savePref } from "./prefStore.ts";

function fakeStorage(opts = {}) {
  const data = new Map();
  return {
    data,
    getItem(k) {
      if (opts.blocked) throw new Error("blocked");
      return data.has(k) ? data.get(k) : null;
    },
    setItem(k, v) {
      if (opts.blocked || opts.full) throw new Error("quota");
      data.set(k, String(v));
    },
  };
}

const defaults = () => ({ n: 1, on: false });
const normalise = (raw) => (raw === null ? defaults() : { n: Number.isInteger(raw.n) ? raw.n : 1, on: raw.on === true });

// nothing saved: normalise gets null and gives the defaults
globalThis.localStorage = fakeStorage();
assert.deepEqual(loadPref("k", normalise, defaults), { n: 1, on: false });

// a saved value comes back through normalise, so a half-valid one is repaired, not trusted
assert.equal(savePref("k", { n: 5, on: true }), true);
assert.deepEqual(loadPref("k", normalise, defaults), { n: 5, on: true });
localStorage.setItem("k", JSON.stringify({ n: "x", on: "yes" }));
assert.deepEqual(loadPref("k", normalise, defaults), { n: 1, on: false });

// text that is not JSON, or a normalise that throws: the fallback, never an exception
localStorage.setItem("k", "{ not json");
assert.deepEqual(loadPref("k", normalise, defaults), { n: 1, on: false });
localStorage.setItem("k", "7");
assert.deepEqual(loadPref("k", () => { throw new Error("rejected"); }, () => "fallback"), "fallback");

// each call gets its own fallback object (callers mutate what they get)
assert.notEqual(loadPref("none", normalise, defaults), loadPref("none", normalise, defaults));

// blocked storage: reading gives the fallback, writing says false and does not throw
globalThis.localStorage = fakeStorage({ blocked: true });
assert.deepEqual(loadPref("k", normalise, defaults), { n: 1, on: false });
assert.equal(savePref("k", { n: 2 }), false);
globalThis.localStorage = fakeStorage({ full: true });
assert.equal(savePref("k", { n: 2 }), false);

// no storage object at all (a context without one)
delete globalThis.localStorage;
assert.deepEqual(loadPref("k", normalise, defaults), { n: 1, on: false });
assert.equal(savePref("k", {}), false);

console.log("prefStore: all tests passed");
