// Mix transport. The Mix tab plays through the LIVE engine (api/mixLive.ts, crates/pm-mixer live
// player): no render step. Play, pause and seek are instant, an edit is sent to the engine about
// 30 ms after it is made and is heard while playing, and the playhead follows the engine's
// `mix-live-position` events. The decoded audio stays in the engine while the mix is open, so the
// second play is instant too.
//
// One thing still renders: `playRange` ("Preview this part" in the export dialog) plays a stretch
// as the partial export makes it (cut fades, export loudness) through the player bar, as before.
import { mixLiveLoad, mixLivePause, mixLivePlay, mixLiveSeek, mixLiveSetMaster, mixLiveUnload, mixLiveUpdate, onMixLivePosition, onMixLiveStatus, type MixLiveStatus } from "$lib/api/mixLive";
import { mixExport } from "$lib/state/mixexport.svelte";
import { mixes } from "$lib/state/mixes.svelte";
import { playback } from "$lib/state/playback.svelte";
import { session } from "$lib/state/session.svelte";
import { vocal } from "$lib/state/vocal.svelte";
import type { VocalMode } from "$lib/api/device";
import type { MixDoc } from "$lib/api/mixes";
import { SR, toSamples } from "./layout";
import { sameRender, volumeToDb } from "./previewState";
import { vocalTools } from "./vocalTools.svelte";
import { mv } from "./view.svelte";

/** The longest stretch one range preview renders (the backend's limit), seconds. */
const MAX_RANGE_PREVIEW = 900;

/** Wait after an edit before it goes to the engine, so a drag sends the last state, not every pixel. */
const EDIT_DEBOUNCE_MS = 30;

export type PreviewStatus = "idle" | "rendering" | "playing" | "paused";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

class Preview {
  status = $state<PreviewStatus>("idle");
  /** Why nothing plays, or something the engine wants said (a clip that is missing, an underrun). */
  notice = $state("");
  /** Decoding progress of the open mix's clips (`loaded` of `total` files), "idle" before a mix is loaded. */
  load = $state.raw<MixLiveStatus>({ state: "idle", loaded: 0, total: 0, message: "" });

  // ---- live engine ----
  /** `root|mix id` of the mix the engine holds, or null. */
  #key: string | null = null;
  /** The document, Vocal mode and loudness gains the engine was last given. */
  #sent: MixDoc | null = null;
  #sentMode: VocalMode = "off";
  #sentGains = "";
  /** The Vocal-tool marks the engine was last given (app-side only: the file has no Vocal tool). */
  #sentVocal = "";
  #listening = false;
  /** Engine calls run one after another, so an update never overtakes the load before it. */
  #chain: Promise<unknown> = Promise.resolve();
  #timer: ReturnType<typeof setTimeout> | undefined;
  /** The player bar is playing a rendered range (`playRange`). */
  #range = false;
  #raf = 0;
  #rangeStart = 0;
  #rangeEnd = 0;
  #token = 0;

  /** Preview level 0..1 (a listening aid, not part of the mix). */
  get volume() {
    return playback.volume;
  }
  set volume(v: number) {
    playback.setVolume(v);
    if (this.#key !== null) void this.#send(() => mixLiveSetMaster(volumeToDb(playback.volume))).catch(() => {});
  }

  get playing() {
    return this.status === "playing";
  }

  /** Whether clips are still being decoded (they join in as soon as they are ready). */
  get loading() {
    return this.load.state === "loading";
  }

  #send<T>(fn: () => Promise<T>): Promise<T> {
    const run = this.#chain.then(fn, fn);
    this.#chain = run.catch(() => {});
    return run;
  }

  #listen() {
    if (this.#listening) return;
    this.#listening = true;
    // a start that throws (no Tauri in a plain browser) is ignored; the commands then fail with a message
    onMixLivePosition((p) => {
      if (this.#range || this.#key === null) return;
      if (p.playing) {
        mv.playhead = toSamples(p.sec);
      } else if (this.status === "playing") {
        // paused by the engine: the end of the mix
        this.status = "paused";
        mv.playhead = toSamples(p.sec);
      }
    }).catch(() => {});
    onMixLiveStatus((s) => {
      if (this.#key === null) return;
      this.load = s;
      if (s.state === "error" || s.message) this.notice = s.message;
      if (s.state === "error" && this.status === "playing") this.status = "paused";
    }).catch(() => {});
  }

  /** Ids of the Wah tools this mix marks as Vocal tools; the engine renders those as the vocal
   * filter and every other Wah tool as Wah. */
  #vocalIds(doc: MixDoc): string[] {
    const marked = new Set(vocalTools.ids(doc.id));
    return doc.clips.flatMap((c) => c.processes.filter((p) => marked.has(p.id)).map((p) => p.id)).sort();
  }

  #gainsKey() {
    return mixExport.gainsKey();
  }

  /** Makes sure the engine holds the open mix as it is now; loads it when it holds another. */
  async #ensure(doc: MixDoc): Promise<boolean> {
    const root = mixes.root;
    if (!root) {
      this.notice = "Open a library first.";
      return false;
    }
    this.#listen();
    const key = `${root}|${doc.id}`;
    const mode = vocal.mode;
    const gains = this.#gainsKey();
    const vocalIds = this.#vocalIds(doc);
    if (this.#key !== key) {
      if (this.#key !== null) this.#drop();
      this.#key = key;
      this.#sent = doc;
      this.#sentMode = mode;
      this.#sentGains = gains;
      this.#sentVocal = vocalIds.join("|");
      try {
        await this.#send(async () => {
          await mixLiveLoad(root, doc, { gains: mixExport.previewGains(), vocalToolIds: vocalIds });
          if (playback.volume < 1) await mixLiveSetMaster(volumeToDb(playback.volume));
        });
      } catch (e) {
        this.#key = null;
        this.#sent = null;
        throw e;
      }
    } else if (!sameRender(this.#sent, this.#sentMode, doc, mode) || this.#sentGains !== gains || this.#sentVocal !== vocalIds.join("|")) {
      await this.#push(doc);
    }
    return true;
  }

  async #push(doc: MixDoc) {
    const refreshVocal = this.#sentMode !== vocal.mode;
    const gainsChanged = this.#sentGains !== this.#gainsKey();
    this.#sent = doc;
    this.#sentMode = vocal.mode;
    this.#sentGains = this.#gainsKey();
    const vocalToolIds = this.#vocalIds(doc);
    this.#sentVocal = vocalToolIds.join("|");
    await this.#send(() => mixLiveUpdate(doc, { refreshVocal, gains: gainsChanged ? mixExport.previewGains() : undefined, vocalToolIds }));
  }

  /** Called when the open document, the card's Vocal mode or the loudness gains may have changed:
   * sends the new state to the engine (while playing the edit is heard within a few hundred ms). */
  edited() {
    if (this.#key === null || this.#range) return;
    const doc = mixes.doc;
    if (!doc) return;
    if (`${mixes.root}|${doc.id}` !== this.#key) {
      // another mix was opened: the old one stops, the next play loads the new one
      this.pause();
      this.#drop();
      return;
    }
    if (sameRender(this.#sent, this.#sentMode, doc, vocal.mode) && this.#sentGains === this.#gainsKey() && this.#sentVocal === this.#vocalIds(doc).join("|")) return;
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => {
      const d = mixes.doc;
      if (!d || this.#key === null) return;
      this.#push(d).catch((e) => (this.notice = errText(e)));
    }, EDIT_DEBOUNCE_MS);
  }

  /** Opens the open mix in the engine as soon as it is shown, so its clips are decoded before the
   * first Play and the first Play is instant. */
  async open(doc: MixDoc) {
    if (!mixes.root || this.#range) return;
    try {
      await this.#ensure(doc);
    } catch (e) {
      this.notice = `Live playback is not available: ${errText(e)}`;
    }
  }

  /** Plays from the playhead (or `fromSamples`). */
  async play(fromSamples: number = mv.playhead) {
    const doc = mixes.doc;
    if (!doc) return;
    if (doc.clips.length === 0) {
      this.notice = "Add a track before playing.";
      return;
    }
    this.notice = "";
    this.#stopRange();
    // the mix and the player bar never play together
    playback.pause();
    let startSec = fromSamples / SR;
    if (startSec >= doc.lengthSec - 0.05) startSec = 0;
    const token = ++this.#token;
    try {
      if (!(await this.#ensure(doc))) return;
      if (token !== this.#token) return;
      mv.playhead = toSamples(startSec);
      await this.#send(() => mixLivePlay(startSec));
      if (token !== this.#token) return;
      this.status = "playing";
    } catch (e) {
      if (token !== this.#token) return;
      this.status = this.status === "playing" ? "paused" : "idle";
      this.notice = errText(e);
    }
  }

  /** Plays a stretch of the mix once, as a partial export makes it: at the level of the current
   * loudness analysis (when there is one) and with the fade at its cuts. It stops at `toSec`. This
   * renders (the live engine has no export fades), so it takes a moment. */
  async playRange(fromSec: number, toSec: number) {
    const doc = mixes.doc;
    if (!doc || doc.clips.length === 0) return;
    this.notice = "";
    const to = Math.min(toSec, fromSec + MAX_RANGE_PREVIEW);
    if (to < toSec) this.notice = "Only the first 15 minutes of the range are previewed.";
    const token = ++this.#token;
    this.#stopRange();
    await this.#pauseLive();
    this.status = "rendering";
    mv.playhead = toSamples(fromSec);
    try {
      const { path } = await mixes.renderPreview(fromSec, to, mixExport.previewGains(), mixExport.prefs.edgeFadeMs);
      if (token !== this.#token) return;
      this.#range = true;
      this.#rangeStart = fromSec;
      this.#rangeEnd = to;
      playback.previewMix(path, { mixId: doc.id, label: doc.name || "Mix preview", fromSecs: fromSec });
      playback.play();
      this.status = "playing";
      this.#tick();
    } catch (e) {
      if (token !== this.#token) return;
      this.status = "idle";
      this.notice = errText(e);
    }
  }

  #tick = () => {
    cancelAnimationFrame(this.#raf);
    if (!this.#range || this.status !== "playing") return;
    if (playback.mixPreview === null) {
      // something else took the player bar
      this.#range = false;
      this.status = "paused";
      return;
    }
    if (!playback.playing) {
      // the rendered part ended
      this.#range = false;
      this.status = "paused";
      mv.playhead = toSamples(this.#rangeEnd);
      return;
    }
    mv.playhead = toSamples(Math.max(this.#rangeStart, playback.position));
    this.#raf = requestAnimationFrame(this.#tick);
  };

  #stopRange() {
    if (!this.#range) return;
    cancelAnimationFrame(this.#raf);
    playback.pause();
    this.#range = false;
  }

  async #pauseLive() {
    if (this.#key === null) return;
    try {
      await this.#send(() => mixLivePause());
    } catch {
      /* nothing is loaded any more */
    }
  }

  pause() {
    this.#token++;
    clearTimeout(this.#timer);
    if (this.#range) this.#stopRange();
    else if (this.#key !== null) void this.#pauseLive();
    if (this.status === "playing" || this.status === "rendering") this.status = "paused";
  }

  async toggle() {
    if (this.status === "playing" || this.status === "rendering") this.pause();
    else await this.play();
  }

  /** Moves the playhead; while playing, the music jumps there at once. */
  async seek(samples: number) {
    const s = Math.max(0, samples);
    mv.playhead = s;
    if (this.#range) {
      await this.play(s);
    } else if (this.status === "playing" && this.#key !== null) {
      try {
        await this.#send(() => mixLiveSeek(s / SR));
      } catch (e) {
        this.notice = errText(e);
      }
    }
  }

  /** Pauses and lets the engine free the decoded audio and the audio device (the view was closed).
   * The next play loads again. */
  release() {
    this.pause();
    this.#drop();
  }

  #drop() {
    clearTimeout(this.#timer);
    if (this.#key === null) return;
    this.#key = null;
    this.#sent = null;
    this.load = { state: "idle", loaded: 0, total: 0, message: "" };
    void this.#send(() => mixLiveUnload()).catch(() => {});
  }

  /** Stops and forgets everything (a different mix was opened, or the session was reset). */
  stop() {
    this.#token++;
    this.#stopRange();
    this.#drop();
    this.status = "idle";
    this.notice = "";
  }
}

export const preview = new Preview();

session.onReset(() => preview.stop());

// Edits, the card's Vocal flag and the loudness gains reach the engine through here; the player bar
// taking over (a library track starts) silences the mix.
$effect.root(() => {
  $effect(() => {
    void mixes.doc;
    void vocal.mode;
    void mixExport.gainsKey();
    if (mixes.doc) void vocalTools.ids(mixes.doc.id);
    preview.edited();
  });
  $effect(() => {
    if (playback.playing && playback.mixPreview === null && preview.status === "playing") preview.pause();
  });
});
