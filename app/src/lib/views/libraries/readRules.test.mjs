// Run: node --experimental-strip-types --test src/lib/views/libraries/readRules.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { invalidateReads, readRight } from "./readRules.ts";
import { readsAtOnce } from "./librariesLogic.ts";

const COMP = "C:\Users\demo\Music\Pacemaker Revival";
const PM = "G:\\";
const VOLUMES = [PM];
const pane = (root) => ({ root, kind: root === PM ? "pacemaker" : "computer", tracks: [], mixes: [], cases: [] });

/** A host like the store (its fields), and a reader stub that records every read and lets the test
 *  finish each one when it chooses. */
function setup(rightRoot, folders = []) {
  const reads = [];
  const host = {
    rightRoot, reading: "", readError: "", loaded: new Map(), again: "", cleared: 0,
    clearRightSelection() { this.cleared++; },
  };
  const deps = {
    fetchPane: (root) => new Promise((resolve, reject) => reads.push({ root, resolve: () => resolve(pane(root)), reject })),
    readsAtOnce: (root) => readsAtOnce(root, VOLUMES, folders, false),
  };
  const tick = () => new Promise((r) => setImmediate(r));
  return { host, deps, reads, tick };
}

test("invalidating re-reads a library on this computer at once, and forgets what was read", async () => {
  const { host, deps, reads, tick } = setup(COMP);
  host.loaded = new Map([["old", pane("old")]]);
  host.readError = "earlier failure";
  invalidateReads(host, deps);
  assert.equal(host.loaded.size, 0, "what was read is dropped");
  assert.equal(host.readError, "", "and an old error with it");
  assert.equal(host.cleared, 1, "the right pane's selection is emptied");
  assert.deepEqual(reads.map((r) => r.root), [COMP], "the computer library is read again without a click");
  reads[0].resolve();
  await tick();
  assert.equal(host.loaded.size, 1);
  assert.equal(host.reading, "");
});

test("invalidating re-reads a folder opened by hand at once, even on a drive root", async () => {
  const folder = "H:\Library";
  const { host, deps, reads } = setup(folder, [folder]);
  invalidateReads(host, deps);
  assert.deepEqual(reads.map((r) => r.root), [folder]);
  // a drive root added by hand is also read at once (it is not a detected Pacemaker any more)
  const { host: h2, deps: d2, reads: r2 } = setup(PM, [PM]);
  invalidateReads(h2, d2);
  assert.deepEqual(r2.map((r) => r.root), [PM]);
});

test("invalidating never reads a detected Pacemaker: it waits for its Read button", async () => {
  const { host, deps, reads, tick } = setup(PM);
  host.loaded = new Map([["g:", pane(PM)]]);
  invalidateReads(host, deps);
  await tick();
  assert.deepEqual(reads, [], "no read was started");
  assert.equal(host.loaded.size, 0);
  assert.equal(host.reading, "");
  // nothing chosen on the right: nothing to read
  const none = setup("");
  invalidateReads(none.host, none.deps);
  assert.deepEqual(none.reads, []);
});

test("keeping the selection leaves the right pane's picks alone", () => {
  const { host, deps } = setup(PM);
  invalidateReads(host, deps, true);
  assert.equal(host.cleared, 0);
});

test("a read asked for during a read is queued once and run after it when the right library is unchanged", async () => {
  const { host, deps, reads, tick } = setup(COMP);
  const first = readRight(host, deps, COMP);
  assert.equal(host.reading, COMP);
  // three more requests while it runs: none starts beside it, only one is kept
  void readRight(host, deps, COMP);
  void readRight(host, deps, COMP);
  void readRight(host, deps, COMP);
  assert.equal(reads.length, 1, "no second read runs beside the one in flight");
  assert.equal(host.again, COMP);
  reads[0].resolve();
  await first;
  await tick();
  assert.equal(reads.length, 2, "the dropped request runs once after the first");
  assert.equal(host.again, "", "and is not kept again");
  reads[1].resolve();
  await tick();
  await tick();
  assert.equal(reads.length, 2, "not a third time");
  assert.equal(host.reading, "");
  assert.equal(host.loaded.size, 1);
});

test("the queued read is dropped when the right library changed meanwhile", async () => {
  const { host, deps, reads, tick } = setup(COMP);
  const first = readRight(host, deps, COMP);
  void readRight(host, deps, COMP);
  host.rightRoot = PM; // the user picked another library while the read ran
  reads[0].resolve();
  await first;
  await tick();
  assert.equal(reads.length, 1, "nothing is read for the library that is no longer on the right");
  assert.equal(host.again, "");
  // no library on the right at all
  const s = setup(COMP);
  const f = readRight(s.host, s.deps, COMP);
  void readRight(s.host, s.deps, COMP);
  s.host.rightRoot = "";
  s.reads[0].resolve();
  await f;
  await s.tick();
  assert.equal(s.reads.length, 1);
});

test("the queued request is matched by place, not by spelling", async () => {
  const { host, deps, reads, tick } = setup("g:/");
  const first = readRight(host, deps, "g:/");
  void readRight(host, deps, "G:\\");
  reads[0].resolve();
  await first;
  await tick();
  assert.equal(reads.length, 2, "G:\ and g:/ are the same library");
});

test("a failed read keeps its message, ends the read, and still runs the queued one", async () => {
  const { host, deps, reads, tick } = setup(COMP);
  const first = readRight(host, deps, COMP);
  void readRight(host, deps, COMP);
  reads[0].reject("the drive went away");
  await first;
  await tick();
  assert.equal(reads.length, 2, "the queued read still runs");
  assert.equal(host.reading, COMP, "and is the one in flight now");
  reads[1].resolve();
  await tick();
  await tick();
  assert.equal(host.readError, "", "a read that works clears the message");
  assert.equal(host.loaded.size, 1);
});
