// Run: node --experimental-strip-types --test src/lib/player/hoverCue.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { cueTime, decideSpace, hoverLabel, timelineSecs, waveSecs } from "./hoverCue.ts";

const base = { surface: null, typing: false, ownedByControl: false, mixPlayingElsewhere: false };

test("no hover: Space keeps its play / pause rules", () => {
  assert.equal(decideSpace(base), "default");
});

test("over the mix timeline or the player wave Space moves the cue", () => {
  assert.equal(decideSpace({ ...base, surface: "mix" }), "move-mix-cue");
  assert.equal(decideSpace({ ...base, surface: "player" }), "move-player-cue");
});

test("a text field or a control that wants Space keeps it, hover or not", () => {
  assert.equal(decideSpace({ ...base, surface: "mix", typing: true }), "default");
  assert.equal(decideSpace({ ...base, surface: "player", ownedByControl: true }), "default");
});

test("a mix playing on another tab: Space still pauses it over the player wave, but the timeline is the mix itself", () => {
  assert.equal(decideSpace({ ...base, surface: "player", mixPlayingElsewhere: true }), "default");
  assert.equal(decideSpace({ ...base, surface: "mix", mixPlayingElsewhere: true }), "move-mix-cue");
});

test("timeline time: header column is no time, scroll and zoom shift it", () => {
  assert.equal(timelineSecs(100, 0, 132, 0, 24), null);
  assert.equal(timelineSecs(132, 0, 132, 0, 24), 0);
  assert.equal(timelineSecs(132 + 240, 0, 132, 0, 24), 10);
  // scrolled 480 px: the same screen spot is 20 s later
  assert.equal(timelineSecs(132 + 240, 0, 132, 480, 24), 30);
  // the scroller does not start at the window edge
  assert.equal(timelineSecs(50 + 132 + 240, 50, 132, 0, 24), 10);
  assert.equal(timelineSecs(500, 0, 132, 0, 0), null);
});

test("player time: fraction of the width, clamped, offset by where the file starts", () => {
  assert.equal(waveSecs(150, 100, 200, 0, 240), 60);
  assert.equal(waveSecs(50, 100, 200, 0, 240), 0);
  assert.equal(waveSecs(900, 100, 200, 0, 240), 240);
  assert.equal(waveSecs(150, 100, 200, 30, 240), 90);
  assert.equal(waveSecs(150, 100, 0, 0, 240), null);
  assert.equal(waveSecs(150, 100, 200, 0, 0), null);
});

test("cue snaps to the nearest beat only when snap is on, Alt is up and a grid exists", () => {
  const beats = [10, 10.5, 11, 11.5];
  assert.equal(cueTime(10.8, beats, true, false), 11);
  assert.equal(cueTime(10.7, beats, true, false), 10.5);
  assert.equal(cueTime(10.8, beats, true, true), 10.8); // Alt = exact
  assert.equal(cueTime(10.8, beats, false, false), 10.8); // snap off
  assert.equal(cueTime(10.8, [], true, false), 10.8); // no grid
});

test("hairline label shows tenths", () => {
  assert.equal(hoverLabel(0), "0:00.0");
  assert.equal(hoverLabel(75.25), "1:15.3");
  assert.equal(hoverLabel(59.96), "1:00.0");
  assert.equal(hoverLabel(-3), "0:00.0");
});
