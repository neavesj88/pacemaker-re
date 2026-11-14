// Run: node --experimental-strip-types src/lib/state/loadSeq.test.mjs
import assert from "node:assert/strict";
import { LoadSeq } from "./loadSeq.ts";

// a plain switch
{
  const q = new LoadSeq();
  const t = q.begin(false);
  assert.ok(q.switching);
  assert.ok(q.current(t));
  assert.equal(q.end(t), true);
  assert.equal(q.switching, false);
}

// a job's reload of the old library that finishes AFTER the user's switch must not win
{
  const q = new LoadSeq();
  const reload = q.begin(true);
  const sw = q.begin(false);
  assert.equal(q.current(reload), false, "the reload is stale as soon as a switch starts");
  assert.equal(q.end(reload), false, "and its end does not clear the switch's reading state");
  assert.ok(q.switching);
  assert.ok(q.current(sw));
  assert.equal(q.end(sw), true);
}

// a reload that would START while a switch is being read is refused
{
  const q = new LoadSeq();
  const sw = q.begin(false);
  assert.equal(q.begin(true), null);
  assert.ok(q.current(sw), "the refused reload did not overtake the switch");
  q.end(sw);
  assert.notEqual(q.begin(true), null, "allowed again once the switch is done");
}

// two switches: the last one the user asked for wins, even if the first finishes last
{
  const q = new LoadSeq();
  const a = q.begin(false);
  const b = q.begin(false);
  assert.equal(q.current(a), false);
  assert.ok(q.current(b));
  assert.equal(q.end(b), true);
  assert.equal(q.end(a), false);
}

console.log("loadSeq.test.mjs ok");
