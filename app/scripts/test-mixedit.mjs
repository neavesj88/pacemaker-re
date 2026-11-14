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
import * as TM from "../src/lib/state/tempoMatch.ts";
import * as R from "../src/lib/state/transitionRecipes.ts";
import * as ST from "../src/lib/state/sectionTransition.ts";
import * as L from "../src/lib/views/mixes/layout.ts";
import * as ML from "../src/lib/views/mixes/mixlist.ts";
import * as PS from "../src/lib/views/mixes/previewState.ts";

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
  assert.deepEqual(R.DEFAULT_RECIPE_PREFS, { recipe: "bassSwap", bars: null, cutAt: "end", onConflict: "ask", sectionEdge: "off", sectionReachBars: 8 });
  assert.deepEqual(R.mergeRecipePrefs({ recipe: "echoOut", bars: 4, cutAt: "start", onConflict: "replace" }), { ...R.DEFAULT_RECIPE_PREFS, recipe: "echoOut", bars: 4, cutAt: "start", onConflict: "replace" });
  assert.deepEqual(R.mergeRecipePrefs({ recipe: "nope", bars: -3, cutAt: "middle", onConflict: 7, sectionEdge: "middle", sectionReachBars: "8" }), R.DEFAULT_RECIPE_PREFS);
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

// ---- sections in the mix editor ---------------------------------------------------------------

/** Invented sections, `bars` long each, at 120 BPM (2 s a bar): [kind, bars] pairs. */
function secs(...parts) {
  let bar = 0;
  return parts.map(([kind, bars], i) => {
    const out = { kind, start_secs: bar * 2, end_secs: (bar + bars) * 2, start_bar: bar, end_bar: bar + bars, confidence: 0.8, start_confidence: i === 0 ? 1 : 0.7, on_phrase: true };
    bar += bars;
    return out;
  });
}
// track 1 (clip a): boundaries at bars 8, 40, 56, 72; track 2 (clip b): boundaries at bars 8, 40
const SEC_A = secs(["intro", 8], ["drop", 32], ["breakdown", 16], ["drop", 16], ["outro", 8]);
const SEC_B = secs(["intro", 8], ["drop", 32], ["outro", 24]);
const lookupOf = (m) => (id) => m[id] ?? null;
const LOOK = lookupOf({ 1: SEC_A, 2: SEC_B });
/** Clip a (track 1, bars 0 to 80) and clip b (track 2, from bar 16): boundaries on the timeline at a: 8 40 56 72, b: 24 56. */
function secDoc(extraB = {}) {
  const a = clip("a", 0, 80 * BAR, 120, 1, grid(120, 0, 800, 0));
  const b = clip("b", 16 * BAR, 80 * BAR, 120, 1, grid(120, 0, 800, 0), { channel: 1, trackId: 2, ...extraB });
  return doc([a, b]);
}

test("section points: boundaries on the timeline through the clip's start and tempo, none without sections", () => {
  const d = secDoc();
  const pts = E.clipSectionPoints(d.clips[1], SEC_B);
  assert.deepEqual(pts.map((p) => p.time), [24 * BAR, 56 * BAR]);
  assert.deepEqual(pts.map((p) => [p.before, p.after]), [["intro", "drop"], ["drop", "outro"]]);
  assert.equal(pts[0].confidence, 0.7);
  // a clip played 25% faster: the audio at source second 16 is heard 12.8 s into the clip
  const fast = clip("f", 3 * BAR, 80 * BAR, 120, 1.25, grid(120, 0, 800, 0));
  const fp = E.clipSectionPoints(fast, SEC_A);
  close(fp[0].time, 3 * BAR + (8 * BAR) / 1.25, 1e-6);
  close(fp[1].time, 3 * BAR + (40 * BAR) / 1.25, 1e-6);
  // nothing, one section, junk
  assert.deepEqual(E.clipSectionPoints(d.clips[0], null), []);
  assert.deepEqual(E.clipSectionPoints(d.clips[0], undefined), []);
  assert.deepEqual(E.clipSectionPoints(d.clips[0], secs(["drop", 40])), []);
  assert.deepEqual(E.clipSectionPoints(d.clips[0], [{ ...SEC_A[0] }, { ...SEC_A[1], start_secs: NaN }]), []);
  // out of order input comes back ascending
  assert.deepEqual(E.clipSectionPoints(d.clips[0], [SEC_A[0], SEC_A[3], SEC_A[1], SEC_A[2]]).map((p) => p.time), [8 * BAR, 40 * BAR, 56 * BAR]);
});

test("section spans cover the clip in clip-local samples through its tempo", () => {
  const d = secDoc();
  const spans = E.clipSectionSpans(d.clips[0], SEC_A);
  assert.equal(spans.length, 5);
  assert.deepEqual([spans[0].from, spans[0].to], [0, 8 * BAR]);
  assert.equal(spans[4].to, 80 * BAR);
  assert.equal(spans[1].kind, "drop");
  const fast = clip("f", 0, 80 * BAR, 120, 1.25, grid(120, 0, 800, 0));
  close(E.clipSectionSpans(fast, SEC_A)[0].to, (8 * BAR) / 1.25, 1e-6);
  assert.deepEqual(E.clipSectionSpans(d.clips[0], null), []);
  assert.deepEqual(E.clipSectionSpans(d.clips[0], [{ ...SEC_A[0], end_secs: 0 }]), [], "an empty or backwards section is not drawn");
});

test("snap mode 'section': targets are section boundaries where a clip has them, phrase lines where it has none", () => {
  const d = secDoc();
  const t = E.snapTargets(d, "section", [], null, LOOK);
  for (const bar of [8, 40, 56, 72, 24]) assert.ok(t.includes(bar * BAR), `bar ${bar} is a boundary`);
  assert.ok(!t.includes(4 * BAR) && !t.includes(12 * BAR), "no plain bars / beats");
  assert.ok(!t.includes(0), "the clip's start is not a boundary");
  // the clip without sections falls back to its phrase lines (every 8 bars)
  const d2 = secDoc({ trackId: 3 });
  const t2 = E.snapTargets(d2, "section", [], null, LOOK);
  assert.ok(t2.includes(8 * BAR) && t2.includes(16 * BAR) && t2.includes(32 * BAR), "b's phrase lines");
  assert.ok(!t2.includes(12 * BAR), "but not its bars");
  // no sections anywhere (or no lookup at all): exactly the phrase mode, so tracks without sections behave as today
  assert.deepEqual(E.snapTargets(d, "section"), E.snapTargets(d, "phrase"));
  assert.deepEqual(E.snapTargets(d, "section", [], null, lookupOf({})), E.snapTargets(d, "phrase"));
  // the other modes ignore the lookup
  for (const mode of ["beat", "bar", "phrase"]) assert.deepEqual(E.snapTargets(d, mode, [], null, LOOK), E.snapTargets(d, mode));
  // excluded clips contribute nothing
  assert.ok(!E.snapTargets(d, "section", ["a"], null, LOOK).includes(72 * BAR));
  // a clip's boundaries outside its audible part are not targets
  const trimmed = secDoc();
  trimmed.clips[0].visibleEnd = 50 * BAR;
  const t3 = E.snapTargets(trimmed, "section", ["b"], null, LOOK);
  assert.ok(t3.includes(40 * BAR) && !t3.includes(56 * BAR));
  // with no clip to snap to the reference tempo's phrase grid is used, as in phrase mode
  const lone = doc([clip("c", 0, 10 * BAR, 120, 1, null)]);
  assert.deepEqual(E.snapTargets(lone, "section", ["c"], [0, 70 * BAR], LOOK), E.snapTargets(lone, "phrase", ["c"], [0, 70 * BAR]));
});

test("snapping a dragged clip by its own section boundary onto another clip's", () => {
  const d = secDoc();
  const opts = { mode: "section", threshold: 8000, sections: LOOK };
  // b's boundary at local bar 8 should meet a's boundary at bar 40: b starts at bar 32
  close(E.snapClipStart(d, "b", 32 * BAR + 1000, opts), 32 * BAR, 1e-6);
  close(E.snapClipStart(d, "b", 32 * BAR - 5000, opts), 32 * BAR, 1e-6);
  // too far: unchanged
  assert.equal(E.snapClipStart(d, "b", 32 * BAR + 20000, opts), 32 * BAR + 20000);
  // without the lookup the same drag is exactly phrase mode
  assert.equal(E.snapClipStart(d, "b", 32 * BAR + 1000, { mode: "section", threshold: 8000 }), E.snapClipStart(d, "b", 32 * BAR + 1000, { mode: "phrase", threshold: 8000 }));
  // resizing and moving a crossfade take the lookup too
  // (a is resized onto b's boundary at bar 56)
  const r = E.resizeClip(d, "a", "right", 56 * BAR + 900, opts);
  close(r.clips[0].visibleEnd, 56 * BAR, 1e-6);
  const withT = E.applyTransition(d, { transition: E.newTransition("a", "b", 16 * BAR, 8 * BAR, "tx"), tools: [], bars: 8, beat: BEAT, barsAvailable: 8, notes: [] });
  const moved = E.moveTransition(withT, "tx", 40 * BAR + 700, null, opts);
  assert.equal(moved.transitions[0].start, 40 * BAR);
});

test("auto transition on section boundaries: off changes nothing, each mode moves what it should", () => {
  const d = secDoc();
  const base = baseSuggestion(d, 8); // bars 16 to 24
  const prefs = (extra) => ({ sectionEdge: "off", sectionReachBars: 8, ...extra });
  const minLen = 2 * BAR;
  // off, or no lookup, or no sections: the very same suggestion object comes back
  for (const [p, look] of [[prefs(), LOOK], [prefs({ sectionEdge: "both" }), undefined], [prefs({ sectionEdge: "both" }), lookupOf({})]]) {
    const r = ST.alignSuggestion(d, base, p, look, minLen);
    assert.equal(r.suggestion, base);
    assert.equal(r.moved, false);
    assert.deepEqual(r.notes, []);
  }
  // start: the nearest boundary to bar 16 within 8 bars is b's intro end at bar 24; the length is kept
  let r = ST.alignSuggestion(d, base, prefs({ sectionEdge: "start" }), LOOK, minLen);
  assert.equal(r.moved, true);
  assert.equal(r.suggestion.transition.start, 24 * BAR);
  assert.equal(r.suggestion.transition.length, 8 * BAR);
  assert.equal(r.suggestion.transition.startSec, (24 * BAR) / E.SR);
  assert.equal(r.suggestion.bars, 8);
  assert.equal(r.suggestion.barsAvailable, 56);
  assert.match(r.notes[0], /^Starts where a drop begins \(8 bars later than the beat-matched start\)\.$/);
  assert.equal(base.transition.start, 16 * BAR, "the backend's suggestion is not mutated");
  assert.deepEqual(base.notes, []);
  // out of reach: untouched geometry, but the note says why
  r = ST.alignSuggestion(d, base, prefs({ sectionEdge: "start", sectionReachBars: 4 }), LOOK, minLen);
  assert.equal(r.moved, false);
  assert.equal(r.suggestion.transition.start, 16 * BAR);
  assert.match(r.notes[0], /No section boundary within 4 bars of the start/);
  assert.deepEqual(r.suggestion.notes, r.notes);
  // end: 12 bars from 16 end at 28; the nearest boundary within 8 is bar 24: shorter by 4 bars
  const long = baseSuggestion(d, 12);
  r = ST.alignSuggestion(d, long, prefs({ sectionEdge: "end" }), LOOK, minLen);
  assert.equal(r.moved, true);
  assert.equal(r.suggestion.transition.start, 16 * BAR);
  assert.equal(r.suggestion.transition.length, 8 * BAR);
  assert.equal(r.suggestion.bars, 8);
  assert.match(r.notes[0], /^Ends where a drop begins \(4 bars shorter than asked\)\.$/);
  // an end that is already on a boundary is reported but nothing moves (the backend's tools stay valid)
  r = ST.alignSuggestion(d, base, prefs({ sectionEdge: "end" }), LOOK, minLen);
  assert.equal(r.moved, false);
  assert.equal(r.suggestion.transition.length, 8 * BAR);
  assert.match(r.notes[0], /^Ends where a drop begins\.$/);
  // both: 14 bars from 16 end at 30; the start goes to 24, the end to the boundary at 40 (reach 10)
  const l14 = baseSuggestion(d, 14);
  r = ST.alignSuggestion(d, l14, prefs({ sectionEdge: "both", sectionReachBars: 10 }), LOOK, minLen);
  assert.equal(r.moved, true);
  assert.equal(r.suggestion.transition.start, 24 * BAR);
  assert.equal(r.suggestion.transition.length, 16 * BAR);
  assert.equal(r.suggestion.bars, 16);
  assert.equal(r.notes.length, 2);
  // both, but only the start finds a boundary: the end keeps the length from the new start
  r = ST.alignSuggestion(d, long, prefs({ sectionEdge: "both" }), LOOK, minLen);
  assert.equal(r.suggestion.transition.start, 24 * BAR);
  assert.equal(r.suggestion.transition.length, 12 * BAR);
  assert.match(r.notes.join(" "), /No section boundary within 8 bars of the end, so it keeps its length/);
  // the window stays inside the audible part of both clips whatever the boundaries
  for (const mode of ["start", "end", "both"]) {
    for (const reach of [1, 8, 32]) {
      for (const bars of [2, 8, 12, 40]) {
        const x = ST.alignSuggestion(d, baseSuggestion(d, bars), prefs({ sectionEdge: mode, sectionReachBars: reach }), LOOK, minLen).suggestion.transition;
        assert.ok(x.start >= d.clips[1].visibleStart && x.start + x.length <= d.clips[0].visibleEnd + 1, `${mode}/${reach}/${bars} stays inside`);
        assert.ok(x.length >= 1);
      }
    }
  }
});

test("alignTransition: a boundary near the end of the clips shortens the window to fit; unusable boundaries are ignored", () => {
  const d = secDoc();
  const base = baseSuggestion(d, 8);
  const pt = (bar) => ({ time: bar * BAR, before: "drop", after: "outro", confidence: 0.9 });
  const input = (points, extra = {}) => ({ mode: "start", points, lo: 16 * BAR, hi: 80 * BAR, minLength: 2 * BAR, reachBars: 64, ...extra });
  const r = ST.alignTransition(base, input([pt(76)]));
  assert.equal(r.moved, true);
  assert.equal(r.suggestion.transition.start, 76 * BAR);
  assert.equal(r.suggestion.transition.length, 4 * BAR);
  assert.match(r.notes.join(" "), /Only 4 bars of both clips are left after the new start: the transition is shortened to fit\./);
  // a boundary with less than the minimum left before the end is not used at all
  const none = ST.alignTransition(base, input([pt(79)]));
  assert.equal(none.moved, false);
  assert.equal(none.suggestion.transition.start, 16 * BAR);
  // a boundary before the audible start is never used
  assert.equal(ST.alignTransition(base, input([pt(10)])).moved, false);
  // equally near boundaries: the earlier one
  assert.equal(ST.alignTransition(base, input([pt(14 + 4), pt(22 + 0)], { lo: 0 })).suggestion.transition.start, 18 * BAR);
  // no tempo (beat 0), no 'between', no points: the suggestion itself comes back
  const noBeat = { ...base, beat: 0 };
  assert.equal(ST.alignTransition(noBeat, input([pt(24)], { mode: "both" })).suggestion, noBeat);
  const noBetween = { ...base, transition: { ...base.transition, between: null } };
  assert.equal(ST.alignTransition(noBetween, input([pt(24)], { mode: "both" })).suggestion, noBetween);
  assert.equal(ST.alignTransition(base, input([], { mode: "both" })).suggestion, base);
  // distances in words
  assert.equal(ST.barsText(8 * BAR, BAR), "8 bars");
  assert.equal(ST.barsText(-BAR, BAR), "1 bar");
  assert.equal(ST.barsText(BAR / 2, BAR), "2 beats");
  assert.equal(ST.barsText(1.5 * BAR, BAR), "1.5 bars");
  assert.equal(ST.barsText(0, BAR), "no distance");
});

test("every recipe builds a valid transition on a window that moved onto section boundaries", () => {
  for (const r of R.RECIPES) {
    for (const cutAt of ["start", "end"]) {
      const d = secDoc();
      const base = baseSuggestion(d, 12);
      const al = ST.alignSuggestion(d, base, { sectionEdge: "both", sectionReachBars: 10 }, LOOK, r.lengths[0] * BAR);
      assert.equal(al.moved, true, r.id);
      const t0 = al.suggestion.transition;
      const s = R.buildRecipe(d, al.suggestion, r.id, { cutAt, moved: al.moved }, E.recipeKit);
      checkBuilt(d, s, `moved ${r.id}/${cutAt}`);
      assert.equal(s.transition.start, t0.start);
      assert.equal(s.transition.length, t0.length);
      assert.ok(s.notes.length >= al.notes.length, "the section notes reach the user");
    }
  }
  // the bass swap builds its own tools on a moved window: one EQ on each clip, inside the window
  const d = secDoc();
  const al = ST.alignSuggestion(d, baseSuggestion(d, 12), { sectionEdge: "start", sectionReachBars: 8 }, LOOK, BAR);
  const s = R.buildRecipe(d, al.suggestion, "bassSwap", { cutAt: "end", moved: true }, E.recipeKit);
  assert.equal(s.tools.length, 2);
  assert.deepEqual(s.tools.map((x) => x.clipId).sort(), ["a", "b"]);
  // unmoved, it still keeps the backend's own tools
  const base = baseSuggestion(d, 16);
  base.tools = [{ clipId: "a", tool: E.newTool("EQ", 16 * BAR, 16 * BAR, "ea"), conflicts: [] }];
  assert.deepEqual(R.buildRecipe(d, base, "bassSwap", { cutAt: "end", moved: false }, E.recipeKit).tools, base.tools);
  // under a bar a moved bass swap has no tools and says so, instead of throwing
  const tiny = baseSuggestion(d, 0.5);
  const ts = R.buildRecipe(d, tiny, "bassSwap", { cutAt: "end", moved: true }, E.recipeKit);
  assert.equal(ts.tools.length, 0);
  assert.match(ts.notes.join(" "), /no bass swap tools/);
});

test("section words and saved settings", () => {
  assert.equal(R.sectionEdgeWords("off"), "");
  assert.equal(R.sectionEdgeWords("start"), "starts on a section boundary");
  assert.equal(R.sectionEdgeWords("end"), "ends on a section boundary");
  assert.equal(R.sectionEdgeWords("both"), "starts and ends on section boundaries");
  assert.deepEqual(R.SECTION_EDGES, ["off", "start", "end", "both"]);
  assert.equal(R.mergeRecipePrefs({ sectionEdge: "both", sectionReachBars: 12.4 }).sectionReachBars, 12);
  assert.equal(R.mergeRecipePrefs({ sectionReachBars: 0 }).sectionReachBars, 8);
  assert.equal(R.mergeRecipePrefs({ sectionReachBars: 33 }).sectionReachBars, 8);
  assert.equal(R.mergeRecipePrefs({ sectionEdge: "both" }).sectionEdge, "both");
  // an old saved value without the new fields reads back as "off"
  assert.equal(R.mergeRecipePrefs({ recipe: "echoOut", bars: 4, cutAt: "start", onConflict: "skip" }).sectionEdge, "off");
});

// ---- the one-bar Mix tab: tempo matching, decks, overlaps, colours, Vocal ---------------------------

test("tempo match picks the target, double or half of it nearest the track's own tempo", () => {
  const pick = (bpm, target) => TM.chooseTempo(bpm, target);
  // a 130 BPM track joining a mix at 87 (= 174 in double time) plays at 174, not slowed to 87
  let c = pick(130, 87);
  assert.equal(c.factor, 2);
  close(c.playsAt, 174);
  close(c.stretchPct, (174 / 130 - 1) * 100, 1e-9);
  // an 87 BPM track in a mix at 174 plays at half of it: no stretch at all
  c = pick(87, 174);
  assert.equal(c.factor, 0.5);
  close(c.speed, 1);
  // 174 into a 130 mix: the same tempo, slowed 25 % (half time 65 would be slowed 63 %)
  c = pick(174, 130);
  assert.equal(c.factor, 1);
  close(c.stretchPct, (130 / 174 - 1) * 100, 1e-9);
  // 130 into 174: the target itself, +34 % (half time 87 is further from 130 in ratio terms)
  c = pick(130, 174);
  assert.equal(c.factor, 1);
  close(c.stretchPct, (174 / 130 - 1) * 100, 1e-9);
  // 128 into 126: a small slow-down at the same time
  c = pick(128, 126);
  assert.equal(c.factor, 1);
  close(c.stretchPct, (126 / 128 - 1) * 100, 1e-9);
  // 64 into 128: double time of 64 is 128, so it plays at half the mix tempo: no stretch
  c = pick(64, 128);
  assert.equal(c.factor, 0.5);
  close(c.speed, 1);
  // 80 into 160 (a misanalysed half-time track): plays unstretched
  c = pick(80, 160);
  assert.equal(c.factor, 0.5);
  close(c.speed, 1);
  // the same tempo is factor 1
  assert.equal(pick(100, 100).factor, 1);
  // junk gives no choice
  assert.equal(pick(0, 120), null);
  assert.equal(pick(120, Number.NaN), null);
  assert.equal(pick(-5, 120), null);
});

test("the stretch limit: at the limit is silent, beyond it warns; text of a stretch", () => {
  assert.equal(TM.exceeds(1.08, 8), false);
  assert.equal(TM.exceeds(1.081, 8), true);
  assert.equal(TM.exceeds(0.92, 8), false);
  assert.equal(TM.exceeds(0.91, 8), true);
  assert.equal(TM.exceeds(1.5, 100), false);
  assert.equal(TM.stretchText(23), "+23 %");
  assert.equal(TM.stretchText(-4.5), "-4.5 %");
  assert.equal(TM.stretchText(0), "+0.0 %");
});

test("syncClipTo: applies a small match, holds a big one back unless forced, keeps the audible start", () => {
  const g = grid(130, 0, 400, 0);
  const c = clip("a", 0, 30 * BAR, 130, 1, g, { trimStart: 2 * BAR });
  const d = doc([c], { tempoBpm: 87 });
  // 130 -> mix 87: double time 174 is +34 %, over the 8 % limit: nothing changes, the choice is returned
  let r = E.syncClipTo(d, "a", 87, 8);
  assert.equal(r.applied, false);
  assert.equal(r.doc, d);
  assert.equal(r.choice.factor, 2);
  // forced: it plays at 174
  r = E.syncClipTo(d, "a", 87, 8, true);
  assert.equal(r.applied, true);
  close(E.clipPlaysAt(r.doc.clips[0]), 174, 0.01);
  close(E.clipSpeed(r.doc.clips[0]), 174 / 130, 1e-6);
  // within the limit: applied without asking
  r = E.syncClipTo(d, "a", 128, 8);
  assert.equal(r.applied, true);
  close(E.clipPlaysAt(r.doc.clips[0]), 128, 0.01);
  // the audible start stays where it was
  close(r.doc.clips[0].visibleStart, d.clips[0].visibleStart, 1);
  // a clip without a tempo is left alone
  const z = doc([clip("z", 0, 10 * BAR, 0, 1, null)]);
  assert.equal(E.syncClipTo(z, "z", 120, 8).applied, false);
});

test("pitch per cent, fine tempo steps and tempo ramps", () => {
  const c = clip("a", 0, 30 * BAR, 124, 1, grid(124, 0, 400, 0));
  const d = doc([c]);
  const up = E.setClipPercent(d, "a", 2.5);
  close(E.clipSpeed(up.clips[0]), 1.025, 1e-6);
  close(E.clipPlaysAt(up.clips[0]), 124 * 1.025, 1e-4);
  assert.equal(E.setClipPercent(d, "a", -95), d, "not below -90 %");
  // 0.01 BPM steps survive the round trip
  const fine = E.setClipBpm(d, "a", 124.01);
  close(E.clipPlaysAt(fine.clips[0]), 124.01, 1e-4);
  // a ramp: a node at 10 bars at 5 % faster, linear between nodes
  const ramp = E.setTempoNodeAt(d, "a", 10 * BAR, 1.05);
  assert.equal(ramp.clips[0].tempo.length, 2);
  assert.deepEqual(ramp.clips[0].tempo.map((n) => n.pos), [0, 10 * BAR]);
  const m = new E.TempoMap(ramp.clips[0].tempo);
  close(m.speedAt(5 * BAR), 1.025, 1e-6);
  // removing a node keeps one; the curve can be replaced whole
  const back = E.removeTempoNode(ramp, "a", 1);
  assert.equal(back.clips[0].tempo.length, 1);
  assert.equal(E.removeTempoNode(back, "a", 0), back);
  const whole = E.setTempoCurve(d, "a", [{ pos: 0, speed: 1 }, { pos: 8 * BAR, speed: 1.04 }]);
  assert.equal(whole.clips[0].tempo.length, 2);
  assert.equal(E.setTempoCurve(d, "a", [{ pos: 0, speed: 9 }]), d, "absurd speeds are refused");
  // musical positions survive a ramp: the file's length in audio is unchanged
  const old = new E.TempoMap(c.tempo);
  close(m.sourceAt(ramp.clips[0].length), old.sourceAt(c.length), 1e-3);
});

test("decks are automatic: overlapping clips end up on different decks and a document that was fine is not rewritten", () => {
  const a = clip("a", 0, 10 * BAR, 120, 1, null);
  const b = clip("b", 8 * BAR, 10 * BAR, 120, 1, null); // overlaps a, both on deck 0
  const d = doc([a, b]);
  const n = E.normalizeChannels(d);
  assert.notEqual(n.clips[0].channel, n.clips[1].channel);
  // already fine: same object back
  assert.equal(E.normalizeChannels(n), n);
  const apart = doc([a, clip("c", 12 * BAR, 4 * BAR, 120, 1, null)]);
  assert.equal(E.normalizeChannels(apart), apart);
  // a clip that has a crossfade over its overlap on the same deck is left as it is
  const covered = E.addTransition(d, E.newTransition("a", "b", 8 * BAR, 2 * BAR, "x"));
  assert.equal(E.normalizeChannels(covered).clips[1].channel, 0);
  // moving a clip over another puts it on the other deck
  const moved = E.normalizeChannels(E.moveClip(doc([a, clip("m", 30 * BAR, 4 * BAR, 120, 1, null)]), "m", 5 * BAR));
  assert.notEqual(moved.clips[0].channel, moved.clips[1].channel);
  // three at once cannot all be separated onto two decks: nothing breaks
  const three = E.normalizeChannels(doc([a, b, clip("c", 9 * BAR, 4 * BAR, 120, 1, null)]));
  assert.equal(three.clips.length, 3);
});

test("overlapSlices cuts a clip by how many clips are heard together, the earlier clip on top", () => {
  const a = clip("a", 0, 10 * BAR, 120, 1, null);
  const b = clip("b", 8 * BAR, 10 * BAR, 120, 1, null, { channel: 1 });
  const d = doc([a, b]);
  const sa = E.overlapSlices(d, d.clips[0]);
  assert.equal(sa.length, 2);
  assert.deepEqual(sa.map((s) => [s.band, s.count]), [[0, 1], [0, 2]]);
  assert.equal(sa[0].to, 8 * BAR);
  assert.equal(sa[1].to, 10 * BAR);
  const sb = E.overlapSlices(d, d.clips[1]);
  assert.deepEqual(sb.map((s) => [s.band, s.count]), [[1, 2], [0, 1]]);
  assert.equal(sb[0].from, 8 * BAR);
  // a clip alone is one slice, full height
  const alone = doc([clip("x", 0, 4 * BAR, 120, 1, null)]);
  assert.deepEqual(E.overlapSlices(alone, alone.clips[0]).map((s) => [s.band, s.count]), [[0, 1]]);
  // slices of one clip tile its audible part exactly
  for (const c of d.clips) {
    const s = E.overlapSlices(d, c);
    assert.equal(s[0].from, c.visibleStart);
    assert.equal(s.at(-1).to, c.visibleEnd);
    for (let i = 1; i < s.length; i++) assert.equal(s[i].from, s[i - 1].to);
  }
});

test("clip colours: a stable palette of distinct colours, bands in the clip's own colour", () => {
  assert.equal(new Set(L.CLIP_COLOURS).size, L.CLIP_COLOURS.length);
  assert.ok(L.CLIP_COLOURS.length >= 8);
  assert.equal(L.clipColour(0), L.CLIP_COLOURS[0]);
  assert.ok(!L.CLIP_COLOURS.includes(L.clipColour(L.CLIP_COLOURS.length)), "the ninth clip is not one of the first eight");
  assert.equal(L.clipColour(-1), L.CLIP_COLOURS[0], "a bad index falls back to the first");
  assert.equal(L.lighten("#000000", 1), "#ffffff");
  assert.equal(L.lighten("#102030", 0), "#102030");
  const b = L.clipBands("#4aa8ff");
  assert.equal(b.low, "#4aa8ff");
  assert.notEqual(b.mid, b.low);
  assert.notEqual(b.high, b.mid);
});

test("Vocal is always in the tool menu, next to Wah, and is written as the Wah slot", () => {
  const names = (trans) => E.toolChoices(trans).map((t) => t.label);
  assert.ok(names(false).includes("Vocal") && names(false).includes("Wah"));
  assert.equal(names(false)[names(false).indexOf("Wah") + 1], "Vocal");
  assert.ok(!names(false).includes("Trans") && names(true).includes("Trans"));
  const v = E.toolChoices(false).find((t) => t.label === "Vocal");
  assert.equal(v.type, "Wah");
  assert.equal(v.vocal, true);
  assert.equal(E.toolChoices(false).filter((t) => t.vocal).length, 1, "Wah itself is not a Vocal choice");
  // what a Vocal choice creates is a plain Wah tool: nothing of the app's own mark is in the document
  const t = E.newTool(v.type, 0, 4 * BAR, "v1");
  assert.equal(t.effects[0].name, "Wah");
  assert.equal(t.effects[0].class, "peq");
  assert.deepEqual(Object.keys(t.params), ["Wah-CutOff"]);
  assert.ok(!/vocal/i.test(JSON.stringify(t)), "no vocal field in the tool the file is made from");
  // with the card's flag on, the label of a Wah tool follows it; a marked Wah is Vocal whatever the card says
  assert.equal(L.toolLabel("Wah", "off", true), "Vocal");
  assert.equal(L.toolLabel("Wah", "off", false), "Wah");
  assert.equal(L.toolLabel("Wah", "vocal", false), "Vocal");
  assert.equal(L.toolLabel("Reverb", "off", true), "Reverb");
});

test("a match that changes nothing makes no new document; a ramp is scaled, not flattened; one speed limit everywhere", () => {
  const g = grid(128, 0, 400, 0);
  const c = clip("a", 0, 30 * BAR, 128, 1, g);
  const d = doc([c], { tempoBpm: 128 });
  // already at the target: the same document, so no undo step and no dirty flag
  const same = E.syncClipTo(d, "a", 128, 8);
  assert.equal(same.applied, false);
  assert.equal(same.already, true);
  assert.equal(same.doc, d);
  assert.equal(E.setClipBpm(d, "a", 128), d);
  // a ramp: Sync to 134 scales every node, the shape (ratio between nodes) stays
  const ramp = E.setTempoNodeAt(d, "a", 10 * BAR, 1.05);
  const r = E.syncClipTo(ramp, "a", 134, 8);
  assert.equal(r.applied, true);
  assert.equal(r.doc.clips[0].tempo.length, 2, "the ramp survives");
  close(E.clipPlaysAt(r.doc.clips[0]), 134, 0.01);
  const sp = r.doc.clips[0].tempo.map((n) => n.speed);
  close(sp[1] / sp[0], 1.05, 1e-6, "ratio between the nodes");
  // one limit for every path: 0.1 < speed <= 4
  assert.equal(E.setClipPercent(d, "a", 900), d, "+900 % is speed 10");
  assert.equal(E.setClipBpm(d, "a", 128 * 5), d);
  assert.notEqual(E.setClipBpm(d, "a", 128 * 4), d);
  assert.equal(E.setClipBpm(d, "a", 12.8), d, "speed 0.1 itself is refused");
});

test("clip colours: never the same for two clips, however many", () => {
  const seen = new Set();
  for (let i = 0; i < 64; i++) {
    const col = L.clipColour(i);
    assert.match(col, /^#[0-9a-f]{6}$/, `clip ${i}`);
    assert.ok(!seen.has(col), `clip ${i} repeats ${col}`);
    seen.add(col);
  }
  assert.equal(L.clipColour(0), L.CLIP_COLOURS[0]);
  assert.equal(L.hslHex(0, 1, 0.5), "#ff0000");
  assert.equal(L.hslHex(120, 1, 0.5), "#00ff00");
});

// the set builder's solver and mix layout have their own test file; running it here keeps one entry point
await import("../src/lib/views/setbuilder/solver.test.mjs");


test("a ramped clip synced to a target really starts at the target; half/double keeps the ramp; add with both decks busy goes to the next free place", () => {
  const g = grid(120, 0, 400, 0);
  const c = clip("a", 3 * BAR, 40 * BAR, 120, 1, g);
  let d = doc([c], { tempoBpm: 128 });
  d = E.setTempoNodeAt(d, "a", 10 * BAR, 1.15);
  const r = E.syncClipTo(d, "a", 128, 100);
  assert.equal(r.applied, true);
  close(E.clipPlaysAt(r.doc.clips[0]), 128, 1e-4, "ramped clip lands on the target");
  assert.equal(E.syncClipTo(r.doc, "a", 128, 100).already, true, "a second Sync is a no-op");
  const two = E.setTempoCurve(d, "a", [{ pos: 0, speed: 1.05 }, { pos: 20 * BAR, speed: 0.97 }]);
  const r2 = E.syncClipTo(two, "a", 128, 100);
  close(E.clipPlaysAt(r2.doc.clips[0]), 128, 1e-4);
  // half / double time keeps a ramp
  const hd = E.matchHalfDouble(d, "a", 64 * 1.02, 8);
  assert.ok(hd && hd.choice.factor !== 1);
  assert.equal(hd.doc.clips[0].tempo.length, 2, "the ramp survives half time");
  assert.equal(E.matchHalfDouble(d, "a", 120, 8), null, "factor 1 is not this path");
  // both decks busy at the spot: the new clip goes after the clips it would overlap
  const busy = doc([clip("x", 0, 10 * BAR, 120, 1, null, { channel: 0 }), clip("y", 0, 12 * BAR, 120, 1, null, { channel: 1 })]);
  const added = E.addClip(busy, clip("n", 0, 4 * BAR, 120, 1, null), 2 * BAR);
  const n = added.clips.find((q) => q.id === "n");
  // 10 bars would lie 2 of its 4 bars over y: more than the cap, so after both
  assert.equal(n.visibleStart, 12 * BAR, "after the clips, not over most of one");
  assert.equal(n.channel, 0);
  const roomy = doc([clip("x", 0, 10 * BAR, 120, 1, null, { channel: 0 }), clip("y", 0, 10.5 * BAR, 120, 1, null, { channel: 1 })]);
  assert.equal(E.addClip(roomy, clip("n", 0, 4 * BAR, 120, 1, null), 2 * BAR).clips.at(-1).visibleStart, 10 * BAR, "a small overlap is fine");
  assert.equal(added.problems?.length ?? 0, busy.problems?.length ?? 0, "no new overlap problem");
});

// ---- Mix tab fixes (fx2) --------------------------------------------------------------------

test("a tool's start or length typed as NaN leaves the mix alone", () => {
  const a = clip("a", 0, 20 * BAR, 120, 1, null);
  let d = doc([a]);
  d = E.addProcess(d, "a", E.newTool("Echo", BAR, 4 * BAR, "e1"));
  assert.equal(E.moveProcess(d, "a", "e1", Number.NaN), d, "move by NaN is a no-op");
  assert.equal(E.moveProcess(d, "a", "e1", Number.POSITIVE_INFINITY), d);
  assert.equal(E.resizeProcess(d, "a", "e1", "right", Number.NaN), d, "resize to NaN is a no-op");
  const p = E.clipOf(E.moveProcess(d, "a", "e1", 2 * BAR), "a").processes[0];
  assert.equal(p.start, 2 * BAR, "a real value still moves it");
  assert.ok(Number.isFinite(p.length));
});

test("an emptied mix has no reference tempo, a mix with clips keeps it", () => {
  const one = doc([clip("a", 0, 10 * BAR, 120, 1, null)]);
  const empty = doc([]);
  assert.equal(E.carryTempo(124, empty), null, "undo or delete to an empty mix drops the tempo");
  assert.equal(E.carryTempo(124, one), 124, "with clips the tempo stays");
  assert.equal(E.carryTempo(null, one), null);
});

test("a deep link that names the tracks under both keys adds them once", () => {
  assert.deepEqual(ML.addIdsFrom({ "mixes.addTracks": [3, 4], addTracks: [3, 4] }), [3, 4]);
  assert.deepEqual(ML.addIdsFrom({ addTracks: [9] }), [9]);
  assert.deepEqual(ML.addIdsFrom({ "mixes.addTracks": [1, "x", 2] }), [1, 2]);
  assert.deepEqual(ML.addIdsFrom({ other: [1] }), []);
  assert.deepEqual(ML.addIdsFrom({ addTracks: "nope" }), []);
});

test("what a library reload means for the mix open in the editor", () => {
  assert.equal(ML.openMixVerdict(100, 100, true), "same");
  assert.equal(ML.openMixVerdict(undefined, 5, false), "same", "a mix opened without a stamp is not judged");
  assert.equal(ML.openMixVerdict(100, 200, false), "reload", "no edits: read it again");
  assert.equal(ML.openMixVerdict(100, 200, true), "changed", "edits: never throw them away, hold Save");
  assert.equal(ML.openMixVerdict(100, null, false), "removed");
  assert.equal(ML.openMixVerdict(100, null, true), "gone");
});

test("Play knows when nothing would be heard, and the Vocal marks reach the engine sorted", () => {
  const aud = clip("a", 0, 10 * BAR, 120, 1, null);
  const d = doc([aud, clip("b", 0, 10 * BAR, 120, 1, null, { clipBypassed: true })]);
  assert.equal(PS.hasAudibleClips(d), true);
  assert.equal(PS.hasAudibleClips(doc([clip("b", 0, 10 * BAR, 120, 1, null, { clipBypassed: true })])), false, "every clip muted");
  assert.equal(PS.hasAudibleClips(doc([])), false);
  let m = E.addProcess(d, "a", E.newTool("Wah", 0, BAR, "w2"));
  m = E.addProcess(m, "a", E.newTool("Wah", 2 * BAR, BAR, "w1"));
  assert.deepEqual(PS.markedToolIds(m, ["w2", "w1", "gone"]), ["w1", "w2"]);
  assert.deepEqual(PS.markedToolIds(m, []), []);
});

test("clips that sit near each other on the bar never share a colour", () => {
  // CIELAB distance (D65) between clipColour(i) and clipColour(j) for neighbours up to three apart
  const lab = (hex) => {
    const v = parseInt(hex.slice(1), 16);
    const [r, g, b] = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((c) => {
      c /= 255;
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    const x = (r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047;
    const y = r * 0.2126 + g * 0.7152 + b * 0.0722;
    const z = (r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883;
    const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
    return [116 * f(y) - 16, 500 * (f(x) - f(y)), 200 * (f(y) - f(z))];
  };
  for (let i = 0; i < 60; i++) {
    for (let j = i + 1; j <= i + 3; j++) {
      const a = lab(L.clipColour(i));
      const b = lab(L.clipColour(j));
      assert.ok(Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) > 15, `clips ${i} and ${j} are too alike`);
    }
  }
});

console.log(`\n${n} tests passed`);
