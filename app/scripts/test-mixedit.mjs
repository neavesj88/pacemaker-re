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

console.log(`\n${n} tests passed`);
