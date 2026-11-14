// Run: node --experimental-strip-types --test src/lib/player/spaceClaim.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { claimSpace, playerOwnsSpace, spaceClaimed } from "./spaceClaim.ts";

test("the player owns Space when nothing handled it and nothing claims it", () => {
  assert.equal(spaceClaimed(), false);
  assert.equal(playerOwnsSpace(false), true);
});

test("a press an element already handled (a table row) is not toggled a second time", () => {
  assert.equal(playerOwnsSpace(true), false);
});

test("a view claim holds while it says yes and goes away when removed", () => {
  let open = true;
  const drop = claimSpace(() => open);
  assert.equal(playerOwnsSpace(false), false);
  open = false;
  assert.equal(playerOwnsSpace(false), true);
  open = true;
  assert.equal(playerOwnsSpace(false), false);
  drop();
  assert.equal(playerOwnsSpace(false), true);
});

test("two claims: either one is enough, and removing one keeps the other", () => {
  const a = claimSpace(() => true);
  const b = claimSpace(() => true);
  a();
  assert.equal(spaceClaimed(), true);
  b();
  assert.equal(spaceClaimed(), false);
});
