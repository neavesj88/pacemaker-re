<script lang="ts">
  // Old-vs-new beat grid of one track in a batch preview: the waveform twice, the old beat lines on
  // the first strip and the new ones on the second, a strip that puts both sets of ticks side by
  // side, and the numbers (tempo, first beat, how many lines stay, move, are added or dropped).
  // Reads the two grids the backend kept for the preview, so what is drawn is what would be saved.
  import { peaks as fetchPeaks, type Peak } from "$lib/api/analysis";
  import { gridBatchDiff, type BatchDiff, type BatchRow } from "$lib/api/grid";
  import { fmtBpm } from "$lib/state/batchLogic";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { diffGrids, diffHeadline } from "./gridDiff";
  import { bandsOver, clamp, fmtTime, isDownbeat, lowerBound, tickStep } from "./gridMath";

  let { root, planId, row }: { root: string; planId: number; row: BatchRow } = $props();

  const LANE = 58;
  const GAP = 6;
  const STRIP = 30;
  const RULER = 16;
  const HEIGHT = LANE * 2 + GAP * 2 + STRIP + RULER;

  let diff = $state<BatchDiff | null>(null);
  let error = $state("");
  let wave = $state<Peak[] | null>(null);
  let span = $state(20);
  let start = $state(0);
  let width = $state(0);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let host = $state<HTMLDivElement | null>(null);

  const rate = $derived(diff?.sample_rate ?? 44100);
  const duration = $derived(diff ? diff.total_frames / diff.sample_rate : 0);
  const stats = $derived(diff ? diffGrids(diff.before, diff.after, diff.sample_rate) : null);
  const location = $derived(session.library?.tracks.find((t) => t.track_id === row.track_id)?.location ?? "");
  const maxStart = $derived(Math.max(0, duration - span));

  // load the two grids for the selected row
  $effect(() => {
    const id = row.track_id;
    let current = true;
    diff = null;
    error = "";
    wave = null;
    gridBatchDiff(root, planId, id)
      .then((d) => {
        if (!current) return;
        diff = d;
        // open on the first beat line of the new grid (a second of lead-in)
        const first = d.after.beats[0]?.pos ?? 0;
        start = clamp(first / d.sample_rate - 1, 0, Math.max(0, d.total_frames / d.sample_rate - span));
      })
      .catch((e) => {
        if (current) error = String(e);
      });
    return () => {
      current = false;
    };
  });

  // waveform columns for the visible window (a failure just leaves the strips without a waveform)
  $effect(() => {
    const loc = location;
    const total = duration;
    const w = width;
    if (!loc || total <= 0 || w <= 0) return;
    let current = true;
    const columns = clamp(Math.ceil((total / span) * w), 1000, 16384);
    fetchPeaks(root, loc, columns)
      .then((p) => {
        if (current) wave = p.length ? p : null;
      })
      .catch(() => {
        if (current) wave = null;
      });
    return () => {
      current = false;
    };
  });

  const css = (name: string) => (host ? getComputedStyle(host).getPropertyValue(name).trim() || "#888" : "#888");

  function lines(g: CanvasRenderingContext2D, beats: { pos: number }[], fd: number | null, y: number, h: number, colour: string, bar: string) {
    const t0 = start * rate;
    const t1 = (start + span) * rate;
    const px = width / (span * rate);
    let i = Math.max(0, lowerBound(beats.map((b) => b.pos), t0) - 1);
    for (; i < beats.length && beats[i].pos <= t1; i++) {
      const x = Math.round((beats[i].pos - t0) * px);
      if (x < 0 || x > width) continue;
      const down = isDownbeat(i, fd);
      g.fillStyle = down ? bar : colour;
      g.globalAlpha = down ? 1 : 0.7;
      g.fillRect(x, down ? y : y + h * 0.2, down ? 2 : 1, down ? h : h * 0.6);
    }
    g.globalAlpha = 1;
  }

  function lane(g: CanvasRenderingContext2D, y: number) {
    g.fillStyle = css("--deep");
    g.fillRect(0, y, width, LANE);
    if (!wave || duration <= 0) return;
    const cols = [css("--wave-low"), css("--wave-mid"), css("--wave-high")];
    const scale = [1, 0.75, 0.45];
    const mid = y + LANE / 2;
    const half = LANE / 2 - 2;
    g.globalAlpha = 0.55;
    for (let x = 0; x < width; x++) {
      const f0 = (start + (x / width) * span) / duration;
      const f1 = (start + ((x + 1) / width) * span) / duration;
      if (f0 >= 1) break;
      const b = bandsOver(wave, f0, f1);
      for (let k = 0; k < 3; k++) {
        const h = (b[k] / 255) * half * scale[k];
        if (h > 0) {
          g.fillStyle = cols[k];
          g.fillRect(x, mid - h, 1, h * 2);
        }
      }
    }
    g.globalAlpha = 1;
  }

  // paint
  $effect(() => {
    const c = canvas;
    const d = diff;
    const W = width;
    void wave;
    void start;
    void span;
    if (!c || !d || W <= 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(W * dpr);
    c.height = Math.round(HEIGHT * dpr);
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, W, HEIGHT);
    const oldColour = css("--muted");
    const newColour = css("--accent");
    const text = css("--text");

    lane(g, 0);
    if (d.before) lines(g, d.before.beats, d.before.first_downbeat, 0, LANE, oldColour, text);
    const y2 = LANE + GAP;
    lane(g, y2);
    lines(g, d.after.beats, d.after.first_downbeat, y2, LANE, newColour, newColour);

    // both sets of ticks side by side: old above, new below, so a shift reads at a glance
    const y3 = y2 + LANE + GAP;
    g.fillStyle = css("--deep");
    g.fillRect(0, y3, W, STRIP);
    if (d.before) lines(g, d.before.beats, null, y3, STRIP / 2 - 1, oldColour, oldColour);
    lines(g, d.after.beats, null, y3 + STRIP / 2, STRIP / 2 - 1, newColour, newColour);

    // time ruler
    const ry = y3 + STRIP;
    const step = tickStep(W / span, 70);
    g.fillStyle = css("--muted");
    g.font = "10px sans-serif";
    g.textBaseline = "top";
    for (let t = Math.ceil(start / step) * step; t <= start + span; t += step) {
      const x = Math.round(((t - start) / span) * W);
      g.fillRect(x, ry, 1, 4);
      g.fillText(fmtTime(t), x + 3, ry + 3);
    }
  });

  const headline = $derived(stats ? diffHeadline(stats) : "");
  const secs = (v: number | null) => (v === null ? "none" : fmtTime(v));
</script>

<section class="diff" aria-label="Old and new beat grid of {row.label}">
  {#if error}
    <p class="err" role="alert">{error}</p>
  {:else if !diff || !stats}
    <p class="muted">Loading the two grids…</p>
  {:else}
    <div class="legend">
      <span><i class="old"></i> old grid{stats.beatsBefore === 0 ? " (none)" : ""}, {stats.beatsBefore} beat lines</span>
      <span><i class="new"></i> new grid, {stats.beatsAfter} beat lines</span>
      <span class="muted">bar lines are the taller ones</span>
    </div>
    <div class="canvas" bind:this={host} bind:clientWidth={width} role="img" aria-label={headline}>
      <canvas bind:this={canvas} style:width="100%" style:height="{HEIGHT}px" aria-hidden="true"></canvas>
    </div>
    <div class="nav">
      <Button small onclick={() => (start = 0)}>Start</Button>
      <Button small onclick={() => (start = maxStart / 2)}>Middle</Button>
      <Button small onclick={() => (start = maxStart)}>End</Button>
      <input type="range" min="0" max={maxStart} step="0.5" bind:value={start} aria-label="Position in the track" disabled={maxStart <= 0} />
      <label class="muted">
        Window
        <select bind:value={span} aria-label="Seconds shown">
          <option value={10}>10 s</option>
          <option value={20}>20 s</option>
          <option value={40}>40 s</option>
          <option value={80}>80 s</option>
        </select>
      </label>
    </div>
    <p class="head">{headline}</p>
    <dl class="facts">
      <dt>Tempo</dt>
      <dd class="num">{stats.bpmBefore === null ? "none" : fmtBpm(stats.bpmBefore)} → {fmtBpm(stats.bpmAfter)} BPM</dd>
      <dt>First beat</dt>
      <dd class="num">{secs(stats.firstBefore)} → {secs(stats.firstAfter)}</dd>
      <dt>First bar line</dt>
      <dd class="num">{secs(stats.barBefore)} → {secs(stats.barAfter)}</dd>
      <dt>Beat lines</dt>
      <dd class="num">{stats.same} stay · {stats.moved} move · {stats.added} added · {stats.removed} dropped</dd>
    </dl>
    {#if row.note}<p class="muted note">Analyser: {row.note}.</p>{/if}
  {/if}
</section>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    font-size: 12px;
  }
  .legend i {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
    margin-right: 4px;
    vertical-align: -1px;
  }
  .old {
    background: var(--muted);
  }
  .new {
    background: var(--accent);
  }
  .canvas {
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
    background: var(--deep);
  }
  canvas {
    display: block;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .nav input[type="range"] {
    flex: 1;
    min-width: 0;
  }
  .head {
    margin: 0;
    font-weight: 600;
  }
  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 14px;
    margin: 0;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
  }
  .note {
    margin: 0;
    font-size: 12px;
  }
  .err {
    color: var(--bad-text);
    margin: 0;
  }
</style>
