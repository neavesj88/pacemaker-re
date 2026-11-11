// The set builder's solver: pure, deterministic, no Svelte / Tauri / app imports (so
// scripts/test-mixedit.mjs runs it under plain node). It orders a pool of tracks into a set:
// harmonic moves on the Camelot wheel, tempo continuity (with half / double time), an optional
// energy curve, a target length with a per-track cap, and tracks the user has pinned to a
// position. Greedy construction from several starts, then local improvement; no randomness, no
// clock, so the same input always gives the same set.
//
// Vocabulary: a "set" is an ordered list of track ids; a "transition" is the hop from one track to
// the next, scored 0..100 (100 = no concerns). The score shown in the UI strip, the cost the
// search minimises and the numbers the mix builder plans from all come from `transitionBetween`
// here, so a preview can never disagree with the action it previews.

// ---------------------------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------------------------

/** A key on the Camelot wheel: number 1..12, A = minor, B = major. */
export type KeyPos = { num: number; letter: "A" | "B" };

export type SetTrack = {
  id: number;
  /** Tempo in BPM; 0 (or not finite) = unknown. */
  bpm: number;
  /** null = unknown key. */
  key: KeyPos | null;
  /** Full length in seconds. */
  secs: number;
  /** 1..10, or null when no energy value exists for the track. */
  energy: number | null;
};

export const keyCode = (k: KeyPos) => `${k.num}${k.letter}`;

// ---------------------------------------------------------------------------------------------
// Key moves
// ---------------------------------------------------------------------------------------------

/** A move on the Camelot wheel from one track's key to the next one's. */
export type KeyMoveId = "same" | "relative" | "adjacent" | "twoSteps" | "diagonal" | "semitone" | "clash";

/** What each move is, in the order the settings list them. `base` is its score when allowed. */
export const KEY_MOVES: { id: Exclude<KeyMoveId, "clash">; label: string; hint: string; base: number }[] = [
  { id: "same", label: "Same key", hint: "8A to 8A", base: 100 },
  { id: "relative", label: "Relative major / minor", hint: "8A to 8B", base: 95 },
  { id: "adjacent", label: "One step round the wheel", hint: "8A to 7A or 9A", base: 90 },
  { id: "twoSteps", label: "Two steps round the wheel", hint: "8A to 6A or 10A", base: 70 },
  { id: "diagonal", label: "Diagonal (one step and switch mode)", hint: "8A to 9B or 7B", base: 70 },
  { id: "semitone", label: "A semitone up or down", hint: "8A to 3A (+7) or 1A (-7)", base: 60 },
];

/** Score of a key pair that no allowed move covers. */
export const CLASH_SCORE = 10;
/** Score when either key is unknown: neither good nor bad. */
export const UNKNOWN_SCORE = 50;

/** The wheel move from `a` to `b`. */
export function keyMove(a: KeyPos, b: KeyPos): KeyMoveId {
  const d = (((b.num - a.num) % 12) + 12) % 12;
  if (a.letter === b.letter) {
    if (d === 0) return "same";
    if (d === 1 || d === 11) return "adjacent";
    if (d === 2 || d === 10) return "twoSteps";
    if (d === 7 || d === 5) return "semitone";
    return "clash";
  }
  if (d === 0) return "relative";
  if (d === 1 || d === 11) return "diagonal";
  return "clash";
}

/** "same key", "+1", "-1", "relative (A to B)" ...: the move as a DJ would write it. */
export function moveText(a: KeyPos, b: KeyPos): string {
  const m = keyMove(a, b);
  const d = (((b.num - a.num) % 12) + 12) % 12;
  const signed = d > 6 ? d - 12 : d;
  const sign = (n: number) => (n > 0 ? `+${n}` : `−${Math.abs(n)}`);
  switch (m) {
    case "same":
      return "same key";
    case "relative":
      return `relative (${a.letter} to ${b.letter})`;
    case "adjacent":
    case "twoSteps":
      return sign(signed);
    case "diagonal":
      return `diagonal (${sign(signed)}, ${a.letter} to ${b.letter})`;
    case "semitone":
      return d === 7 ? "+7 (semitone up)" : "−7 (semitone down)";
    default:
      return a.letter === b.letter ? `${sign(signed)} (clash)` : "clash";
  }
}

// ---------------------------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------------------------

export type EnergyCurve = "off" | "arc";

export type SolverOptions = {
  /** The key moves that count as good. Any other pair is a clash. */
  allowedMoves: KeyMoveId[];
  /** Largest tempo jump between neighbours, as a percentage of the first track's tempo. */
  maxBpmJumpPct: number;
  /** A track at half or double the other's tempo counts as matching. */
  halfDouble: boolean;
  /** 0..1: how much key counts against tempo in a transition's score. */
  keyWeight: number;
  /** "arc": warm-up, peak, cool-down, used for tracks that have an energy value. */
  curve: EnergyCurve;
  /** Where the peak falls, as a fraction of the set (0.3 .. 0.9). */
  peakAt: number;
  /** 0..1: how hard the search pulls tracks towards the curve. */
  energyWeight: number;
  /** Longest a track plays, seconds (longer tracks are cut short in the mix). null = whole track. */
  maxTrackSecs: number | null;
  /** Length to aim for, seconds. null = use every track. */
  targetSecs: number | null;
};

export const DEFAULT_SOLVER: SolverOptions = {
  allowedMoves: ["same", "relative", "adjacent"],
  maxBpmJumpPct: 6,
  halfDouble: true,
  keyWeight: 0.5,
  curve: "off",
  peakAt: 0.65,
  energyWeight: 0.5,
  maxTrackSecs: null,
  targetSecs: null,
};

/** Seconds a track occupies in the set (its length, capped by the per-track maximum). */
export function playSecs(t: SetTrack, o: Pick<SolverOptions, "maxTrackSecs">): number {
  const s = Number.isFinite(t.secs) && t.secs > 0 ? t.secs : 0;
  return o.maxTrackSecs != null && o.maxTrackSecs > 0 ? Math.min(s, o.maxTrackSecs) : s;
}

// ---------------------------------------------------------------------------------------------
// Tempo
// ---------------------------------------------------------------------------------------------

export type TempoRelation = "same" | "double" | "half" | "unknown";

export type BpmMatch = {
  relation: TempoRelation;
  /** Signed percentage change from `a` to `b` (after halving / doubling `b`, when that is the match). */
  deltaPct: number;
  /** `b` as compared with `a`: b, 2b or b / 2. */
  used: number;
};

const known = (bpm: number) => Number.isFinite(bpm) && bpm > 0;

/** How well `b`'s tempo follows `a`'s. With half / double allowed, whichever of b, 2b, b/2 is
 * nearest to `a` is the one compared. "unknown" when either tempo is missing. */
export function bpmMatch(a: number, b: number, halfDouble: boolean): BpmMatch {
  if (!known(a) || !known(b)) return { relation: "unknown", deltaPct: 0, used: b };
  let used = b;
  let relation: TempoRelation = "same";
  if (halfDouble) {
    for (const [v, r] of [[b * 2, "double"], [b / 2, "half"]] as const) {
      if (Math.abs(v - a) < Math.abs(used - a) - 1e-9) {
        used = v;
        relation = r;
      }
    }
  }
  return { relation, deltaPct: ((used - a) / a) * 100, used };
}

// ---------------------------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------------------------

export type Transition = {
  from: number;
  to: number;
  /** null when either key is unknown. */
  move: KeyMoveId | null;
  /** The move in words ("+1", "relative (A to B)"), or "key unknown". */
  moveLabel: string;
  fromKey: string;
  toKey: string;
  /** null = unknown. */
  keyOk: boolean | null;
  bpmFrom: number;
  bpmTo: number;
  relation: TempoRelation;
  /** Signed tempo change in percent (0 when unknown). */
  deltaPct: number;
  /** null = unknown. */
  bpmOk: boolean | null;
  /** to.energy - from.energy, null when either is missing. */
  energyDelta: number | null;
  keyScore: number;
  bpmScore: number;
  /** 0..100. */
  score: number;
  /** Short reasons the transition is weak, in words. */
  concerns: string[];
};

const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x));

/** Score of a tempo change: 100 at no change down to 60 at the limit, then falling to 0 at twice
 * the limit. Unknown tempo scores `UNKNOWN_SCORE`. */
export function bpmScore(deltaPct: number, maxPct: number): number {
  const d = Math.abs(deltaPct);
  const lim = Math.max(0.1, maxPct);
  if (d <= lim) return 100 - 40 * (d / lim);
  return Math.max(0, 60 - 60 * ((d - lim) / lim));
}

/** The transition from `a` to `b`: the single place that scores a hop. */
export function transitionBetween(a: SetTrack, b: SetTrack, o: SolverOptions): Transition {
  const concerns: string[] = [];

  let move: KeyMoveId | null = null;
  let keyOk: boolean | null = null;
  let keyScore = UNKNOWN_SCORE;
  let moveLabel = "key unknown";
  if (a.key && b.key) {
    move = keyMove(a.key, b.key);
    moveLabel = moveText(a.key, b.key);
    const allowed = move !== "clash" && o.allowedMoves.includes(move);
    keyOk = allowed;
    keyScore = allowed ? KEY_MOVES.find((m) => m.id === move)!.base : CLASH_SCORE;
    if (!allowed) concerns.push(move === "clash" ? `keys clash (${moveLabel})` : `key move not allowed (${moveLabel})`);
  } else {
    concerns.push(!a.key && !b.key ? "both keys unknown" : "one key unknown");
  }

  const m = bpmMatch(a.bpm, b.bpm, o.halfDouble);
  let bpmOk: boolean | null = null;
  let bScore = UNKNOWN_SCORE;
  if (m.relation !== "unknown") {
    bpmOk = Math.abs(m.deltaPct) <= o.maxBpmJumpPct + 1e-9;
    bScore = bpmScore(m.deltaPct, o.maxBpmJumpPct);
    if (!bpmOk) concerns.push(`tempo jumps ${Math.abs(m.deltaPct).toFixed(1)} % (limit ${o.maxBpmJumpPct} %)`);
  } else {
    concerns.push(!known(a.bpm) && !known(b.bpm) ? "both tempos unknown" : "one tempo unknown");
  }

  const w = clamp(o.keyWeight, 0, 1);
  const score = Math.round(w * keyScore + (1 - w) * bScore);
  const energyDelta = a.energy != null && b.energy != null ? b.energy - a.energy : null;
  return {
    from: a.id,
    to: b.id,
    move,
    moveLabel,
    fromKey: a.key ? keyCode(a.key) : "?",
    toKey: b.key ? keyCode(b.key) : "?",
    keyOk,
    bpmFrom: a.bpm,
    bpmTo: b.bpm,
    relation: m.relation,
    deltaPct: m.deltaPct,
    bpmOk,
    energyDelta,
    keyScore,
    bpmScore: bScore,
    score,
    concerns,
  };
}

/** Search cost of a transition: low is good. A broken limit costs extra on top of a low score so
 * the search prefers a clean pair over a slightly better-scoring one that breaks a limit. */
function costOf(t: Transition): number {
  return 100 - t.score + (t.keyOk === false ? 60 : 0) + (t.bpmOk === false ? 60 : 0);
}

// ---------------------------------------------------------------------------------------------
// Energy curve
// ---------------------------------------------------------------------------------------------

/** The energy (0..1) the curve wants at position `t` (0..1) of the set: from 0.25 up to 1 at the
 * peak, then down to 0.4. */
export function arcTarget(t: number, peakAt: number): number {
  const p = clamp(peakAt, 0.05, 0.95);
  const x = clamp(t, 0, 1);
  return x <= p ? 0.25 + 0.75 * (x / p) : 1 - 0.6 * ((x - p) / (1 - p));
}

/** Energy 1..10 as 0..1. */
export const energyNorm = (e: number) => clamp((e - 1) / 9, 0, 1);

/** Whether the curve can act: it is on and at least one track has an energy value. */
export function curveActive(tracks: SetTrack[], o: SolverOptions): boolean {
  return o.curve !== "off" && tracks.some((t) => t.energy != null);
}

/** Cost of `t` sitting at `slot` of `n`, when the curve is on. 0 for a track without energy. */
export function energyCost(t: SetTrack, slot: number, n: number, o: SolverOptions): number {
  if (o.curve === "off" || t.energy == null) return 0;
  const target = arcTarget(n <= 1 ? 0 : slot / (n - 1), o.peakAt);
  return clamp(o.energyWeight, 0, 1) * 60 * Math.abs(energyNorm(t.energy) - target);
}

// ---------------------------------------------------------------------------------------------
// Evaluating a given order
// ---------------------------------------------------------------------------------------------

export type SetSummary = {
  transitions: Transition[];
  /** Sum of the tracks' play times (capped by the per-track maximum). Overlaps are not taken off. */
  secs: number;
  /** Mean transition score, null for a set of fewer than two tracks. */
  avgScore: number | null;
  /** Lowest transition score and where (index of the transition: 0 = between the first two). */
  minScore: number | null;
  minAt: number | null;
  /** Transitions that break a key or tempo limit. */
  violations: number;
  /** The search cost (transitions + energy curve). */
  cost: number;
};

/** Scores an order as it stands (the strip, the totals). `byId` must hold every id in `order`. */
export function evaluate(byId: ReadonlyMap<number, SetTrack>, order: readonly number[], o: SolverOptions): SetSummary {
  const tracks = order.map((id) => byId.get(id)).filter((t): t is SetTrack => !!t);
  const transitions: Transition[] = [];
  for (let i = 0; i + 1 < tracks.length; i++) transitions.push(transitionBetween(tracks[i], tracks[i + 1], o));
  let cost = transitions.reduce((s, t) => s + costOf(t), 0);
  tracks.forEach((t, i) => (cost += energyCost(t, i, tracks.length, o)));
  let minScore: number | null = null;
  let minAt: number | null = null;
  transitions.forEach((t, i) => {
    if (minScore === null || t.score < minScore) {
      minScore = t.score;
      minAt = i;
    }
  });
  return {
    transitions,
    secs: tracks.reduce((s, t) => s + playSecs(t, o), 0),
    avgScore: transitions.length ? transitions.reduce((s, t) => s + t.score, 0) / transitions.length : null,
    minScore,
    minAt,
    violations: transitions.filter((t) => t.keyOk === false || t.bpmOk === false).length,
    cost,
  };
}

// ---------------------------------------------------------------------------------------------
// Pins
// ---------------------------------------------------------------------------------------------

/** Where each pinned track goes in a set of `n`: its wanted position clamped into the set, kept in
 * the order the user gave and never two on one slot. Needs `pins.length <= n`. Returns [id, slot]. */
export function resolvePins(pins: readonly (readonly [number, number])[], n: number): [number, number][] {
  const sorted = [...pins].sort((a, b) => a[1] - b[1] || a[0] - b[0]).slice(0, n);
  const out: [number, number][] = [];
  let prev = -1;
  sorted.forEach(([id, want], k) => {
    const roomAfter = sorted.length - 1 - k;
    const slot = Math.max(prev + 1, Math.min(Math.max(0, Math.trunc(want)), n - 1 - roomAfter));
    out.push([id, slot]);
    prev = slot;
  });
  return out;
}

// ---------------------------------------------------------------------------------------------
// Arranging a fixed set of tracks
// ---------------------------------------------------------------------------------------------

/** Work counter that stops the local search after a fixed amount of work (not time), so the same
 * input always ends at the same place. */
const LOCAL_SEARCH_BUDGET = 40_000_000;

/** How many greedy starts to try for `free` free tracks. */
function startCount(free: number): number {
  if (free <= 24) return free;
  if (free <= 80) return 12;
  return 4;
}

/**
 * Orders `ids` (all in `byId`) with the `pinned` tracks held at their positions. Greedy
 * construction from several starting tracks, then swap / move / reverse passes until nothing
 * improves or the work budget is spent. Returns the order.
 */
export function arrange(byId: ReadonlyMap<number, SetTrack>, ids: readonly number[], pinned: ReadonlyMap<number, number>, o: SolverOptions): number[] {
  const n = ids.length;
  if (n <= 2 && pinned.size === 0 && !curveActive(ids.map((i) => byId.get(i)!), o)) return [...ids];
  const tr = ids.map((id) => byId.get(id)!);
  const local = new Map(ids.map((id, i) => [id, i]));
  // pair costs, indexed by position in `ids`
  const C: number[][] = tr.map((a) => tr.map((b) => (a === b ? 0 : costOf(transitionBetween(a, b, o)))));
  const useCurve = curveActive(tr, o);
  const E = (i: number, slot: number) => (useCurve ? energyCost(tr[i], slot, n, o) : 0);

  const pinList = resolvePins([...pinned].filter(([id]) => local.has(id)).map(([id, pos]) => [local.get(id)!, pos] as [number, number]), n);
  const slotOf = new Map<number, number>(pinList); // local index -> slot
  const fixed: (number | undefined)[] = Array(n).fill(undefined);
  for (const [i, s] of pinList) fixed[s] = i;
  const freeSlots: number[] = [];
  for (let s = 0; s < n; s++) if (fixed[s] === undefined) freeSlots.push(s);
  const free: number[] = [];
  for (let i = 0; i < n; i++) if (!slotOf.has(i)) free.push(i);

  const full = (assign: number[]): number[] => {
    const seq = fixed.map((x) => x ?? -1);
    freeSlots.forEach((s, k) => (seq[s] = assign[k]));
    return seq;
  };
  const costSeq = (seq: number[]): number => {
    let c = 0;
    for (let s = 0; s < n; s++) {
      if (s > 0) c += C[seq[s - 1]][seq[s]];
      c += E(seq[s], s);
    }
    return c;
  };

  // ---- greedy construction -----------------------------------------------------------------
  const greedy = (first: number | null): number[] => {
    const left = new Set(free);
    const seq: number[] = Array(n).fill(-1);
    for (let s = 0; s < n; s++) {
      if (fixed[s] !== undefined) {
        seq[s] = fixed[s]!;
        continue;
      }
      let pick = -1;
      if (s === 0 && first !== null) pick = first;
      else {
        let best = Infinity;
        const nextFixed = s + 1 < n ? fixed[s + 1] : undefined;
        for (const x of free) {
          if (!left.has(x)) continue;
          let c = E(x, s) + (s > 0 ? C[seq[s - 1]][x] : 0);
          if (nextFixed !== undefined) c += C[x][nextFixed];
          if (c < best - 1e-9) {
            best = c;
            pick = x;
          }
        }
      }
      seq[s] = pick;
      left.delete(pick);
    }
    return seq;
  };

  // candidate first tracks when slot 0 is free: spread over the pool, deterministic
  let starts: (number | null)[];
  if (fixed[0] !== undefined || free.length === 0) starts = [null];
  else {
    const want = startCount(free.length);
    if (want >= free.length) starts = [...free];
    else {
      const byTempo = [...free].sort((a, b) => (tr[a].bpm || 1e9) - (tr[b].bpm || 1e9) || a - b);
      starts = Array.from({ length: want }, (_, k) => byTempo[Math.floor((k * (byTempo.length - 1)) / Math.max(1, want - 1))]);
      starts = [...new Set(starts)];
    }
  }

  let bestSeq: number[] = [];
  let bestCost = Infinity;
  for (const st of starts) {
    const seq = greedy(st);
    const c = costSeq(seq);
    if (c < bestCost - 1e-9) {
      bestCost = c;
      bestSeq = seq;
    }
  }

  // ---- local improvement over the free slots -------------------------------------------------
  let assign = freeSlots.map((s) => bestSeq[s]);
  let cur = bestCost;
  let budget = LOCAL_SEARCH_BUDGET;
  const m = assign.length;
  const tryAssign = (cand: number[]): boolean => {
    budget -= n;
    const c = costSeq(full(cand));
    if (c < cur - 1e-9) {
      assign = cand;
      cur = c;
      return true;
    }
    return false;
  };
  for (let pass = 0; pass < 80 && budget > 0; pass++) {
    let improved = false;
    for (let i = 0; i < m && budget > 0; i++) {
      for (let j = i + 1; j < m && budget > 0; j++) {
        // swap
        const sw = assign.slice();
        [sw[i], sw[j]] = [sw[j], sw[i]];
        if (tryAssign(sw)) improved = true;
        // reverse the stretch i..j
        if (j - i >= 2) {
          const rv = assign.slice(0, i).concat(assign.slice(i, j + 1).reverse(), assign.slice(j + 1));
          if (tryAssign(rv)) improved = true;
        }
        // take the track at j and put it at i
        const mv = assign.slice();
        mv.splice(i, 0, mv.splice(j, 1)[0]);
        if (tryAssign(mv)) improved = true;
        // take the track at i and put it at j
        const mv2 = assign.slice();
        mv2.splice(j, 0, mv2.splice(i, 1)[0]);
        if (tryAssign(mv2)) improved = true;
      }
    }
    if (!improved) break;
  }
  return full(assign).map((i) => ids[i]);
}

// ---------------------------------------------------------------------------------------------
// The whole solve
// ---------------------------------------------------------------------------------------------

export type SolveResult = {
  /** The set, in play order. */
  order: number[];
  /** Tracks of the pool that were dropped to reach the target length, in the order dropped. */
  dropped: number[];
  /** Total play time of `order` (capped per track), seconds. */
  secs: number;
  /** The target was set and the pool is shorter than it, so every track is used. */
  short: boolean;
};

/**
 * Builds a set from `pool` (the tracks the user wants considered, in the order given: that order
 * breaks ties). `pins` maps a track id to the position it must keep; a pinned track is never
 * dropped. With a target length, tracks are dropped (the ones whose removal costs the set least)
 * while that brings the total closer to the target; with no target every track is used.
 */
export function solve(pool: readonly SetTrack[], pins: ReadonlyMap<number, number>, o: SolverOptions): SolveResult {
  const byId = new Map(pool.map((t) => [t.id, t]));
  let ids = pool.map((t) => t.id);
  const pinnedIds = new Set([...pins.keys()].filter((id) => byId.has(id)));
  const secsOf = (id: number) => playSecs(byId.get(id)!, o);
  const dropped: number[] = [];
  let short = false;

  if (o.targetSecs != null && o.targetSecs > 0) {
    let total = ids.reduce((s, id) => s + secsOf(id), 0);
    short = total < o.targetSecs;
    while (!short && total > o.targetSecs) {
      const closer = ids.filter((id) => !pinnedIds.has(id) && Math.abs(total - secsOf(id) - o.targetSecs!) < Math.abs(total - o.targetSecs!) - 1e-9);
      if (closer.length === 0) break;
      // the arrangement the removal gains are measured on: cheap (greedy only, pins respected)
      const quick = greedyOnly(byId, ids, pins, o);
      let pick = -1;
      let bestGain = -Infinity;
      for (const id of closer) {
        const g = removalGain(byId, quick, id, o);
        if (g > bestGain + 1e-9 || (Math.abs(g - bestGain) <= 1e-9 && pick >= 0 && secsOf(id) > secsOf(pick))) {
          bestGain = g;
          pick = id;
        }
      }
      ids = ids.filter((id) => id !== pick);
      dropped.push(pick);
      total -= secsOf(pick);
    }
  }

  const livePins = new Map([...pins].filter(([id]) => ids.includes(id)));
  const order = arrange(byId, ids, livePins, o);
  return { order, dropped, secs: order.reduce((s, id) => s + secsOf(id), 0), short };
}

/** A single greedy pass (no local search), used to rank which track to drop. */
function greedyOnly(byId: ReadonlyMap<number, SetTrack>, ids: readonly number[], pins: ReadonlyMap<number, number>, o: SolverOptions): number[] {
  const n = ids.length;
  const tr = ids.map((id) => byId.get(id)!);
  const local = new Map(ids.map((id, i) => [id, i]));
  const pinList = resolvePins([...pins].filter(([id]) => local.has(id)).map(([id, p]) => [local.get(id)!, p] as [number, number]), n);
  const fixed: (number | undefined)[] = Array(n).fill(undefined);
  for (const [i, s] of pinList) fixed[s] = i;
  const taken = new Set(pinList.map(([i]) => i));
  const left = new Set<number>();
  for (let i = 0; i < n; i++) if (!taken.has(i)) left.add(i);
  const useCurve = curveActive(tr, o);
  const seq: number[] = [];
  for (let s = 0; s < n; s++) {
    if (fixed[s] !== undefined) {
      seq.push(fixed[s]!);
      continue;
    }
    let pick = -1;
    let best = Infinity;
    for (const x of left) {
      let c = useCurve ? energyCost(tr[x], s, n, o) : 0;
      if (s > 0) c += costOf(transitionBetween(tr[seq[s - 1]], tr[x], o));
      if (c < best - 1e-9) {
        best = c;
        pick = x;
      }
    }
    seq.push(pick);
    left.delete(pick);
  }
  return seq.map((i) => ids[i]);
}

/** How much cheaper the set gets when `id` is taken out of `order` (positive = the set improves). */
function removalGain(byId: ReadonlyMap<number, SetTrack>, order: readonly number[], id: number, o: SolverOptions): number {
  const i = order.indexOf(id);
  const t = byId.get(id)!;
  const prev = i > 0 ? byId.get(order[i - 1]) : undefined;
  const next = i + 1 < order.length ? byId.get(order[i + 1]) : undefined;
  let gain = 0;
  if (prev) gain += costOf(transitionBetween(prev, t, o));
  if (next) gain += costOf(transitionBetween(t, next, o));
  if (prev && next) gain -= costOf(transitionBetween(prev, next, o));
  return gain + energyCost(t, i, order.length, o);
}

// ---------------------------------------------------------------------------------------------
// Editing an order by hand
// ---------------------------------------------------------------------------------------------

/** Moves the `moving` ids (kept in their current relative order) so they sit before position `at`
 * of the original list. Returns the same array when nothing changes. */
export function moveBlock(order: readonly number[], moving: ReadonlySet<number>, at: number): number[] {
  const block = order.filter((id) => moving.has(id));
  const rest = order.filter((id) => !moving.has(id));
  const into = order.slice(0, Math.max(0, at)).filter((id) => !moving.has(id)).length;
  const next = [...rest.slice(0, into), ...block, ...rest.slice(into)];
  return next.every((id, i) => id === order[i]) ? (order as number[]) : next;
}

/** The position in `order` where adding `track` (which `byId` must hold) costs the set least (0..order.length). Pinned
 * tracks are not special-cased here: the caller re-reads their positions from the new order. */
export function bestInsert(byId: ReadonlyMap<number, SetTrack>, order: readonly number[], track: SetTrack, o: SolverOptions): number {
  let best = order.length;
  let bestCost = Infinity;
  for (let at = 0; at <= order.length; at++) {
    const next = [...order.slice(0, at), track.id, ...order.slice(at)];
    const c = evaluate(byId, next, o).cost;
    if (c < bestCost - 1e-9) {
      bestCost = c;
      best = at;
    }
  }
  return best;
}

// ---------------------------------------------------------------------------------------------
// Words for the totals
// ---------------------------------------------------------------------------------------------

/** "58:30" / "1:02:10". */
export function clock(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}` : `${m}:${String(r).padStart(2, "0")}`;
}

/** The set's length against the target, in words. */
export function lengthLine(secs: number, o: Pick<SolverOptions, "targetSecs">, short = false): string {
  if (o.targetSecs == null || o.targetSecs <= 0) return `${clock(secs)} of music`;
  const diff = secs - o.targetSecs;
  if (Math.abs(diff) < 1) return `${clock(secs)}, exactly the ${clock(o.targetSecs)} target`;
  const tail = diff > 0 ? `${clock(diff)} over the ${clock(o.targetSecs)} target` : `${clock(-diff)} under the ${clock(o.targetSecs)} target`;
  return short ? `${clock(secs)}, ${tail} (every track is used; there is not enough music)` : `${clock(secs)}, ${tail}`;
}

// ---------------------------------------------------------------------------------------------
// Settings (what the user can change; remembered by prefs.svelte.ts)
// ---------------------------------------------------------------------------------------------

export type SetPrefs = {
  allowedMoves: KeyMoveId[];
  maxBpmJumpPct: number;
  halfDouble: boolean;
  keyWeight: number;
  curve: EnergyCurve;
  peakAt: number;
  energyWeight: number;
  /** Minutes; null = whole tracks. */
  maxTrackMins: number | null;
  /** Minutes; null = use every track. */
  targetMins: number | null;
  /** Order the set as soon as a source is loaded. */
  solveOnLoad: boolean;
  /** Dragging a track to a new place also locks it there. */
  lockOnDrag: boolean;
  /** Creating a mix: beat-match each track to the one before it. */
  beatmatch: boolean;
  /** Creating a mix: the tempo it runs at, BPM; null = the middle tempo of the set. */
  mixTempo: number | null;
};

export const DEFAULT_PREFS: SetPrefs = {
  allowedMoves: [...DEFAULT_SOLVER.allowedMoves] as KeyMoveId[],
  maxBpmJumpPct: DEFAULT_SOLVER.maxBpmJumpPct,
  halfDouble: DEFAULT_SOLVER.halfDouble,
  keyWeight: DEFAULT_SOLVER.keyWeight,
  curve: DEFAULT_SOLVER.curve,
  peakAt: DEFAULT_SOLVER.peakAt,
  energyWeight: DEFAULT_SOLVER.energyWeight,
  maxTrackMins: null,
  targetMins: null,
  solveOnLoad: true,
  lockOnDrag: true,
  beatmatch: true,
  mixTempo: null,
};

const num = (v: unknown, lo: number, hi: number): number | null => (typeof v === "number" && Number.isFinite(v) && v >= lo && v <= hi ? v : null);

/** Saved settings merged over the defaults; anything missing, unknown or out of range falls back,
 * so an old or corrupt entry never breaks the view. */
export function mergePrefs(saved: unknown): SetPrefs {
  const out: SetPrefs = { ...DEFAULT_PREFS, allowedMoves: [...DEFAULT_PREFS.allowedMoves] };
  if (!saved || typeof saved !== "object") return out;
  const s = saved as Record<string, unknown>;
  if (Array.isArray(s.allowedMoves)) {
    out.allowedMoves = KEY_MOVES.map((m) => m.id).filter((id) => (s.allowedMoves as unknown[]).includes(id));
  }
  out.maxBpmJumpPct = num(s.maxBpmJumpPct, 0, 50) ?? out.maxBpmJumpPct;
  if (typeof s.halfDouble === "boolean") out.halfDouble = s.halfDouble;
  out.keyWeight = num(s.keyWeight, 0, 1) ?? out.keyWeight;
  if (s.curve === "off" || s.curve === "arc") out.curve = s.curve;
  out.peakAt = num(s.peakAt, 0.3, 0.9) ?? out.peakAt;
  out.energyWeight = num(s.energyWeight, 0, 1) ?? out.energyWeight;
  out.maxTrackMins = s.maxTrackMins === null ? null : (num(s.maxTrackMins, 0.25, 120) ?? out.maxTrackMins);
  out.targetMins = s.targetMins === null ? null : (num(s.targetMins, 1, 1440) ?? out.targetMins);
  if (typeof s.solveOnLoad === "boolean") out.solveOnLoad = s.solveOnLoad;
  if (typeof s.lockOnDrag === "boolean") out.lockOnDrag = s.lockOnDrag;
  if (typeof s.beatmatch === "boolean") out.beatmatch = s.beatmatch;
  out.mixTempo = s.mixTempo === null ? null : (num(s.mixTempo, 40, 250) ?? out.mixTempo);
  return out;
}

/** The solver's options for these settings (minutes become seconds). */
export function solverOptions(p: SetPrefs): SolverOptions {
  return {
    allowedMoves: p.allowedMoves,
    maxBpmJumpPct: p.maxBpmJumpPct,
    halfDouble: p.halfDouble,
    keyWeight: p.keyWeight,
    curve: p.curve,
    peakAt: p.peakAt,
    energyWeight: p.energyWeight,
    maxTrackSecs: p.maxTrackMins != null ? p.maxTrackMins * 60 : null,
    targetSecs: p.targetMins != null ? p.targetMins * 60 : null,
  };
}

// ---------------------------------------------------------------------------------------------
// Planning a mix from a set
// ---------------------------------------------------------------------------------------------
// What "create a mix" will do to each hop, worked out here (not in the builder) so the dialog can
// show it before anything is written and the builder then follows the very same plan.

/** "blend": the next track overlaps the last and gets a transition (beat-matched when asked).
 * "sequence": it starts when the last one ends, with no overlap. */
export type PairMode = "blend" | "sequence";

export type PairPlan = {
  from: number;
  to: number;
  mode: PairMode;
  /** Why a hop is a sequence, as a code the dialog words: no beat grid / no tempo / half or double time. */
  why: "noGrid" | "noTempo" | "halfDouble" | null;
  /** The track the reason is about (for "noGrid" / "noTempo" / "halfDouble"). */
  culprit: number | null;
};

export type MixPlan = {
  pairs: PairPlan[];
  /** The tempo each track plays at in the mix, BPM; null when its tempo is unknown. A track whose
   * hop to the next is a sequence keeps its own tempo unless a blend run starts on it. */
  tempos: (number | null)[];
  /** How far each track is stretched from its own tempo, percent (0 = untouched). */
  stretchPct: number[];
};

export type MixPlanOptions = {
  /** Beat-match each track to the one before it (tempo and beats). */
  beatmatch: boolean;
  /** The tempo the mix runs at, BPM; null = the middle tempo of the tracks that can be beat-matched
   * (it keeps the biggest stretch smallest). Only used with beat-matching. */
  mixTempo: number | null;
  halfDouble: boolean;
};

/** The middle tempo of `tracks` that have a tempo and a grid; a track at half or double time of
 * the first is counted at the first's octave. null when none qualifies. */
export function middleTempo(tracks: readonly SetTrack[], hasGrid: (id: number) => boolean, halfDouble: boolean): number | null {
  const usable = tracks.filter((t) => known(t.bpm) && hasGrid(t.id));
  if (usable.length === 0) return null;
  const ref = usable[0].bpm;
  const v = usable.map((t) => bpmMatch(ref, t.bpm, halfDouble).used).sort((a, b) => a - b);
  const mid = v.length >> 1;
  return Math.round((v.length % 2 ? v[mid] : (v[mid - 1] + v[mid]) / 2) * 100) / 100;
}

/**
 * Plans the hops of `tracks` (in play order). `hasGrid` says whether a track has a beat grid.
 * Consecutive tracks joined by blends form a run; with beat-matching a run plays at one tempo (the
 * one given, else the middle tempo of the run), and a track that starts a new run keeps its own.
 */
export function planMix(tracks: readonly SetTrack[], hasGrid: (id: number) => boolean, o: MixPlanOptions): MixPlan {
  const n = tracks.length;
  const pairs: PairPlan[] = [];
  for (let i = 1; i < n; i++) {
    const a = tracks[i - 1];
    const t = tracks[i];
    const seq = (why: PairPlan["why"], culprit: number) => pairs.push({ from: a.id, to: t.id, mode: "sequence", why, culprit });
    if (!hasGrid(a.id)) seq("noGrid", a.id);
    else if (!hasGrid(t.id)) seq("noGrid", t.id);
    else if (!known(a.bpm)) seq("noTempo", a.id);
    else if (!known(t.bpm)) seq("noTempo", t.id);
    else if (bpmMatch(a.bpm, t.bpm, o.halfDouble).relation !== "same") seq("halfDouble", t.id);
    else pairs.push({ from: a.id, to: t.id, mode: "blend", why: null, culprit: null });
  }
  const tempos: (number | null)[] = [];
  const stretchPct: number[] = [];
  let i = 0;
  while (i < n) {
    let j = i;
    while (j + 1 < n && pairs[j].mode === "blend") j++;
    const run = tracks.slice(i, j + 1);
    const target = o.beatmatch && run.length > 1 ? (o.mixTempo ?? middleTempo(run, hasGrid, o.halfDouble)) : null;
    for (const t of run) {
      const own = known(t.bpm) ? t.bpm : null;
      const tempo = target ?? own;
      tempos.push(tempo);
      stretchPct.push(own !== null && tempo !== null ? (tempo / own - 1) * 100 : 0);
    }
    i = j + 1;
  }
  return { pairs, tempos, stretchPct };
}
