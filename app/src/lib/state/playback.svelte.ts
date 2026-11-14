// The playback transport: what is loaded, whether it plays, where it is. Owned by the playback
// area (the engine in lib/player reads and drives these fields); every other area, the mix
// editor and the grid editor included, only calls the methods and reads the fields. This is
// the one place the "now playing" state lives, so views never talk to the engine directly.
import { tableRows } from "$lib/views/tracks/tableRows.svelte";
import { session } from "./session.svelte";

/** A rendered mix being previewed instead of a single track. `path` is the rendered audio file
 * (from export.renderMixPreview); `fromSecs` is where that file starts on the mix timeline. */
export type MixPreview = { path: string; mixId: number | null; label: string; fromSecs: number };

export type Loop = { start: number; end: number };

class Playback {
  /** Track loaded in the player, or null. */
  trackId = $state<number | null>(null);
  playing = $state(false);
  /** Seconds from the start of the loaded track (or of the mix timeline when previewing one). */
  position = $state(0);
  /** Seconds, 0 until the engine knows it. */
  duration = $state(0);
  /** Set while a mix is being previewed through the transitions instead of a single track. */
  mixPreview = $state<MixPreview | null>(null);

  /** Player volume, 0 to 1. */
  volume = $state(0.8);
  /** The device-style cue point of the loaded track, seconds. Starts at the first downbeat of
   * the .str; the grid editor may set it with `setCue`. Not read from `tracks.cue_point`: that
   * column's unit is unverified (notes/FINDINGS.md section 4c). */
  cue = $state(0);
  /** The active beat loop, seconds, or null. */
  loop = $state<Loop | null>(null);
  /** Why the loaded track cannot play (file missing, cannot decode), or "". */
  error = $state("");
  /** Bumped by `retry`: the engine opens the loaded file again even though nothing else changed. */
  reloads = $state(0);

  /** Track ids that next/previous walk through; empty = the Tracks table as it is shown now. */
  queue = $state<number[]>([]);
  /** A seek the engine has not applied yet (seconds on the same scale as `position`). */
  seekRequest = $state<{ secs: number } | null>(null);

  load(trackId: number, autoplay = false, queue?: number[]) {
    if (queue) this.queue = queue;
    // loading the track that is already loaded only restarts it: the engine does not reload it,
    // so its cue point (the default first downbeat, or one the user set) stays as it is
    const same = this.trackId === trackId && this.mixPreview === null;
    // asking for the track that failed again is a retry: the file may be back
    if (same && this.error) this.reloads++;
    this.mixPreview = null;
    this.trackId = trackId;
    this.position = 0;
    if (!same) {
      this.duration = 0;
      this.cue = 0;
      this.loop = null;
    }
    this.error = "";
    this.seekRequest = { secs: 0 };
    this.playing = autoplay;
  }

  play() {
    // nothing is loaded in the engine after an error: "playing" would show Pause over silence and
    // tell background analysis a preview is audible. Retry opens the file again.
    if (this.error) return;
    this.playing = this.trackId !== null || this.mixPreview !== null;
  }

  pause() {
    this.playing = false;
  }

  /** Opens the loaded track (or mix preview) again after an error, and plays it. */
  retry() {
    if (this.trackId === null && this.mixPreview === null) return;
    this.error = "";
    this.reloads++;
    this.seekRequest = { secs: this.mixPreview?.fromSecs ?? 0 };
    this.position = this.mixPreview?.fromSecs ?? 0;
    this.playing = true;
  }

  toggle() {
    // after an error Space does what the bar's Try again button does
    if (this.error) return this.retry();
    if (this.playing) this.pause();
    else this.play();
  }

  /** Move to `secs`; the engine clamps it. */
  seek(secs: number) {
    const s = Math.max(0, secs);
    this.position = s;
    this.seekRequest = { secs: s };
  }

  setCue(secs: number) {
    this.cue = Math.max(0, secs);
  }

  setVolume(v: number) {
    this.volume = Math.min(1, Math.max(0, v));
  }

  /** Preview a rendered mix (the file `export.renderMixPreview` returned) from its start.
   * `fromSecs` is where that file begins on the mix timeline, so `position` reads in mix time. */
  previewMix(path: string, opts: { mixId?: number; label?: string; fromSecs?: number } = {}) {
    const fromSecs = opts.fromSecs ?? 0;
    this.trackId = null;
    this.mixPreview = { path, mixId: opts.mixId ?? null, label: opts.label ?? "Mix preview", fromSecs };
    this.position = fromSecs;
    this.duration = 0;
    this.cue = 0;
    this.loop = null;
    this.error = "";
    this.seekRequest = null;
    this.playing = true;
  }

  /** The ids next/previous walk through. */
  #order(): number[] {
    if (this.queue.length) return this.queue;
    // no list was given: the library as the Tracks table shows it right now (search, filters and
    // every sort key). It is the table's own cached list, recomputed only when something it
    // reads changed, so a change made while a track plays still counts
    return session.library ? tableRows.ids : [];
  }

  /** The track after the loaded one in the list, or null at the end / when nothing is loaded. */
  neighbour(step: 1 | -1): number | null {
    if (this.trackId === null) return null;
    const order = this.#order();
    const i = order.indexOf(this.trackId);
    if (i < 0) return null;
    return order[i + step] ?? null;
  }

  /** Next track in the list, keeping the playing state. */
  next(): boolean {
    const id = this.neighbour(1);
    if (id === null) return false;
    this.load(id, this.playing);
    return true;
  }

  /** Previous track; more than three seconds in, it restarts the loaded track instead. */
  previous(): boolean {
    if (this.position > 3) {
      this.seek(0);
      return true;
    }
    const id = this.neighbour(-1);
    if (id === null) {
      this.seek(0);
      return false;
    }
    this.load(id, this.playing);
    return true;
  }

  stop() {
    this.playing = false;
    this.trackId = null;
    this.mixPreview = null;
    this.position = 0;
    this.duration = 0;
    this.cue = 0;
    this.loop = null;
    this.error = "";
    this.seekRequest = null;
    this.queue = [];
  }
}

export const playback = new Playback();
// A different library has different track ids: whatever was loaded no longer exists.
session.onReset(() => playback.stop());
