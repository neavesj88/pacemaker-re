// Tests for the pure editing logic of the mix editor (src/lib/state/mixedit.ts). No test runner
// is installed in this project, so this is a plain node script:
//
//   node --experimental-strip-types app/scripts/test-mixedit.mjs      (from the repo root)
//   node --experimental-strip-types scripts/test-mixedit.mjs          (from app/)
//
// It exits non-zero on the first failed assertion. The numbers in the TempoMap tests are the same
// ones the Rust tests of pm_formats::mix::TempoMap use, so the two ports stay in step.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as E from "../src/lib/state/mixedit.ts";
import * as R from "../src/lib/state/transitionRecipes.ts";
import * as L from "../src/lib/views/mixes/layout.ts";
import * as ML from "../src/lib/views/mixes/mixlist.ts";

const here = dirname(fileURLToPath(import.meta.url));
let n = 0;
const test = (name, fn) => {
  fn();
  n++;
  console.log(`ok  ${name}`);
};
const close = (a, b, eps = 1e-6, msg = "") => assert.ok(Math.abs(a - b) <= eps, `${msg} ${a} vs ${b}`);

// ---- fixtures ---------------------------------------------------------------------------------

const span = (start, length, extra = {}) => ({ start, length, trimStart: 0, trimEnd: 0, limitSize: false, startSec: 0, lengthSec: 0, ...extra });

function grid(bpm, first, count, firstDownbeat) {
  const step = (60 / bpm) * E.SR;
  return {
    sourceRate: E.SR,
    beatStartOffset: first,
    beatFrames: Array.from({ length: count }, (_, i) => Math.round(first + i * step)),
    beatBpm: Array.from({ length: count }, () => bpm),
    firstDownbeat,
  };
}

function clip(id, start, length, bpm, speed, g, extra = {}) {
  return {
    ...span(start, length, { limitSize: true }),
    id,
    trackId: 1,
    channel: 0,
    visibleStart: 0,
    visibleEnd: 0,
    visibleStartSec: 0,
    visibleEndSec: 0,
    bpm,
    tempo: [{ pos: 0, speed }],
    pitchMode: false,
    key: [{ pos: 0, cents: 0, flags: 1 }],
    gain: [{ pos: 0, value: 0 }, { pos: length - 1, value: 0 }],
    processes: [],
    filePath: "x.mp3",
    offset: 0,
    clipName: "AudioClip",
    clipBypassed: false,
    toniumId: "",
    otherIds: {},
    fuzzyId: { artist: "", album: "", title: "", genre: "" },
    volume: { ...span(0, length), name: "noname", bypassed: false, effects: [], sessionEventId: "", otherParams: {}, gainKeyPresent: true },
    track: { trackId: 1, available: true, problem: null, title: id, artist: "", album: "", genre: "", key: "", bpm, durationSecs: 200, sourceRate: E.SR, location: "", grid: g, gridProblem: null },
    ...extra,
  };
}

function doc(clips, extra = {}) {
  return E.refresh({ id: 1, name: "t", genre: "", artist: "", deviceInfo: "", sampleRate: E.SR, tempoBpm: 120, clips, transitions: [], length: 0, lengthSec: 0, meta: null, ...extra });
}

const BEAT = 22050; // 120 BPM
const BAR = 4 * BEAT;

// ---- tests ------------------------------------------------------------------------------------

test("TempoMap matches the Rust implementation's values", () => {
  let m = new E.TempoMap([{ pos: 0, speed: 1.25 }]);
  assert.equal(m.sourceAt(1000), 1250);
  assert.equal(m.localAt(1250), 1000);
  assert.equal(m.speedAt(-5), 1.25);
  m = new E.TempoMap([{ pos: 0, speed: Math.fround(1.0242525) }, { pos: 9925796, speed: Math.fround(1.0242525) }, { pos: 9925797, speed: Math.fround(1.0242525) }]);
  assert.equal(Math.round(m.sourceAt(9925797.4744418114)), 10166523); // the Editor's demo cue_point
  assert.equal(new E.TempoMap([]).sourceAt(77), 77);
  m = new E.TempoMap([{ pos: 0, speed: 1 }, { pos: 1000, speed: 2 }]);
  close(m.sourceAt(500), 625);
  close(m.sourceAt(1000), 1500);
  close(m.sourceAt(1200), 1900);
  close(m.speedAt(250), 1.25);
  for (const x of [-30, 0, 1, 333.3, 999.9, 1000, 5000.5]) close(m.localAt(m.sourceAt(x)), x, 1e-6, `round trip ${x}`);
  m = new E.TempoMap([{ pos: 2000, speed: 2 }, { pos: 1000, speed: 1 }, { pos: 1000, speed: 0.5 }, { pos: 0, speed: 1 }]);
  let prev = -Infinity;
  for (let x = 0; x < 4000; x += 10) {
    const s = m.sourceAt(x);
    assert.ok(s >= prev);
    prev = s;
    close(m.localAt(s), x, 1e-6, `unsorted ${x}`);
  }
});

test("tool catalogue keys are the ones the Rust model writes", () => {
  const rs = readFileSync(join(here, "../../crates/pm-formats/src/mix.rs"), "utf8");
  const arm = (kind) => {
    const m = rs.match(new RegExp(`EffectKind::${kind}[^=]*=> &\\[([^\\]]*)\\]`));
    assert.ok(m, kind);
    return [...m[1].matchAll(/"([^"]*)"/g)].map((x) => x[1]);
  };
  const byName = { Reverb: "Fx", Echo: "Delay", Roll: "Delay", Wah: "Peq", Crush: "Crush", FILTER: "Filter", EQ: "Eq" };
  for (const t of E.TOOL_TYPES) {
    const want = arm(byName[t.name]).map((p) => `${t.name}-${p}`);
    assert.deepEqual(t.params.map((p) => p.key).sort(), [...want].sort(), t.name);
  }
});

test("the Wah tool is presented as Vocal only while the card's Vocal filter is on", () => {
  const wah = (mode) => E.toolTypesFor(mode).find((t) => t.name === "Wah");
  // off (stock Wah, or the kill switch): the catalogue itself, nothing renamed
  assert.equal(E.toolTypesFor("off"), E.TOOL_TYPES);
  assert.equal(wah("off").label, "Wah");
  assert.equal(wah("off").params[0].label, "Cut-off");
  assert.equal(L.toolLabel("Wah"), "Wah");
  assert.equal(L.toolLabel("Wah", "off"), "Wah");
  for (const mode of ["vocal", "inverted"]) {
    const w = wah(mode);
    // renamed, with the same key and range: documents and the device see no difference
    assert.equal(w.label, "Vocal");
    assert.equal(L.toolLabel("Wah", mode), w.label, "the tool block and the menu agree");
    assert.equal(w.params.length, 1);
    assert.equal(w.params[0].key, "Wah-CutOff");
    assert.deepEqual([w.params[0].min, w.params[0].max, w.params[0].flat], [-100, 100, 0]);
    // the other tools are the very same objects
    for (const t of E.toolTypesFor(mode)) if (t.name !== "Wah") assert.equal(t, E.TOOL_TYPES.find((x) => x.name === t.name));
    assert.equal(L.toolLabel("FILTER", mode), "Filter");
  }
  // the parameter label names the direction: negative is the first thing it says, positive the second
  assert.match(wah("vocal").params[0].label, /cut \(-\).*keep \(\+\)/);
  assert.match(wah("inverted").params[0].label, /keep \(-\).*cut \(\+\)/);
  // creating a Wah tool does not depend on the mode: the key stays the Editor's
  assert.deepEqual(Object.keys(E.newTool("Wah", 0, 4410, "w").params), ["Wah-CutOff"]);
});

test("derived fields and transition links", () => {
  const a = clip("a", 0, 10 * BAR, 120, 1, grid(120, 0, 100, 0));
  const b = clip("b", 8 * BAR, 10 * BAR, 120, 1, grid(120, 0, 100, 0), { channel: 1, trimStart: BAR });
  let d = doc([a, b]);
  assert.equal(d.clips[1].visibleStart, 9 * BAR);
  assert.equal(d.clips[0].visibleEnd, 10 * BAR);
  assert.equal(d.length, 18 * BAR);
  close(d.clips[1].visibleStartSec, (9 * BAR) / E.SR);
  d = E.addTransition(d, E.newTransition("x", "y", 9 * BAR + 100, BAR));
  assert.deepEqual(d.transitions[0].between, { from: "a", to: "b" });
});

test("beats are classed relative to the first downbeat and follow the tempo curve", () => {
  const c = clip("a", 1000, 10 * BAR, 120, 1, grid(120, 0, 200, 2));
  const pts = E.clipBeatPoints(c);
  assert.equal(pts[0].time, 1000);
  assert.equal(pts[1].time, 1000 + BEAT);
  assert.deepEqual(pts.slice(0, 7).map((p) => p.kind), ["beat", "beat", "phrase", "beat", "beat", "beat", "bar"]);
  const fast = clip("f", 0, 10 * BAR, 120, 1.04, grid(120, 0, 8, 0));
  close(E.clipBeats(fast)[2], (2 * BEAT) / 1.04, 1e-6);
  assert.deepEqual(E.clipBeats(clip("n", 0, 100, 120, 1, null)), []);
});

test("snapping: targets, nearest, and a dragged clip's beats landing on another clip's bars", () => {
  const a = clip("a", 0, 40 * BAR, 120, 1, grid(120, 0, 400, 0));
  const b = clip("b", 0, 40 * BAR, 120, 1, grid(120, 5000, 400, 0), { channel: 1 }); // beats 5000 samples late
  const d = doc([a, b]);
  const bars = E.snapTargets(d, "bar", ["b"]);
  assert.equal(bars[0], 0);
  assert.equal(bars[1], BAR);
  assert.equal(E.snapTargets(d, "off").length, 0);
  assert.equal(E.nearestIndex([0, 10, 100], 50), 1);
  assert.equal(E.nearestIndex([0, 10, 100], 40), 1);
  assert.equal(E.snap(98, [0, 10, 100], 5), 100);
  assert.equal(E.snap(90, [0, 10, 100], 5), 90);
  // drag b near bar 3 of a: its own beats (5000 late) are pulled onto a's grid
  const opts = { mode: "bar", threshold: 8000 };
  const s = E.snapClipStart(d, "b", 3 * BAR - 5000 + 1234, opts);
  close(s, 3 * BAR - 5000, 1e-6);
  // too far: unchanged
  assert.equal(E.snapClipStart(d, "b", 3 * BAR + BAR / 2, { mode: "bar", threshold: 100 }), 3 * BAR + BAR / 2);
  // with no grid anywhere the reference tempo's grid is used
  const lone = doc([clip("c", 0, 10 * BAR, 120, 1, null)]);
  const t = E.snapTargets(lone, "beat", ["c"], [0, 5 * BEAT]);
  assert.equal(t[1], BEAT);
});

test("move, resize, add, delete clips", () => {
  const a = clip("a", 0, 10 * BAR, 120, 1, grid(120, 0, 100, 0));
  const b = clip("b", 20 * BAR, 10 * BAR, 120, 1, grid(120, 0, 100, 0), { trimStart: 2 * BAR });
  let d = doc([a, b]);
  d = E.moveClip(d, "b", 5 * BAR);
  assert.equal(d.clips[1].start, 5 * BAR);
  assert.equal(d.clips[1].visibleStart, 7 * BAR);
  d = E.moveClip(d, "b", -10 * BAR); // audible start may not go below 0
  assert.equal(d.clips[1].visibleStart, 0);
  // resize: left trim cannot reveal audio before the file start, right cannot go past the end
  d = E.resizeClip(d, "a", "left", -500);
  assert.equal(d.clips[0].trimStart, 0);
  d = E.resizeClip(d, "a", "left", 3 * BAR);
  assert.equal(d.clips[0].trimStart, 3 * BAR);
  assert.equal(d.clips[0].visibleStart, 3 * BAR);
  d = E.resizeClip(d, "a", "right", 100 * BAR);
  assert.equal(d.clips[0].trimEnd, 0);
  d = E.resizeClip(d, "a", "right", 6 * BAR);
  assert.equal(d.clips[0].trimEnd, 4 * BAR);
  d = E.resizeClip(d, "a", "right", 0); // never below MIN_LENGTH
  assert.equal(d.clips[0].visibleEnd - d.clips[0].visibleStart, E.MIN_LENGTH);
  // process positions are clip-local: trimming does not move them
  const tool = E.newTool("Reverb", BAR, BAR, "t1");
  d = E.addProcess(d, "a", tool);
  d = E.resizeClip(d, "a", "left", 4 * BAR);
  assert.equal(d.clips[0].processes[0].start, BAR);
  // add picks the free deck
  const c = clip("c", 0, 4 * BAR, 120, 1, null);
  const d2 = E.addClip(doc([clip("x", 0, 10 * BAR, 120, 1, null)]), c, 2 * BAR);
  assert.equal(d2.clips[1].channel, 1);
  assert.equal(d2.clips[1].visibleStart, 2 * BAR);
  // delete drops the transitions that joined the clip
  let d3 = doc([clip("p", 0, 10 * BAR, 120, 1, null), clip("q", 8 * BAR, 10 * BAR, 120, 1, null, { channel: 1 })]);
  d3 = E.addTransition(d3, E.newTransition("p", "q", 8 * BAR, 2 * BAR, "tr"));
  assert.equal(d3.transitions[0].between.to, "q");
  d3 = E.deleteClips(d3, ["q"]);
  assert.equal(d3.clips.length, 1);
  assert.equal(d3.transitions.length, 0);
});

test("processes: create flat tools, move / resize within the clip, delete loops", () => {
  const t = E.newTool("Echo", 1000, 8000, "e1");
  assert.deepEqual(Object.keys(t.params), ["Echo-Time", "Echo-Time ", "Echo-Wet/Dry"]);
  assert.deepEqual(t.params["Echo-Wet/Dry"], [{ pos: 0, value: 40 }, { pos: 7999, value: 40 }]);
  assert.equal(t.effects[0].class, "delay");
  assert.throws(() => E.newTool("Nope", 0, 10));
  let d = doc([clip("a", 0, 10 * BAR, 120, 1, null)]);
  d = E.addProcess(d, "a", t);
  d = E.moveProcess(d, "a", "e1", -50);
  assert.equal(d.clips[0].processes[0].start, 0);
  d = E.moveProcess(d, "a", "e1", 100 * BAR);
  assert.equal(d.clips[0].processes[0].start, 10 * BAR - 8000);
  d = E.moveProcess(d, "a", "e1", BAR);
  d = E.resizeProcess(d, "a", "e1", "right", BAR + 16000);
  const r = d.clips[0].processes[0];
  assert.equal(r.length, 16000);
  assert.deepEqual(r.params["Echo-Wet/Dry"].map((x) => x.pos), [0, 15999], "envelopes stretch with the tool");
  d = E.resizeProcess(d, "a", "e1", "left", BAR - 4000);
  assert.equal(d.clips[0].processes[0].start, BAR - 4000);
  assert.equal(d.clips[0].processes[0].length, 20000);
  // same-kind overlap is reported
  const other = E.newTool("Echo", BAR, 1000, "e2");
  assert.deepEqual(E.toolConflicts(d.clips[0], other), ["e1"]);
  assert.deepEqual(E.toolConflicts(d.clips[0], E.newTool("Reverb", BAR, 1000, "r")), []);
  // deleting a loop's parent turns the child into a first loop
  const l1 = E.newRegion("loop", 100, 1000, "l1");
  const l2 = { ...E.newRegion("loop", 1100, 1000, "l2"), parent: "l1", reLoop: true };
  d = E.addProcess(E.addProcess(d, "a", l1), "a", l2);
  d = E.deleteProcesses(d, ["l1"]);
  assert.equal(d.clips[0].processes.find((p) => p.id === "l2").parent, null);
});

test("envelopes: insert, move between neighbours, remove, evaluate", () => {
  let nodes = [{ pos: 0, value: 0 }, { pos: 100, value: 10 }];
  nodes = E.insertNode(nodes, { pos: 50, value: 5 });
  assert.deepEqual(nodes.map((x) => x.pos), [0, 50, 100]);
  nodes = E.insertNode(nodes, { pos: 50, value: 7 });
  assert.equal(nodes.length, 3);
  assert.equal(nodes[1].value, 7);
  nodes = E.moveNode(nodes, 1, { pos: 500.4, value: 1 });
  assert.equal(nodes[1].pos, 100, "cannot pass its neighbour");
  nodes = E.moveNode(nodes, 1, { pos: 40.6 });
  assert.equal(nodes[1].pos, 41);
  assert.equal(E.removeNode(nodes, 0).length, 2);
  close(E.envelopeAt([{ pos: 0, value: 0 }, { pos: 100, value: 10 }], 25), 2.5);
  assert.equal(E.envelopeAt([{ pos: 10, value: 3 }], 99), 3);
  let d = doc([clip("a", 0, 10 * BAR, 120, 1, null)]);
  d = E.setEnvelope(d, "a", { kind: "gain" }, [{ pos: 0, value: -3 }]);
  assert.equal(d.clips[0].gain[0].value, -3);
  assert.equal(E.setEnvelope(d, "a", { kind: "tempo" }, []), d, "a clip always keeps a tempo node");
});

test("retempo keeps each musical position: cue, tools, key nodes, beats", () => {
  const g = grid(124, 3000, 300, 1);
  let c = clip("a", 100000, 30 * BAR, 124, 1, g, { trimStart: 5 * BAR });
  c = { ...c, key: [{ pos: 0, cents: 0, flags: 1 }, { pos: 10 * BAR, cents: -300, flags: 1 }] };
  c.processes = [E.newTool("Reverb", 12 * BAR, 2 * BAR, "r1"), E.newRegion("loop", 8 * BAR, BAR, "l1")];
  const old = new E.TempoMap(c.tempo);
  const tempo = [{ pos: 0, speed: Math.fround(128 / 124) }];
  const r = E.retempoClip(c, tempo);
  const nw = new E.TempoMap(tempo);
  // the audible start stays where it was on the timeline
  close(r.start + r.trimStart, c.start + c.trimStart, 1e-6);
  // the cue is the same place in the audio
  close(nw.sourceAt(r.trimStart), old.sourceAt(c.trimStart), 1e-3);
  // the file's length in audio is unchanged
  close(nw.sourceAt(r.length), old.sourceAt(c.length), 1e-3);
  // the tool sits on the same audio, and is shorter by the speed ratio
  const t0 = c.processes[0];
  const t1 = r.processes[0];
  close(nw.sourceAt(t1.start), old.sourceAt(t0.start), 1e-3);
  close(t1.length, t0.length / Math.fround(128 / 124), 1e-3);
  close(r.processes[1].iterationLength, (c.processes[1].iterationLength * r.processes[1].length) / c.processes[1].length, 1e-6);
  assert.equal(r.key[1].pos, Math.round(nw.localAt(old.sourceAt(10 * BAR))));
  // beats keep their distance in the audio: beat k of the new clip is at nw.localAt(source k)
  close(E.clipBeats(r)[10] - r.start, nw.localAt(g.beatFrames[10]), 1e-6);
  // setClipBpm goes through the same path; a no-op request returns the same document
  const d = doc([c]);
  const d2 = E.setClipBpm(d, "a", 128);
  close(d2.clips[0].tempo[0].speed, 128 / 124, 1e-6);
  assert.equal(E.setClipBpm(d, "a", 0), d);
});

test("applyBeatmatch and applyTransition", () => {
  const a = clip("a", 0, 20 * BAR, 120, 1, grid(120, 0, 200, 0));
  const b = clip("b", 15 * BAR, 20 * BAR, 120, 1, grid(120, 0, 200, 0), { channel: 1 });
  let d = doc([a, b]);
  d = E.applyBeatmatch(d, { clipId: "b", referenceClipId: "a", tempo: [{ pos: 0, speed: 1 }], start: 16 * BAR, length: 20 * BAR, trimStart: 0, trimEnd: 0, targetBpm: 120, speed: 1, shift: BAR, anchorBeat: 0, referenceBeat: 0, barAligned: true, overlaps: true, notes: [] });
  assert.equal(d.clips[1].start, 16 * BAR);
  assert.equal(d.clips[1].visibleStart, 16 * BAR);
  const eq = (id) => ({ ...E.newTool("EQ", 16 * BAR, 4 * BAR, id) });
  const sugg = {
    transition: E.newTransition("a", "b", 16 * BAR, 4 * BAR, "tr1"),
    tools: [{ clipId: "a", tool: eq("ea"), conflicts: [] }, { clipId: "b", tool: { ...eq("eb"), start: 0 }, conflicts: ["old"] }],
    bars: 4,
    notes: [],
  };
  const e = E.applyTransition(d, sugg);
  assert.equal(e.transitions.length, 1);
  assert.equal(e.clips[0].processes.length, 1);
  assert.equal(e.clips[1].processes.length, 0, "a conflicting tool is skipped unless replacing");
  const f = E.applyTransition(d, sugg, true);
  assert.equal(f.clips[1].processes.length, 1);
});

test("copy / paste give fresh ids and keep loop chains", () => {
  const a = clip("a", 0, 20 * BAR, 120, 1, grid(120, 0, 200, 0));
  const l1 = E.newRegion("loop", 100, 1000, "l1");
  const l2 = { ...E.newRegion("loop", 1100, 1000, "l2"), parent: "l1", reLoop: true };
  a.processes = [l1, l2, E.newTool("FILTER", 5000, 3000, "f1")];
  let d = doc([a, clip("b", 30 * BAR, 10 * BAR, 120, 1, null)]);
  const cb = E.copyClips(d, ["a"]);
  const r = E.pasteClips(d, cb, 50 * BAR);
  assert.equal(r.doc.clips.length, 3);
  const pasted = r.doc.clips.find((c) => c.id === r.ids[0]);
  assert.notEqual(pasted.id, "a");
  assert.equal(pasted.visibleStart, 50 * BAR);
  const ids = pasted.processes.map((p) => p.id);
  assert.ok(ids.every((x) => !["l1", "l2", "f1"].includes(x)));
  assert.equal(pasted.processes[1].parent, ids[0], "the copy continues its own first loop");
  const all = new Set([...r.doc.clips.map((c) => c.id), ...r.doc.clips.flatMap((c) => c.processes.map((p) => p.id))]);
  assert.equal(all.size, r.doc.clips.length + 6);
  // processes into another clip
  const pc = E.copyProcesses(d, "a", ["l2", "f1"]);
  const p = E.pasteProcesses(d, "b", pc, 2000);
  const got = p.doc.clips[1].processes;
  assert.deepEqual(got.map((x) => x.start), [2000, 2000 + (5000 - 1100)]);
  assert.equal(got[0].parent, null, "a loop whose parent was not copied starts a chain");
  assert.equal(E.copyClips(d, ["zz"]), null);
});

test("find, ids", () => {
  const a = clip("a", 0, 10 * BAR, 120, 1, null, { processes: [E.newTool("EQ", 0, 1000, "p1")] });
  const d = E.addTransition(doc([a]), E.newTransition("a", "a", 0, 1000, "t1"));
  assert.equal(E.find(d, "a").kind, "clip");
  assert.equal(E.find(d, "p1").kind, "process");
  assert.equal(E.find(d, "t1").kind, "transition");
  assert.equal(E.find(d, "zz"), null);
  const id = E.newId();
  assert.match(id, /^[0-9a-f]{32}$/);
  assert.notEqual(id, E.newId());
});

// ---- transition recipes -----------------------------------------------------------------------

/** A base suggestion like the backend's: `bars` long from timeline `t0`. */
function baseSuggestion(d, bars, t0 = 16 * BAR, beat = BEAT) {
  const t = E.newTransition("a", "b", t0, bars * 4 * beat, "tr1");
  return { transition: t, tools: [], bars, beat, barsAvailable: bars, notes: [] };
}

function recipeDoc() {
  const a = clip("a", 0, 80 * BAR, 120, 1, grid(120, 0, 800, 0));
  const b = clip("b", 16 * BAR, 80 * BAR, 120, 1, grid(120, 0, 800, 0), { channel: 1 });
  return doc([a, b]);
}

const keysOf = (type) => E.TOOL_TYPES.find((x) => x.name === type).params;

/** Everything a saved document needs of a built suggestion (the Rust validator's rules for these parts). */
function checkBuilt(d, s, label) {
  const t = s.transition;
  assert.equal(t.between.from, "a", label);
  // curve: x strictly increasing from 0 to 1, values 0..1
  assert.equal(t.curve[0].x, 0, label);
  assert.equal(t.curve.at(-1).x, 1, label);
  for (let i = 0; i < t.curve.length; i++) {
    assert.ok(t.curve[i].value >= 0 && t.curve[i].value <= 1, `${label} curve value`);
    if (i) assert.ok(t.curve[i].x > t.curve[i - 1].x, `${label} curve x increases`);
  }
  assert.equal(t.curve[0].value, 0, `${label}: starts on the outgoing clip`);
  assert.equal(t.curve.at(-1).value, 1, `${label}: ends on the incoming clip`);
  const placed = new Map();
  for (const ts of s.tools) {
    const c = d.clips.find((x) => x.id === ts.clipId);
    const p = ts.tool;
    assert.equal(p.kind, "tool");
    // inside its clip and inside the transition window, on the clip's local axis
    assert.ok(p.start >= 0 && p.start + p.length <= c.length + 1e-6, `${label}: ${p.toolType} inside the clip`);
    assert.ok(c.start + p.start >= t.start - 1e-6 && c.start + p.start + p.length <= t.start + t.length + 1e-6, `${label}: ${p.toolType} inside the window`);
    // every parameter has an envelope, nothing extra, in the catalogue's range, integer strictly increasing positions over the tool
    const want = keysOf(p.toolType);
    assert.deepEqual(Object.keys(p.params).sort(), want.map((k) => k.key).sort(), `${label}: ${p.toolType} keys`);
    for (const k of want) {
      const nodes = p.params[k.key];
      assert.ok(nodes.length >= 2, `${label} ${k.key}`);
      assert.equal(nodes[0].pos, 0, `${label} ${k.key} first`);
      assert.equal(nodes.at(-1).pos, Math.round(p.length) - 1, `${label} ${k.key} last`);
      nodes.forEach((n, i) => {
        assert.ok(Number.isInteger(n.pos), `${label} ${k.key} integer`);
        assert.ok(n.value >= k.min && n.value <= k.max, `${label} ${k.key} ${n.value} in ${k.min}..${k.max}`);
        if (i) assert.ok(n.pos > nodes[i - 1].pos, `${label} ${k.key} increasing`);
      });
    }
    // no two tools of one kind overlap on a clip
    const key = `${ts.clipId}/${p.toolType}`;
    for (const o of placed.get(key) ?? []) assert.ok(p.start >= o.start + o.length - 1 || o.start >= p.start + p.length - 1, `${label}: two ${p.toolType} overlap`);
    placed.set(key, [...(placed.get(key) ?? []), p]);
  }
  // applying it gives a document with the transition and all its tools
  const applied = E.applyTransition(d, s);
  assert.equal(applied.transitions.length, 1);
  assert.equal(applied.clips.flatMap((c) => c.processes).length, s.tools.length, `${label}: all tools applied`);
}

test("recipe table: ids, lengths and defaults are consistent", () => {
  assert.deepEqual(R.RECIPE_IDS, ["bassSwap", "longBlend", "echoOut", "filterSweep", "dropSwap", "hardCut"]);
  assert.equal(new Set(R.RECIPE_IDS).size, R.RECIPES.length);
  for (const r of R.RECIPES) {
    assert.ok(r.lengths.includes(r.bars), `${r.id} default length is offered`);
    assert.deepEqual([...r.lengths].sort((x, y) => x - y), r.lengths, `${r.id} lengths ascend`);
    assert.ok(r.makes.length >= 1 && r.summary.length > 10 && r.label.length > 2);
    for (const l of r.lengths) assert.ok(Number.isInteger(l * 4), `${r.id}: ${l} bars is whole beats`);
  }
  // every tool a recipe can add is in the device catalogue
  const cat = new Set(E.TOOL_TYPES.map((x) => x.name));
  const used = new Set();
  for (const r of R.RECIPES) {
    const d = recipeDoc();
    for (const l of r.lengths) {
      for (const ts of R.buildRecipe(d, baseSuggestion(d, l), r.id, { cutAt: "end" }, E.recipeKit).tools) used.add(ts.tool.toolType);
    }
  }
  for (const u of used) assert.ok(cat.has(u), `${u} is a device tool`);
  assert.deepEqual([...used].sort(), ["EQ", "Echo", "FILTER", "Roll"]);
});

test("recipe lengths: labels, nearest, requested", () => {
  assert.equal(R.lengthLabel(16), "16 bars");
  assert.equal(R.lengthLabel(1), "1 bar");
  assert.equal(R.lengthLabel(0.25), "1 beat");
  assert.equal(R.lengthLabel(0.5), "2 beats");
  assert.equal(R.lengthLabel(1.5), "1.5 bars");
  assert.equal(R.nearestLength("echoOut", 16), 8);
  assert.equal(R.nearestLength("echoOut", 3), 4);
  assert.equal(R.nearestLength("echoOut", 2.5), 2);
  assert.equal(R.nearestLength("longBlend", 1), 8);
  assert.equal(R.nearestLength("hardCut", 0.3), 0.25);
  assert.equal(R.nearestLength("hardCut", NaN), 8);
  assert.equal(R.requestedBars("longBlend", { bars: null }), 32);
  assert.equal(R.requestedBars("longBlend", { bars: 4 }), 8);
  assert.equal(R.requestedBars("bassSwap", { bars: 16 }), 16);
  assert.throws(() => R.recipeDef("nope"));
});

test("recipe settings: defaults, merge ignores junk", () => {
  assert.deepEqual(R.mergeRecipePrefs(undefined), R.DEFAULT_RECIPE_PREFS);
  assert.deepEqual(R.mergeRecipePrefs("x"), R.DEFAULT_RECIPE_PREFS);
  assert.deepEqual(R.DEFAULT_RECIPE_PREFS, { recipe: "bassSwap", bars: null, cutAt: "end", onConflict: "ask" });
  assert.deepEqual(R.mergeRecipePrefs({ recipe: "echoOut", bars: 4, cutAt: "start", onConflict: "replace" }), { recipe: "echoOut", bars: 4, cutAt: "start", onConflict: "replace" });
  assert.deepEqual(R.mergeRecipePrefs({ recipe: "nope", bars: -3, cutAt: "middle", onConflict: 7 }), R.DEFAULT_RECIPE_PREFS);
  assert.equal(R.mergeRecipePrefs({ bars: 1000 }).bars, null);
  assert.equal(R.mergeRecipePrefs({ bars: "8" }).bars, null);
  assert.equal(R.mergeRecipePrefs({ bars: 0.25 }).bars, 0.25);
});

test("envelope() and curveOf() build valid nodes", () => {
  assert.deepEqual(R.envelope(1000, [[0, 1], [999, 2]]), [{ pos: 0, value: 1 }, { pos: 999, value: 2 }]);
  // missing ends are added with the neighbouring value; same position: the later wins; unsorted input is sorted
  assert.deepEqual(R.envelope(1000, [[500, 5], [250, 3], [500.4, 6]]), [{ pos: 0, value: 3 }, { pos: 250, value: 3 }, { pos: 500, value: 6 }, { pos: 999, value: 6 }]);
  // out of range positions are clamped
  assert.deepEqual(R.envelope(100, [[-5, 1], [5000, 2]]), [{ pos: 0, value: 1 }, { pos: 99, value: 2 }]);
  assert.deepEqual(R.envelope(10, []), []);
  assert.deepEqual(R.curveOf([[0.5, 0.5]]), [{ x: 0, value: 0.5 }, { x: 0.5, value: 0.5 }, { x: 1, value: 0.5 }]);
  assert.deepEqual(R.curveOf([[0, 0], [1, 1], [1, 0.7], [-1, 0]]).map((n) => n.x), [0, 1]);
  assert.deepEqual(R.curveOf([[0, -4], [1, 9]]), [{ x: 0, value: 0 }, { x: 1, value: 1 }]);
});

test("every recipe at every length builds a valid transition", () => {
  for (const r of R.RECIPES) {
    for (const l of r.lengths) {
      for (const cutAt of ["start", "end"]) {
        const d = recipeDoc();
        const s = R.buildRecipe(d, baseSuggestion(d, l), r.id, { cutAt }, E.recipeKit);
        checkBuilt(d, s, `${r.id}/${l}/${cutAt}`);
        assert.equal(s.transition.length, l * BAR, "the geometry is the backend's");
        assert.equal(s.transition.start, 16 * BAR);
      }
    }
  }
});

test("recipes keep to the backend's geometry and do not mutate it", () => {
  const d = recipeDoc();
  const base = baseSuggestion(d, 8);
  base.notes = ["kept"];
  const before = JSON.stringify(base);
  const s = R.buildRecipe(d, base, "echoOut", { cutAt: "end" }, E.recipeKit);
  assert.equal(JSON.stringify(base), before);
  assert.deepEqual(s.notes, ["kept"]);
  assert.equal(s.transition.id, "tr1");
  assert.equal(s.bars, 8);
  assert.notEqual(s.transition.curve, base.transition.curve);
});

test("the backend's bass-swap-only note is dropped by recipes that make no bass swap", () => {
  const d = recipeDoc();
  const note = "1 beat(s) is under a bar: no bass swap tools";
  const base = baseSuggestion(d, 0.25);
  base.notes = [note, "kept"];
  assert.deepEqual(R.buildRecipe(d, base, "hardCut", { cutAt: "end" }, E.recipeKit).notes, ["kept"]);
  assert.deepEqual(R.buildRecipe(d, base, "bassSwap", { cutAt: "end" }, E.recipeKit).notes, [note, "kept"]);
});

test("bass swap keeps the backend's tools", () => {
  const d = recipeDoc();
  const base = baseSuggestion(d, 16);
  base.tools = [{ clipId: "a", tool: E.newTool("EQ", 16 * BAR, 16 * BAR, "ea"), conflicts: [] }];
  const s = R.buildRecipe(d, base, "bassSwap", { cutAt: "end" }, E.recipeKit);
  assert.deepEqual(s.tools, base.tools);
  assert.deepEqual(s.transition.curve, [{ x: 0, value: 0 }, { x: 1, value: 1 }]);
});

test("echo out: echo takes over the outgoing clip by the middle, the incoming bass comes in there", () => {
  const d = recipeDoc();
  const s = R.buildRecipe(d, baseSuggestion(d, 4), "echoOut", { cutAt: "end" }, E.recipeKit);
  const L = 4 * BAR;
  assert.deepEqual(s.tools.map((x) => [x.clipId, x.tool.toolType]), [["a", "Echo"], ["b", "EQ"]]);
  const wet = s.tools[0].tool.params["Echo-Wet/Dry"];
  assert.equal(wet[0].value, 0);
  assert.equal(E.envelopeAt(wet, L / 2), 100);
  assert.equal(E.envelopeAt(wet, L / 4), 0);
  assert.equal(wet.at(-1).value, 100);
  // the outgoing clip is held until the middle, then the fader goes to the incoming one
  assert.deepEqual(s.transition.curve, [{ x: 0, value: 0 }, { x: 0.5, value: 0 }, { x: 1, value: 1 }]);
  const lo = s.tools[1].tool.params["EQ-Lo"];
  assert.equal(lo[0].value, -100);
  assert.equal(lo.at(-1).value, 0);
  assert.equal(E.envelopeAt(lo, L / 2), -100);
  // tools start at the transition, on each clip's own local axis (b starts at 16 bars, a at 0)
  assert.equal(s.tools[1].tool.start, 0);
  assert.equal(s.tools[0].tool.start, 16 * BAR);
  // nodes sit on beats
  for (const n of wet.slice(0, -1)) assert.equal(n.pos % BEAT, 0);
});

test("filter sweep: outgoing high-pass up, incoming low-pass opening", () => {
  const d = recipeDoc();
  const s = R.buildRecipe(d, baseSuggestion(d, 8), "filterSweep", { cutAt: "end" }, E.recipeKit);
  const [out, inc] = s.tools.map((x) => x.tool.params["FILTER-CutOff"]);
  assert.deepEqual(s.tools.map((x) => x.clipId), ["a", "b"]);
  assert.equal(out[0].value, 0);
  assert.equal(out.at(-1).value, 100);
  assert.ok(out.every((n, i) => i === 0 || n.value >= out[i - 1].value), "the high-pass only rises");
  assert.equal(inc[0].value, -100);
  assert.equal(inc.at(-1).value, 0);
  assert.ok(inc.every((n, i) => i === 0 || n.value >= inc[i - 1].value), "the low-pass only opens");
  assert.deepEqual(s.transition.curve, [{ x: 0, value: 0 }, { x: 1, value: 1 }]);
});

test("long blend: S-shaped fader and an overlapping bass trade", () => {
  const d = recipeDoc();
  const s = R.buildRecipe(d, baseSuggestion(d, 32), "longBlend", { cutAt: "end" }, E.recipeKit);
  const L = 32 * BAR;
  const [outLo, inLo] = s.tools.map((x) => x.tool.params["EQ-Lo"]);
  assert.equal(s.tools[0].clipId, "a");
  assert.equal(outLo[0].value, 0);
  assert.equal(outLo.at(-1).value, -100);
  assert.equal(inLo[0].value, -100);
  assert.equal(inLo.at(-1).value, 0);
  // at the middle neither bass is killed: the incoming one is on its way in before the outgoing one is gone
  assert.ok(E.envelopeAt(inLo, L * 0.5) > -50 && E.envelopeAt(outLo, L * 0.5) > -50);
  assert.ok(E.envelopeAt(inLo, L * 0.3) === -100 && E.envelopeAt(outLo, L * 0.3) === 0);
  assert.ok(E.envelopeAt(inLo, L * 0.6) === 0 && E.envelopeAt(outLo, L * 0.7) === -100);
  const q = s.transition.curve.find((n) => n.x === 0.25);
  assert.ok(q && q.value < 0.25, "slow start");
});

test("hard cut: a step at the end or the start, no tools", () => {
  const d = recipeDoc();
  const end = R.buildRecipe(d, baseSuggestion(d, 8), "hardCut", { cutAt: "end" }, E.recipeKit);
  assert.equal(end.tools.length, 0);
  const L = 8 * BAR;
  const [c0, c1, c2] = end.transition.curve;
  assert.deepEqual([c0, c2], [{ x: 0, value: 0 }, { x: 1, value: 1 }]);
  assert.equal(c1.value, 0);
  close((1 - c1.x) * L, R.CUT_RAMP, 2, "the step lasts about ten milliseconds");
  const start = R.buildRecipe(d, baseSuggestion(d, 8), "hardCut", { cutAt: "start" }, E.recipeKit);
  assert.deepEqual(start.transition.curve.map((n) => n.value), [0, 1, 1]);
  close(start.transition.curve[1].x * L, R.CUT_RAMP, 2);
  // a one-beat window: the step takes at most half of it
  const tiny = R.buildRecipe(d, baseSuggestion(d, 0.25), "hardCut", { cutAt: "end" }, E.recipeKit);
  assert.ok(tiny.transition.curve.every((n, i, a) => i === 0 || n.x > a[i - 1].x));
  assert.ok(tiny.transition.curve[1].x >= 0.5);
});

test("drop swap: build on the outgoing clip, step to the incoming one at the end", () => {
  const d = recipeDoc();
  const s = R.buildRecipe(d, baseSuggestion(d, 8), "dropSwap", { cutAt: "start" }, E.recipeKit);
  const L = 8 * BAR;
  assert.deepEqual(s.tools.map((x) => [x.clipId, x.tool.toolType]), [["a", "EQ"], ["a", "FILTER"], ["a", "Roll"]]);
  // the hard-cut option does not move this one
  assert.deepEqual(s.transition.curve.map((n) => n.value), [0, 0, 1]);
  assert.ok(s.transition.curve[1].x > 0.99);
  const [eq, filter, roll] = s.tools.map((x) => x.tool);
  assert.equal(E.envelopeAt(eq.params["EQ-Lo"], L / 2 - BEAT), 0, "bass present before the middle");
  assert.equal(E.envelopeAt(eq.params["EQ-Lo"], L - 1), -100, "bass gone for the build");
  assert.equal(filter.start, 16 * BAR + L / 2, "the rising filter starts at the middle");
  assert.equal(filter.start + filter.length, 16 * BAR + L);
  assert.equal(filter.params["FILTER-CutOff"].at(-1).value, 80);
  // the beat-repeat is the last bar, in whole beats, and tightens from an eighth to a sixteenth
  assert.equal(roll.length, BAR);
  assert.equal(roll.start + roll.length, 16 * BAR + L);
  assert.deepEqual([...new Set(roll.params["Roll-Time "].map((n) => n.value))], [-1, -2]);
  assert.ok(roll.params["Roll-Wet/Dry"].at(-1).value > roll.params["Roll-Wet/Dry"][0].value);
});

test("drop swap on a two-bar window keeps the repeat inside the window", () => {
  const d = recipeDoc();
  const s = R.buildRecipe(d, baseSuggestion(d, 2), "dropSwap", { cutAt: "end" }, E.recipeKit);
  checkBuilt(d, s, "drop swap 2 bars");
  const roll = s.tools.find((x) => x.tool.toolType === "Roll").tool;
  assert.equal(roll.length, BAR);
});

test("a recipe that needs more overlap than the clips share says so and changes nothing", () => {
  const d = recipeDoc();
  assert.throws(() => R.buildRecipe(d, baseSuggestion(d, 3), "longBlend", { cutAt: "end" }, E.recipeKit), /Only 3 bars .* long blend needs at least 8 bars/);
  assert.throws(() => R.buildRecipe(d, baseSuggestion(d, 1), "filterSweep", { cutAt: "end" }, E.recipeKit), /filter sweep needs at least 2 bars/);
  assert.throws(() => R.buildRecipe(d, baseSuggestion(d, 0.25), "echoOut", { cutAt: "end" }, E.recipeKit), /1 beat/);
  // the ones with a low minimum still work on short overlaps
  assert.doesNotThrow(() => R.buildRecipe(d, baseSuggestion(d, 0.25), "hardCut", { cutAt: "end" }, E.recipeKit));
  assert.doesNotThrow(() => R.buildRecipe(d, baseSuggestion(d, 1), "bassSwap", { cutAt: "end" }, E.recipeKit));
  assert.deepEqual(R.buildRecipe(d, baseSuggestion(d, 0.5), "bassSwap", { cutAt: "end" }, E.recipeKit).tools, []);
  const noLink = baseSuggestion(d, 8);
  noLink.transition = { ...noLink.transition, between: null };
  assert.throws(() => R.buildRecipe(d, noLink, "hardCut", { cutAt: "end" }, E.recipeKit), /which clips/);
  const wrong = baseSuggestion(d, 8);
  wrong.transition = { ...wrong.transition, between: { from: "zz", to: "b" } };
  assert.throws(() => R.buildRecipe(d, wrong, "echoOut", { cutAt: "end" }, E.recipeKit), /no clip zz/);
});

test("tools of the user's that are in the way are found and named", () => {
  const mine = E.newTool("Echo", 17 * BAR, 2 * BAR, "mine");
  const inside = E.newTool("Reverb", 18 * BAR, BAR, "rev");
  const outside = E.newTool("FILTER", 40 * BAR, BAR, "far");
  const a = clip("a", 0, 80 * BAR, 120, 1, grid(120, 0, 800, 0), { processes: [mine, inside, outside, E.newRegion("loop", 18 * BAR, BAR, "lp")] });
  const b = clip("b", 16 * BAR, 80 * BAR, 120, 1, grid(120, 0, 800, 0), { channel: 1 });
  const d = doc([a, b]);
  const s = R.buildRecipe(d, baseSuggestion(d, 4), "echoOut", { cutAt: "end" }, E.recipeKit);
  // the new Echo overlaps mine
  assert.deepEqual(s.tools[0].conflicts, ["mine"]);
  let way = R.toolsInTheWay(d, s, null);
  assert.deepEqual(way, [{ id: "mine", clipId: "a", toolType: "Echo", why: "overlaps" }]);
  // rebuilding a transition also takes in the tools inside its window, never loops or tools outside it
  way = R.toolsInTheWay(d, s, { start: 16 * BAR, length: 4 * BAR, between: { from: "a", to: "b" } });
  assert.deepEqual(way.map((w) => [w.id, w.why]), [["mine", "overlaps"], ["rev", "inside"]]);
  assert.deepEqual(R.toolsInTheWay(d, s, { start: 16 * BAR, length: 4 * BAR, between: null }).map((w) => w.id), ["mine"]);
  // applying: the new Echo is skipped without replacing, and replaces mine with it
  const kept = E.applyTransition(d, s, false);
  assert.equal(kept.clips[0].processes.some((p) => p.id === "mine"), true);
  assert.equal(kept.clips[0].processes.filter((p) => p.toolType === "Echo").length, 1, "the new Echo was skipped");
  const swapped = E.applyTransition(d, s, true);
  assert.equal(swapped.clips[0].processes.some((p) => p.id === "mine"), false);
  assert.equal(swapped.clips[0].processes.filter((p) => p.toolType === "Echo").length, 1);
  // nothing in the way on a clean mix
  const clean = recipeDoc();
  assert.deepEqual(R.toolsInTheWay(clean, R.buildRecipe(clean, baseSuggestion(clean, 8), "filterSweep", { cutAt: "end" }, E.recipeKit), null), []);
});

test("describeTools names every tool added", () => {
  const d = recipeDoc();
  const s = R.buildRecipe(d, baseSuggestion(d, 8), "dropSwap", { cutAt: "end" }, E.recipeKit);
  const lines = R.describeTools(s, (id) => `Track ${id}`);
  assert.deepEqual(lines, ['EQ on "Track a"', 'FILTER on "Track a"', 'Roll on "Track a"']);
  assert.equal(R.describeTools(s, (id) => id, (t) => t.toLowerCase())[0], 'eq on "a"');
});

// ---- mix library hygiene (mixlist.ts) and the Trans / Volume tools ---------------------------------

const sum = (id, name, secs = 600, tracks = 4) => ({ id, name, play_time_secs: secs, track_count: tracks });
const row = (over = {}) => ({ genre: "", year: 0, rating: 0, producer: "", remixer: "", comments: "", dateCreated: 0, dateModified: 0, timesPlayed: 0, lastPlayed: 0, playTimeSecs: 0, creatorId: "", ...over });
const LIST = [sum(1, "Warm-up Set", 1800, 8), sum(2, "Café Closing", 3600, 12), sum(3, "Set 10", 900, 3), sum(4, "Set 2", 905, 3), sum(5, "Peak Time", 2400, 9)];
const META = new Map([
  [1, row({ genre: "Deep House", rating: 4, dateModified: 300, comments: "for the Friday gig", year: 2024 })],
  [2, row({ genre: "Jazz", rating: 0, dateModified: 100, producer: "Invented Producer" })],
  [3, row({ genre: "Techno", rating: 5, dateModified: 500 })],
  [5, row({ genre: "Techno", rating: 3, dateModified: 200, remixer: "Invented Remixer" })],
]);
const metaOf = (id) => META.get(id);
const names = (l) => l.map((m) => m.name);

test("search: every word must match, accents and case are ignored, deep search widens the fields", () => {
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "", true)), names(LIST));
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "   ", true)), names(LIST));
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "cafe", false)), ["Café Closing"]);
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "SET", false)), ["Warm-up Set", "Set 10", "Set 2"]);
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "set 10", false)), ["Set 10"], "both words");
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "techno", false)), [], "name only");
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "techno", true)), ["Set 10", "Peak Time"]);
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "friday", true)), ["Warm-up Set"], "comments");
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "invented remixer", true)), ["Peak Time"]);
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "2024", true)), ["Warm-up Set"], "year");
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "set 2024", true)), ["Warm-up Set"], "name word plus year word");
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "techno peak", true)), ["Peak Time"]);
  assert.deepEqual(names(ML.filterMixes(LIST, () => undefined, "techno", true)), [], "facts not loaded yet: nothing invented");
  assert.deepEqual(names(ML.filterMixes(LIST, metaOf, "zzz", true)), []);
});

test("sort: numeric names, descending, ties fall back to the name, missing facts sort as empty", () => {
  const sorted = (key, asc = true) => names(ML.sortMixes(LIST, key, asc, metaOf));
  assert.deepEqual(sorted("name"), ["Café Closing", "Peak Time", "Set 2", "Set 10", "Warm-up Set"], "Set 2 before Set 10");
  assert.deepEqual(sorted("name", false), ["Warm-up Set", "Set 10", "Set 2", "Peak Time", "Café Closing"]);
  assert.deepEqual(sorted("play_time_secs"), ["Set 10", "Set 2", "Warm-up Set", "Peak Time", "Café Closing"]);
  assert.deepEqual(sorted("track_count"), ["Set 2", "Set 10", "Warm-up Set", "Peak Time", "Café Closing"], "equal counts: by name");
  assert.deepEqual(sorted("date", false), ["Set 10", "Warm-up Set", "Peak Time", "Café Closing", "Set 2"], "Set 2 has no facts = 0");
  assert.deepEqual(sorted("rating", false), ["Set 10", "Warm-up Set", "Peak Time", "Café Closing", "Set 2"]);
  assert.deepEqual(sorted("genre"), ["Set 2", "Warm-up Set", "Café Closing", "Peak Time", "Set 10"], "empty first, then Deep House, Jazz, Techno (tie by name)");
  assert.deepEqual(names(LIST), ["Warm-up Set", "Café Closing", "Set 10", "Set 2", "Peak Time"], "the input is not reordered");
  assert.equal(ML.defaultAscending("name"), true);
  assert.equal(ML.defaultAscending("rating"), false);
});

test("selection: click, Ctrl toggles, Shift selects a range, the search un-ticks hidden rows", () => {
  const rows = [10, 20, 30, 40, 50];
  const none = { selected: new Set(), anchor: null };
  const mod = (shift = false, ctrl = false) => ({ shift, ctrl });
  const ids = (s) => [...s.selected].sort((a, b) => a - b);
  let s = ML.clickRow(none, rows, 30, mod());
  assert.deepEqual(ids(s), [30]);
  s = ML.clickRow(s, rows, 50, mod(true));
  assert.deepEqual(ids(s), [30, 40, 50], "range from the anchor");
  assert.equal(s.anchor, 30, "the anchor stays");
  s = ML.clickRow(s, rows, 10, mod(true));
  assert.deepEqual(ids(s), [10, 20, 30], "range upwards replaces");
  s = ML.clickRow(s, rows, 40, mod(false, true));
  assert.deepEqual(ids(s), [10, 20, 30, 40], "ctrl adds");
  s = ML.clickRow(s, rows, 20, mod(false, true));
  assert.deepEqual(ids(s), [10, 30, 40], "ctrl removes");
  s = ML.clickRow(s, rows, 50, mod());
  assert.deepEqual(ids(s), [50], "plain click is that row alone");
  s = ML.clickRow({ selected: new Set([99]), anchor: 99 }, rows, 20, mod(true));
  assert.deepEqual(ids(s), [20], "shift with the anchor off screen is a plain click");
  s = ML.toggleRow(ML.toggleRow(none, 10), 20);
  assert.deepEqual(ids(s), [10, 20]);
  s = ML.toggleRow(s, 10);
  assert.deepEqual(ids(s), [20]);
  assert.deepEqual([...ML.prune(new Set([10, 20, 99]), [20, 30])], [20], "ticked rows that are not shown go");
});

test("copy names: numbered, within the library's length limit, same rule as the Rust unique_name", () => {
  assert.equal(ML.uniqueName([], "Set"), "Set");
  assert.equal(ML.uniqueName(["Set"], "Set"), "Set (2)");
  assert.equal(ML.uniqueName(["Set", "Set (2)", "Set (3)"], " Set "), "Set (4)");
  assert.equal(ML.copyName(["Set"], "Set"), "Set copy");
  assert.equal(ML.copyName(["Set", "Set copy"], "Set"), "Set copy (2)");
  const long = "x".repeat(ML.MAX_NAME);
  const n = ML.uniqueName([long], long);
  assert.equal([...n].length, ML.MAX_NAME);
  assert.ok(n.endsWith(" (2)"));
  assert.equal([...ML.copyName([], long)].length, ML.MAX_NAME);
});

test("list settings: a corrupt or old entry falls back field by field", () => {
  assert.deepEqual(ML.mergeListPrefs(null), ML.DEFAULT_LIST_PREFS);
  assert.deepEqual(ML.mergeListPrefs("junk"), ML.DEFAULT_LIST_PREFS);
  const custom = { sortKey: "rating", asc: false, deepSearch: false, onExisting: "skip" };
  assert.deepEqual(ML.mergeListPrefs(custom), custom);
  assert.deepEqual(ML.mergeListPrefs({ sortKey: "bogus", asc: "yes", onExisting: "delete-everything" }), ML.DEFAULT_LIST_PREFS);
  assert.deepEqual(ML.mergeListPrefs({ sortKey: "genre" }), { ...ML.DEFAULT_LIST_PREFS, sortKey: "genre" });
  assert.equal(ML.DEFAULT_LIST_PREFS.onExisting, "number", "never overwrite unless told to");
});

test("confirmation and report wording names every mix and file", () => {
  assert.equal(ML.deleteLine(sum(1, "Warm-up Set", 1800, 8)), "“Warm-up Set”: 8 tracks, 30:00");
  assert.equal(ML.deleteLine(sum(1, "One", 61, 1)), "“One”: 1 track, 1:01");
  assert.equal(ML.planLine({ id: 1, name: "A", fileName: "A.pmk", action: "write" }), "“A” → A.pmk  (new file)");
  assert.match(ML.planLine({ id: 1, name: "A", fileName: "A.pmk", action: "replace" }), /REPLACES/);
  assert.match(ML.planLine({ id: 1, name: "A", fileName: "A.pmk", action: "skip" }), /not exported.*kept/);
  assert.deepEqual(ML.deleteSummary({ deleted: [{ id: 1, name: "A" }], failed: [] }), { text: "Deleted 1 mix.", ok: true });
  const f = ML.deleteSummary({ deleted: [], failed: Array.from({ length: 6 }, (_, i) => ({ id: i, name: `M${i}`, error: "renamed" })) });
  assert.equal(f.ok, false);
  assert.match(f.text, /^Nothing was deleted\. Kept 6: “M0”: renamed; .*“M3”: renamed and 2 more\.$/);
  const e = ML.exportSummary({ written: [{ id: 1, name: "A", path: "x" }, { id: 2, name: "B", path: "y" }], skipped: [{ id: 3, name: "C" }], failed: [] }, "D:\\out");
  assert.equal(e.text, "Exported 2 mixes to D:\\out. Skipped 1 (file already there).");
  assert.equal(e.ok, true);
  assert.equal(ML.exportSummary({ written: [], skipped: [], failed: [{ id: 1, name: "A", error: "disk full" }] }, "d").ok, false);
});

test("Trans can be created (no envelope, tempo kept), Volume never is, and the menu follows the setting", () => {
  const t = E.newTool("Trans", 1000, 88200, "t1", 127.456);
  assert.equal(t.toolType, "Trans");
  assert.deepEqual(t.params, {}, "no automation keys: the Editor's names for them are unknown");
  assert.deepEqual(t.effects, [{ name: "Trans", bypassed: false, class: "trans", startBpm: 127.46, active: true }]);
  assert.equal(E.newTool("Trans", 0, 1000).effects[0].startBpm, 0, "0 = follow the clip");
  assert.equal(E.newTool("Trans", 0, 1000, "x", -5).effects[0].startBpm, 0);
  assert.throws(() => E.newTool("Volume", 0, 1000), /unknown tool type/);
  assert.throws(() => E.newTool("Delay", 0, 1000), /unknown tool type/);
  const plain = E.addableToolTypes("off", false).map((x) => x.name);
  const withTrans = E.addableToolTypes("off", true).map((x) => x.name);
  assert.deepEqual(withTrans, [...plain, "Trans"]);
  assert.ok(!plain.includes("Trans") && !withTrans.includes("Volume"));
  assert.equal(E.addableToolTypes("vocal", true).find((x) => x.name === "Wah").label, "Vocal");
  // in a document it is a tool like any other
  const d1 = E.addProcess(recipeDoc(), "a", E.newTool("Trans", 4 * BAR, 2 * BAR, "tr", 128));
  const tr = d1.clips[0].processes.find((p) => p.id === "tr");
  assert.equal(tr.toolType, "Trans");
  assert.deepEqual(E.toolConflicts(d1.clips[0], tr), [], "alone on its clip");
  const d2 = E.addProcess(d1, "a", E.newTool("Trans", 5 * BAR, 2 * BAR, "tr2", 128));
  assert.deepEqual(E.toolConflicts(d2.clips[0], d2.clips[0].processes.find((p) => p.id === "tr2")), ["tr"], "two Trans overlapping is refused like any tool");
});

console.log(`\n${n} tests passed`);
