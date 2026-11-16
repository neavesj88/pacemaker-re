// Run: node --experimental-strip-types src/lib/state/toastText.test.mjs   (from app/)
import assert from "node:assert/strict";
import { autoMs, condense, errorMessage, errorToast, firstClause, TOAST_HARD, wordCount } from "./toastText.ts";
import { addToast, MAX_TOASTS } from "./appearanceLogic.ts";

let n = 0;
const test = (name, fn) => {
  fn();
  n++;
  console.log(`ok  ${name}`);
};

test("words are counted by whitespace", () => {
  assert.equal(wordCount("  Saved  the mix "), 3);
  assert.equal(wordCount(""), 0);
});

test("the first clause stops at a full stop or semicolon and keeps colons", () => {
  assert.equal(firstClause("disk is full. Free some space and try again."), "disk is full");
  assert.equal(firstClause("Not matched: +25 % stretch is over your 20% limit. Use Sync."), "Not matched: +25 % stretch is over your 20% limit");
  assert.equal(firstClause("a b c d e f g h i j k l", 5), "a b c d e…");
  assert.equal(firstClause("tempo 127.5 is fine (really)"), "tempo 127.5 is fine");
});

test("an error is its head plus the first clause of the reason, the whole reason is the details", () => {
  const r = errorToast("Couldn't save the mix.", "Error: the folder is read-only. Pick another place; then try again.");
  assert.equal(r.text, "Couldn't save the mix. The folder is read-only.");
  assert.equal(r.details, "the folder is read-only. Pick another place; then try again.");
  assert.equal(errorToast("Couldn't eject", new Error("busy")).text, "Couldn't eject. Busy.");
  // a reason that is only the head again adds nothing
  assert.equal(errorToast("Couldn't save the mix.", "couldn't save the mix").text, "Couldn't save the mix.");
  // a long reason is cut to what fits under the limit
  const long = errorToast("Couldn't export the mix.", "one two three four five six seven eight nine ten eleven twelve thirteen");
  assert.ok(wordCount(long.text) <= TOAST_HARD, long.text);
  assert.match(long.details, /thirteen$/);
  assert.equal(errorMessage("invalid: Enter a tempo"), "Enter a tempo");
});

test("text over 15 words keeps its first sentence and moves the rest to the details", () => {
  assert.deepEqual(condense("Saved."), { text: "Saved." });
  const text = "Backed up 120 files (3.1 GB) to the folder. Every file was copied and its checksum compared after writing, so you can trust it.";
  const c = condense(text);
  assert.equal(c.text, "Backed up 120 files (3.1 GB) to the folder.");
  assert.equal(c.details, text);
  // one long sentence is cut at its last comma inside 15 words
  const one = condense("Deleted 12 files (4.2 MB), 2 left alone because they are no longer unused, stopped early at your request, 1 could not be deleted");
  assert.equal(one.text, "Deleted 12 files (4.2 MB), 2 left alone because they are no longer unused.");
  assert.ok(wordCount(one.text) <= TOAST_HARD);
  // no comma to cut at: 15 words and an ellipsis
  const nocomma = condense("one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen");
  assert.equal(wordCount(nocomma.text), 15);
  assert.ok(nocomma.text.endsWith("…"));
});

test("a short message stays 3 s, a long one up to 7 s", () => {
  assert.equal(autoMs("Saved"), 3000);
  assert.equal(autoMs("a b c d e f g h i j k l"), 6100);
  assert.equal(autoMs(Array(40).fill("w").join(" ")), 7000);
});

test("a message with a key replaces the card with that key, whatever its text", () => {
  let items = [];
  items = addToast(items, { id: 1, text: "Added 1 track to “A”.", kind: "ok", key: "case-add" });
  items = addToast(items, { id: 2, text: "Saved", kind: "ok" });
  items = addToast(items, { id: 3, text: "Added 2 tracks to “A”.", kind: "ok", key: "case-add" });
  assert.deepEqual(items.map((t) => t.text), ["Saved", "Added 2 tracks to “A”."]);
  assert.equal(items[1].count, 1, "a changed text starts its own count");
  items = addToast(items, { id: 4, text: "Added 2 tracks to “A”.", kind: "ok", key: "case-add" });
  assert.equal(items[1].count, 2, "the same text again counts up");
  assert.equal(MAX_TOASTS, 3);
});

console.log(`${n} passed`);
