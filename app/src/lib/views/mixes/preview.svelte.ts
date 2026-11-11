// Mix preview: renders a window of the open (possibly unsaved) document with the audio area's
// `export_render_preview` and plays the temporary file. The playhead follows the audio clock.
//
// The player bar (state/playback.svelte.ts) has no way to be handed a rendered file yet, so the
// editor plays it with its own <audio> element and only silences the track player first; see the
// integration request in the hand-off note. When rendering is unavailable (browser preview, or a
// build without the mixer) `notice` says so and nothing plays.
import { convertFileSrc } from "@tauri-apps/api/core";
import { mixes } from "$lib/state/mixes.svelte";
import { playback } from "$lib/state/playback.svelte";
import { session } from "$lib/state/session.svelte";
import type { MixDoc } from "$lib/api/mixes";
import { SR, toSamples } from "./layout";
import { mv } from "./view.svelte";

/** Length of one rendered window; playback continues into the next window by itself. */
const WINDOW_SECS = 180;

export type PreviewStatus = "idle" | "rendering" | "playing" | "paused";

class Preview {
  status = $state<PreviewStatus>("idle");
  /** Why nothing plays (rendering unavailable, file could not be played), or "". */
  notice = $state("");
  /** The document the audio was rendered from; differs from `mixes.doc` after an edit. */
  renderedDoc = $state.raw<MixDoc | null>(null);

  #audio: HTMLAudioElement | null = null;
  /** Mix time (seconds) at which the loaded file starts, and its length. */
  #from = 0;
  #span = 0;
  #token = 0;
  #raf = 0;

  get playing() {
    return this.status === "playing";
  }

  get stale() {
    return this.renderedDoc !== null && this.renderedDoc !== mixes.doc;
  }

  /** Plays from the playhead (or `fromSamples`), rendering first when the loaded audio is out of
   * date or does not cover that point. */
  async play(fromSamples: number = mv.playhead) {
    const doc = mixes.doc;
    if (!doc) return;
    if (doc.clips.length === 0) {
      this.notice = "Add a track before playing.";
      return;
    }
    playback.pause();
    this.notice = "";
    const endSec = doc.lengthSec;
    let startSec = fromSamples / SR;
    if (startSec >= endSec - 0.05) startSec = 0;
    const loaded = this.#audio && this.renderedDoc === doc;
    const inside = startSec >= this.#from - 0.01 && startSec <= this.#from + this.#span - 0.2;
    if (loaded && inside && this.#audio) {
      this.#audio.currentTime = Math.max(0, startSec - this.#from);
      await this.#start();
      return;
    }
    await this.#render(startSec);
  }

  async #render(startSec: number) {
    const doc = mixes.doc;
    if (!doc) return;
    const token = ++this.#token;
    this.#halt();
    this.status = "rendering";
    mv.playhead = toSamples(startSec);
    try {
      const to = Math.min(doc.lengthSec, startSec + WINDOW_SECS);
      const { path } = await mixes.renderPreview(startSec, to);
      if (token !== this.#token) return;
      const audio = new Audio(convertFileSrc(path));
      audio.preload = "auto";
      audio.onended = () => void this.#ended();
      audio.onerror = () => {
        if (token !== this.#token) return;
        this.#fail("The rendered preview could not be played (the file is not readable by the webview).");
      };
      this.#audio = audio;
      this.#from = startSec;
      this.#span = to - startSec;
      this.renderedDoc = doc;
      await this.#start();
    } catch (e) {
      if (token !== this.#token) return;
      this.#fail(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    }
  }

  async #start() {
    const audio = this.#audio;
    if (!audio) return;
    try {
      await audio.play();
      this.status = "playing";
      this.#tick();
    } catch (e) {
      this.#fail(`Playback was blocked: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  #fail(message: string) {
    this.#halt();
    this.status = "idle";
    this.notice = message;
  }

  async #ended() {
    const doc = mixes.doc;
    if (!doc) return;
    const next = this.#from + this.#span;
    if (this.renderedDoc === doc && next < doc.lengthSec - 0.1) {
      await this.#render(next);
    } else {
      this.status = "paused";
      cancelAnimationFrame(this.#raf);
      mv.playhead = toSamples(Math.min(next, doc.lengthSec));
    }
  }

  #tick = () => {
    cancelAnimationFrame(this.#raf);
    if (this.status !== "playing" || !this.#audio) return;
    mv.playhead = toSamples(this.#from + this.#audio.currentTime);
    this.#raf = requestAnimationFrame(this.#tick);
  };

  pause() {
    this.#token++;
    if (this.#audio) this.#audio.pause();
    cancelAnimationFrame(this.#raf);
    if (this.status === "playing" || this.status === "rendering") this.status = "paused";
  }

  async toggle() {
    if (this.status === "playing" || this.status === "rendering") this.pause();
    else await this.play();
  }

  /** Moves the playhead; if playing, playback jumps there (re-rendering when out of the window). */
  async seek(samples: number) {
    const s = Math.max(0, samples);
    mv.playhead = s;
    if (this.status === "playing" || this.status === "rendering") await this.play(s);
  }

  #halt() {
    cancelAnimationFrame(this.#raf);
    if (this.#audio) {
      this.#audio.onended = null;
      this.#audio.onerror = null;
      this.#audio.pause();
      this.#audio = null;
    }
  }

  /** Stops and forgets the rendered audio (a different mix was opened, or the view closed). */
  stop() {
    this.#token++;
    this.#halt();
    this.status = "idle";
    this.notice = "";
    this.renderedDoc = null;
  }
}

export const preview = new Preview();

session.onReset(() => preview.stop());
