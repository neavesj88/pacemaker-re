// Optional click on every beat while the track plays, so a grid can be judged by ear. WebAudio
// oscillator blips scheduled a little ahead of the playhead; bar lines click higher. The click
// is timed from the transport position (which the player reports in coarse steps), so it is an
// audition aid, not sample-accurate: `latencyMs` shifts it to match the output device.
import { beatsToSchedule } from "./gridMath";

const LOOKAHEAD = 0.14;
const JUMP = 0.4;

export class Metronome {
  #ctx: AudioContext | null = null;
  #scheduledUpTo = -Infinity;
  #lastPos = -Infinity;
  latencyMs = 0;

  /** Must be called from a user gesture (the toggle's click) the first time. */
  async arm() {
    if (!this.#ctx) {
      const Ctor = window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      this.#ctx = new Ctor();
    }
    if (this.#ctx.state === "suspended") await this.#ctx.resume();
  }

  reset() {
    this.#scheduledUpTo = -Infinity;
    this.#lastPos = -Infinity;
  }

  /** Call every ~20 ms while playing with the interpolated playhead (seconds). */
  tick(pos: number, beatSecs: ArrayLike<number>, firstDownbeat: number | null) {
    const ctx = this.#ctx;
    if (!ctx || ctx.state !== "running") return;
    if (pos < this.#lastPos - 0.05 || pos > this.#lastPos + JUMP) this.#scheduledUpTo = pos - 0.002; // seek
    this.#lastPos = pos;
    const horizon = pos + LOOKAHEAD;
    for (const i of beatsToSchedule(beatSecs, this.#scheduledUpTo, horizon)) {
      const when = ctx.currentTime + (beatSecs[i] - pos) - this.latencyMs / 1000;
      this.#click(ctx, Math.max(ctx.currentTime, when), firstDownbeat !== null && i >= firstDownbeat && (i - firstDownbeat) % 4 === 0);
    }
    this.#scheduledUpTo = Math.max(this.#scheduledUpTo, horizon);
  }

  #click(ctx: AudioContext, when: number, bar: boolean) {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.frequency.value = bar ? 1760 : 1100;
    gain.gain.setValueAtTime(0.0001, when);
    gain.gain.exponentialRampToValueAtTime(bar ? 0.6 : 0.35, when + 0.002);
    gain.gain.exponentialRampToValueAtTime(0.0001, when + 0.04);
    osc.connect(gain).connect(ctx.destination);
    osc.start(when);
    osc.stop(when + 0.05);
  }

  close() {
    void this.#ctx?.close();
    this.#ctx = null;
    this.reset();
  }
}
