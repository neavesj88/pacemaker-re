// Mix playback. The Mix tab plays the open, possibly unsaved document LIVE through the audio
// area's mix engine (api/mixLive.ts): the clips are decoded in the background when the mix is
// opened, an edit while playing is sent to the engine as it happens, and there is no render step.
// The playhead follows the engine's position events.
//
// One thing is still rendered: "Preview this part" in the export dialog plays a stretch exactly
// as a partial export makes it (loudness gain, edge fades), which needs the export render
// (`playRange` below). Starting either one stops the other.
import { onMixLivePosition, onMixLiveStatus, mixLiveLoad, mixLivePause, mixLivePlay, mixLiveSeek, mixLiveSetMaster, mixLiveUnload, mixLiveUpdate, type MixLiveStatus } from "$lib/api/mixLive";
import { mixExport } from "$lib/state/mixexport.svelte";
import { mixes } from "$lib/state/mixes.svelte";
import { playback } from "$lib/state/playback.svelte";
import { session } from "$lib/state/session.svelte";
import type { MixDoc } from "$lib/api/mixes";
import { SR, toSamples } from "./layout";
import { vocalTools } from "./vocalTools.svelte";
import { mv } from "./view.svelte";

/** The longest stretch one range preview renders (the backend's limit), seconds. */
const MAX_RANGE_PREVIEW = 900;

export type PreviewStatus = "idle" | "rendering" | "playing" | "paused";

const IDLE: MixLiveStatus = { state: "idle", loaded: 0, total: 0, message: "" };

/** Preview level 0..1 as dB for the engine (0 is silence: -90). */
const toDb = (v: number) => (v <= 0.0001 ? -90 : 20 * Math.log10(Math.min(1, v)));

class Preview {
  status = $state<PreviewStatus>("idle");
  /** Why nothing plays, or "". */
  notice = $state("");
  /** The engine's own state: decoding progress and errors. */
  engine = $state<MixLiveStatus>({ ...IDLE });

  #unsubs: (() => void)[] = [];
  #raf = 0;
  #queued: MixDoc | null = null;
  /** A range preview (rendered, played by the shared player) is running. */
  #range = false;
  #rangeToken = 0;
  #rangeRaf = 0;
  #rangeFrom = 0;
  #volume = 1;

  /** Preview level 0..1 (a listening aid, not part of the mix). */
  get volume() {
    return this.#volume;
  }
  set volume(v: number) {
    this.#volume = Math.min(1, Math.max(0, v));
    void mixLiveSetMaster(toDb(this.#volume)).catch(() => {});
  }

  get playing() {
    return this.status === "playing";
  }

  /** The live engine plays what is on screen, so there is no out-of-date state. */
  get stale() {
    return false;
  }

  /** True while the engine is still decoding clips. */
  get decoding() {
    return this.engine.state === "loading" && this.engine.loaded < this.engine.total;
  }

  /** Opens the live engine on the open mix: listens to its events and starts the decoding. */
  async open(doc: MixDoc) {
    const root = session.library?.root;
    if (!root) return;
    this.#detach();
    this.status = "idle";
    this.notice = "";
    this.engine = { state: "loading", loaded: 0, total: doc.clips.length, message: "" };
    try {
      this.#unsubs.push(
        await onMixLivePosition((p) => {
          if (this.#range) return;
          if (p.playing || this.status !== "playing") mv.playhead = toSamples(p.sec);
          if (p.playing) this.status = "playing";
          else if (this.status === "playing") this.status = "paused";
        }),
        await onMixLiveStatus((st) => {
          this.engine = st;
          if (st.state === "error") {
            this.notice = st.message || "The mix engine reported an error.";
            if (this.status === "playing") this.status = "paused";
          } else if (this.notice && st.state !== "idle") this.notice = "";
        }),
      );
      await mixLiveLoad(root, doc, vocalToolIds(doc));
      await mixLiveSetMaster(toDb(this.#volume)).catch(() => {});
    } catch (e) {
      this.engine = { state: "error", loaded: 0, total: doc.clips.length, message: errorText(e) };
      this.notice = `Live playback is not available: ${errorText(e)}`;
    }
  }

  /** Sends an edit to the engine (at most once per frame, so a drag costs one update per frame). */
  sync(doc: MixDoc) {
    this.#queued = doc;
    if (this.#raf) return;
    this.#raf = requestAnimationFrame(() => {
      this.#raf = 0;
      const d = this.#queued;
      this.#queued = null;
      if (d) void mixLiveUpdate(d, vocalToolIds(d)).catch(() => {});
    });
  }

  /** Plays from the playhead (or `fromSamples`). */
  async play(fromSamples: number = mv.playhead) {
    const doc = mixes.doc;
    if (!doc) return;
    if (doc.clips.length === 0) {
      this.notice = "Add a track before playing.";
      return;
    }
    this.#stopRange();
    this.notice = "";
    let startSec = fromSamples / SR;
    if (startSec >= doc.lengthSec - 0.05) startSec = 0;
    mv.playhead = toSamples(startSec);
    try {
      await mixLivePlay(startSec);
      this.status = "playing";
    } catch (e) {
      this.status = "idle";
      this.notice = errorText(e);
    }
  }

  pause() {
    if (this.#range) {
      this.#stopRange();
      return;
    }
    if (this.status === "playing") this.status = "paused";
    void mixLivePause().catch(() => {});
  }

  async toggle() {
    if (this.status === "playing" || this.status === "rendering") this.pause();
    else await this.play();
  }

  /** Moves the playhead; the engine jumps there too (playing or not). */
  async seek(samples: number) {
    const s = Math.max(0, samples);
    mv.playhead = s;
    if (this.#range) this.#stopRange();
    await mixLiveSeek(s / SR).catch(() => {});
  }

  /** Plays a stretch of the mix once, as a partial export makes it: at the level of the current
   * loudness analysis (when there is one) and with the fade at its cuts. It stops at `toSec`. */
  async playRange(fromSec: number, toSec: number) {
    const doc = mixes.doc;
    if (!doc || doc.clips.length === 0) return;
    this.notice = "";
    void mixLivePause().catch(() => {});
    const to = Math.min(toSec, fromSec + MAX_RANGE_PREVIEW);
    if (to < toSec) this.notice = "Only the first 15 minutes of the range are previewed.";
    const token = ++this.#rangeToken;
    this.#range = true;
    this.status = "rendering";
    mv.playhead = toSamples(fromSec);
    this.#rangeFrom = fromSec;
    try {
      const { path } = await mixes.renderPreview(fromSec, to, mixExport.previewGains(), mixExport.prefs.edgeFadeMs);
      if (token !== this.#rangeToken) return;
      playback.previewMix(path, { mixId: doc.id, label: doc.name || "Mix preview", fromSecs: fromSec });
      playback.play();
      this.status = "playing";
      this.#rangeTick(token);
    } catch (e) {
      if (token !== this.#rangeToken) return;
      this.#range = false;
      this.status = "idle";
      this.notice = errorText(e);
    }
  }

  #rangeTick(token: number) {
    cancelAnimationFrame(this.#rangeRaf);
    if (token !== this.#rangeToken || !this.#range) return;
    if (!playback.playing) {
      this.#range = false;
      this.status = "paused";
      return;
    }
    mv.playhead = toSamples(playback.position);
    this.#rangeRaf = requestAnimationFrame(() => this.#rangeTick(token));
  }

  #stopRange() {
    if (!this.#range) return;
    this.#rangeToken++;
    cancelAnimationFrame(this.#rangeRaf);
    playback.pause();
    this.#range = false;
    this.status = "paused";
  }

  #detach() {
    for (const u of this.#unsubs) u();
    this.#unsubs = [];
    cancelAnimationFrame(this.#raf);
    this.#raf = 0;
    this.#queued = null;
  }

  /** Stops and drops the decoded audio (a different mix was opened, or the editor closed). */
  stop() {
    this.#stopRange();
    this.#detach();
    void mixLivePause().catch(() => {});
    void mixLiveUnload().catch(() => {});
    this.status = "idle";
    this.notice = "";
    this.engine = { ...IDLE };
  }
}

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

/** Ids of the Wah tools this mix marks as Vocal tools, for the engine. */
function vocalToolIds(doc: MixDoc): string[] {
  const marked = new Set(vocalTools.ids(doc.id));
  return doc.clips.flatMap((c) => c.processes.filter((p) => marked.has(p.id)).map((p) => p.id));
}

export const preview = new Preview();

session.onReset(() => preview.stop());
