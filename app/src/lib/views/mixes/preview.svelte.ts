// Mix preview: renders a window of the open (possibly unsaved) document with the audio area's
// `export_render_preview` and hands the temporary file to the shared player (`playback.previewMix`),
// so the editor and the player bar are one transport. The playhead follows `playback.position`,
// which is in mix time. When rendering is unavailable (browser preview, or a build without the
// mixer) `notice` says so and nothing plays.
import { mixExport } from "$lib/state/mixexport.svelte";
import { mixes } from "$lib/state/mixes.svelte";
import { playback } from "$lib/state/playback.svelte";
import { session } from "$lib/state/session.svelte";
import { vocal } from "$lib/state/vocal.svelte";
import type { VocalMode } from "$lib/api/device";
import type { MixDoc } from "$lib/api/mixes";
import { SR, toSamples } from "./layout";
import { sameRender } from "./previewState";
import { mv } from "./view.svelte";

/** Length of one rendered window; playback continues into the next window by itself. */
const WINDOW_SECS = 180;

/** The longest stretch one range preview renders (the backend's limit), seconds. */
const MAX_RANGE_PREVIEW = 900;

export type PreviewStatus = "idle" | "rendering" | "playing" | "paused";

class Preview {
  status = $state<PreviewStatus>("idle");
  /** Why nothing plays (rendering unavailable, file could not be played), or "". */
  notice = $state("");
  /** The document the audio was rendered from; differs from `mixes.doc` after an edit. */
  renderedDoc = $state.raw<MixDoc | null>(null);
  /** The card's Vocal mode the audio was rendered with. A Wah tool sounds different per mode, so a
   * change of the flag makes the rendered audio out of date like an edit does. */
  renderedMode = $state<VocalMode>("off");

  /** The rendered file this editor handed to the player, or null. */
  #path: string | null = null;
  #loaded = false;
  #lastPos = 0;
  /** Mix time (seconds) at which the loaded file starts, and its length. */
  #from = 0;
  #span = 0;
  #token = 0;
  #raf = 0;
  /** The loudness gains the loaded file was rendered with (`mixExport.gainsKey()`; "" = none). */
  #gainsKey = "";
  /** A range preview is loaded: it ends at this mix time and does not run on into the next window. */
  #limit: number | null = null;

  /** Preview level 0..1 (the player's volume; a listening aid, not part of the mix). */
  get volume() {
    return playback.volume;
  }
  set volume(v: number) {
    playback.setVolume(v);
  }

  get playing() {
    return this.status === "playing";
  }

  get stale() {
    return this.renderedDoc !== null && (!sameRender(this.renderedDoc, this.renderedMode, mixes.doc, vocal.mode) || this.#gainsKey !== mixExport.gainsKey());
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
    this.notice = "";
    const endSec = doc.lengthSec;
    let startSec = fromSamples / SR;
    if (startSec >= endSec - 0.05) startSec = 0;
    const loaded = this.#loaded && this.#owns() && sameRender(this.renderedDoc, this.renderedMode, doc, vocal.mode) && this.#limit === null && this.#gainsKey === mixExport.gainsKey();
    const inside = startSec >= this.#from - 0.01 && startSec <= this.#from + this.#span - 0.2;
    if (loaded && inside) {
      playback.seek(startSec);
      this.#start();
      return;
    }
    await this.#render(startSec);
  }

  /** Plays a stretch of the mix once, as a partial export makes it: at the level of the current
   * loudness analysis (when there is one) and with the fade at its cuts. It stops at `toSec`. */
  async playRange(fromSec: number, toSec: number) {
    const doc = mixes.doc;
    if (!doc || doc.clips.length === 0) return;
    this.notice = "";
    const to = Math.min(toSec, fromSec + MAX_RANGE_PREVIEW);
    if (to < toSec) this.notice = "Only the first 15 minutes of the range are previewed.";
    await this.#render(fromSec, { to, fadeMs: mixExport.prefs.edgeFadeMs });
  }

  async #render(startSec: number, range?: { to: number; fadeMs: number }) {
    const doc = mixes.doc;
    if (!doc) return;
    const token = ++this.#token;
    // the mode read before the render starts: if the flag changes while it runs, the audio counts as
    // out of date and the next play renders again
    const mode = vocal.mode;
    this.#halt();
    this.status = "rendering";
    mv.playhead = toSamples(startSec);
    const gains = mixExport.previewGains();
    const gainsKey = mixExport.gainsKey();
    try {
      const to = range ? range.to : Math.min(doc.lengthSec, startSec + WINDOW_SECS);
      const { path } = await mixes.renderPreview(startSec, to, gains, range?.fadeMs);
      if (token !== this.#token) return;
      this.#gainsKey = gainsKey;
      this.#limit = range ? range.to : null;
      this.#path = path;
      this.#loaded = true;
      this.#from = startSec;
      this.#span = to - startSec;
      this.renderedDoc = doc;
      this.renderedMode = mode;
      this.#lastPos = startSec;
      playback.previewMix(path, { mixId: doc.id, label: doc.name || "Mix preview", fromSecs: startSec });
      this.#start();
    } catch (e) {
      if (token !== this.#token) return;
      this.#fail(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    }
  }

  /** True while the player still holds the file this editor rendered. */
  #owns() {
    return this.#path !== null && playback.mixPreview?.path === this.#path;
  }

  #start() {
    playback.play();
    this.status = "playing";
    this.#tick();
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
    if (this.#limit === null && sameRender(this.renderedDoc, this.renderedMode, doc, vocal.mode) && this.#gainsKey === mixExport.gainsKey() && next < doc.lengthSec - 0.1) {
      await this.#render(next);
    } else {
      this.status = "paused";
      cancelAnimationFrame(this.#raf);
      mv.playhead = toSamples(Math.min(next, doc.lengthSec));
    }
  }

  #tick = () => {
    cancelAnimationFrame(this.#raf);
    if (this.status !== "playing") return;
    if (!this.#owns()) {
      // something else took the player (a library track): this preview is over
      this.#loaded = false;
      this.status = "paused";
      return;
    }
    if (!playback.playing) {
      // the player stopped by itself: it reached the end of the rendered window
      if (this.#lastPos >= this.#from + this.#span - 0.6) void this.#ended();
      else this.status = "paused";
      return;
    }
    this.#lastPos = playback.position;
    mv.playhead = toSamples(playback.position);
    this.#raf = requestAnimationFrame(this.#tick);
  };

  pause() {
    this.#token++;
    if (this.#owns()) playback.pause();
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
    if (this.#owns()) playback.pause();
    this.#loaded = false;
  }

  /** Stops and forgets the rendered audio (a different mix was opened, or the view closed). */
  stop() {
    this.#token++;
    this.#halt();
    this.#path = null;
    this.#limit = null;
    this.#gainsKey = "";
    this.status = "idle";
    this.notice = "";
    this.renderedDoc = null;
    this.renderedMode = "off";
  }
}

export const preview = new Preview();

session.onReset(() => preview.stop());
