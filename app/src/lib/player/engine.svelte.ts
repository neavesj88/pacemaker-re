// The playback engine: turns the transport state in `state/playback.svelte.ts` into sound.
// Audio goes through an <audio> element over the asset protocol; in the browser preview (no
// audio files) a silent clock stands in so the whole bar can still be reviewed. The engine owns
// everything derived from the loaded file (waveform peaks, beat ticks); views never touch it.
import { convertFileSrc } from "@tauri-apps/api/core";
import { untrack } from "svelte";
import { peaks as fetchPeaks, type Peak } from "$lib/api/analysis";
import type { Unlisten } from "$lib/api/events";
import { playbackAllowFile, playbackBeats, playbackSource } from "$lib/api/playback";
import { playback } from "$lib/state/playback.svelte";
import { session } from "$lib/state/session.svelte";

/** The few things the engine needs from an audio output. */
interface Media {
  time: number;
  readonly duration: number;
  volume: number;
  onended: (() => void) | null;
  onduration: (() => void) | null;
  onfail: ((message: string) => void) | null;
  /** Points at a new file (`null` unloads). `knownDuration` is only used by the silent clock. */
  load(url: string | null, knownDuration: number): void;
  play(): Promise<void>;
  pause(): void;
}

class AudioMedia implements Media {
  onended: (() => void) | null = null;
  onduration: (() => void) | null = null;
  onfail: ((message: string) => void) | null = null;
  #a = new Audio();

  constructor() {
    this.#a.preload = "auto";
    this.#a.addEventListener("ended", () => this.onended?.());
    this.#a.addEventListener("loadedmetadata", () => this.onduration?.());
    this.#a.addEventListener("durationchange", () => this.onduration?.());
    this.#a.addEventListener("error", () => {
      if (!this.#a.getAttribute("src")) return; // unloading is not an error
      const code = this.#a.error?.code;
      this.onfail?.(
        code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED
          ? "This audio file cannot be played (unsupported or missing)."
          : `Audio error${this.#a.error?.message ? `: ${this.#a.error.message}` : ""}`,
      );
    });
  }

  get time() {
    return this.#a.currentTime;
  }
  set time(v: number) {
    this.#a.currentTime = v;
  }
  get duration() {
    return Number.isFinite(this.#a.duration) ? this.#a.duration : 0;
  }
  get volume() {
    return this.#a.volume;
  }
  set volume(v: number) {
    this.#a.volume = v;
  }

  load(url: string | null) {
    this.#a.pause();
    if (url === null) {
      this.#a.removeAttribute("src");
      this.#a.load();
    } else {
      this.#a.src = url;
    }
  }
  play() {
    return this.#a.play();
  }
  pause() {
    this.#a.pause();
  }
}

/** Plays nothing but keeps time, for the browser preview. */
class ClockMedia implements Media {
  onended: (() => void) | null = null;
  onduration: (() => void) | null = null;
  onfail: ((message: string) => void) | null = null;
  volume = 1;
  #length = 0;
  #base = 0;
  #since: number | null = null;

  get time() {
    if (this.#since === null) return this.#base;
    const t = this.#base + (performance.now() - this.#since) / 1000;
    if (t >= this.#length) {
      // reached the end: stop and report once
      this.#base = this.#length;
      this.#since = null;
      queueMicrotask(() => this.onended?.());
      return this.#length;
    }
    return t;
  }
  set time(v: number) {
    this.#base = Math.min(Math.max(0, v), this.#length);
    if (this.#since !== null) this.#since = performance.now();
  }
  get duration() {
    return this.#length;
  }

  load(url: string | null, knownDuration: number) {
    this.#since = null;
    this.#base = 0;
    this.#length = url === null ? 0 : knownDuration;
    if (url !== null) queueMicrotask(() => this.onduration?.());
  }
  play() {
    if (this.#since === null) this.#since = performance.now();
    return Promise.resolve();
  }
  pause() {
    this.#base = this.time;
    this.#since = null;
  }
}

const PEAK_COLUMNS = 1600;
/** Within this many seconds of the cue point counts as "at the cue" for the CUE button. */
const AT_CUE_SECS = 0.06;
const LOOP_BEATS = 4;

class Engine {
  /** Colour waveform of the loaded track, or null while loading / for a mix preview. */
  peaks = $state<Peak[] | null>(null);
  /** Beat positions of the loaded track in seconds. */
  beats = $state<number[]>([]);
  firstDownbeat = $state<number | null>(null);
  /** Why there are no beat ticks ("no .str file", ...), or "". */
  beatsProblem = $state("");
  /** Why there is no waveform ("analysis failed: ..."), or "". */
  peaksProblem = $state("");
  /** True when nothing is audible: the browser preview, where a silent clock keeps time. */
  silent = $state(false);
  /** The source is resolved and the media can play. */
  ready = $state(false);
  loading = $state(false);

  #audio: AudioMedia | null = null;
  #clock = new ClockMedia();
  #media: Media = this.#clock;
  #token = 0;
  #raf = 0;
  #tick: ReturnType<typeof setInterval> | undefined;
  #cuePreviewing = false;
  #started = false;

  /** Starts the effects that keep the audio in step with the transport. Returns the teardown. */
  start(): Unlisten {
    if (this.#started) return () => {};
    this.#started = true;
    try {
      this.#audio = new AudioMedia();
    } catch {
      this.#audio = null;
    }
    for (const m of [this.#audio, this.#clock]) {
      if (!m) continue;
      m.onended = () => this.#ended(m);
      m.onduration = () => {
        if (m === this.#media) playback.duration = m.duration;
      };
      m.onfail = (msg) => {
        if (m !== this.#media) return;
        this.ready = false;
        playback.error = msg;
        playback.playing = false;
      };
    }

    const stop = $effect.root(() => {
      // 1. what is loaded: resolve the file, beats and peaks
      $effect(() => {
        const id = playback.trackId;
        const mix = playback.mixPreview;
        const root = session.library?.root ?? null;
        untrack(() => void this.#select(id, mix, root));
      });
      // 2. play / pause, once the source is ready
      $effect(() => {
        const want = playback.playing;
        const ready = this.ready;
        untrack(() => {
          if (!ready) return;
          if (want) this.#play();
          else {
            this.#media.pause();
            this.#halt();
            this.#sync();
          }
        });
      });
      // 3. volume
      $effect(() => {
        const v = playback.volume;
        if (this.#audio) this.#audio.volume = v;
      });
      // 4. seeks asked for by the transport
      $effect(() => {
        const req = playback.seekRequest;
        const ready = this.ready;
        if (!req || !ready) return;
        untrack(() => {
          const off = playback.mixPreview?.fromSecs ?? 0;
          const len = this.#media.duration;
          const t = req.secs - off;
          const loop = playback.loop;
          // a seek outside the loop leaves it
          if (loop && (t < loop.start || t >= loop.end)) playback.loop = null;
          this.#media.time = len > 0 ? Math.min(t, Math.max(0, len - 0.01)) : Math.max(0, t);
          playback.seekRequest = null;
          this.#sync();
        });
      });
    });

    return () => {
      stop();
      this.#halt();
      this.#media.pause();
      this.#started = false;
    };
  }

  async #select(id: number | null, mix: typeof playback.mixPreview, root: string | null) {
    const token = ++this.#token;
    this.#halt();
    this.#media.pause();
    this.#media.load(null, 0);
    this.ready = false;
    this.peaks = null;
    this.beats = [];
    this.firstDownbeat = null;
    this.beatsProblem = "";
    this.peaksProblem = "";
    this.#cuePreviewing = false;
    if (!root || (id === null && !mix)) {
      this.loading = false;
      return;
    }
    this.loading = true;
    try {
      if (mix) {
        const { path } = await playbackAllowFile(root, mix.path);
        if (token !== this.#token) return;
        this.#open(path, 30); // only the silent clock uses the length: a stand-in for the browser preview
        return;
      }
      const track = session.library?.tracks.find((t) => t.track_id === id);
      if (!track) throw new Error("This track is not in the open library.");
      // peaks and beats only decorate the bar; they must not hold up (or fail) playback
      void fetchPeaks(root, track.location, PEAK_COLUMNS)
        .then((p) => token === this.#token && (this.peaks = p))
        .catch((e) => {
          if (token === this.#token) this.peaksProblem = e instanceof Error ? e.message : String(e);
        });
      const beats = playbackBeats(root, track.location).then(
        (b) => {
          if (token !== this.#token) return;
          this.beats = b.beats;
          this.firstDownbeat = b.first_downbeat;
          this.beatsProblem = b.problem ?? "";
          if (playback.cue === 0 && b.beats.length) playback.cue = b.beats[b.first_downbeat ?? 0] ?? 0;
        },
        (e) => {
          if (token === this.#token) this.beatsProblem = String(e);
        },
      );
      const { path } = await playbackSource(root, track.location);
      if (token !== this.#token) return;
      this.#open(path, track.play_time_secs);
      await beats;
    } catch (e) {
      if (token !== this.#token) return;
      playback.error = e instanceof Error ? e.message : String(e);
      playback.playing = false;
    } finally {
      if (token === this.#token) this.loading = false;
    }
  }

  /** Points the output at `path`; an empty path means the browser preview (silent clock). */
  #open(path: string, knownDuration: number) {
    const silent = path === "" || !this.#audio;
    this.silent = silent;
    this.#media = silent ? this.#clock : this.#audio!;
    this.#media.volume = playback.volume;
    this.#media.load(silent ? "silent" : convertFileSrc(path), knownDuration);
    playback.duration = silent ? knownDuration : 0;
    this.ready = true;
  }

  #play() {
    const token = this.#token;
    this.#media.play().then(
      () => this.#frame(),
      (e) => {
        // a pause or a track change interrupts a pending play(): that is not a failure, and it
        // must not cancel the autoplay of whatever was loaded since
        if (token !== this.#token || (e instanceof Error && e.name === "AbortError")) return;
        // autoplay refusals and decode failures land here
        playback.playing = false;
        if (!playback.error) playback.error = `Cannot play: ${e instanceof Error ? e.message : String(e)}`;
      },
    );
  }

  /** Copies the output's clock into the transport, applying the beat loop. */
  #sync() {
    const off = playback.mixPreview?.fromSecs ?? 0;
    const loop = playback.mixPreview ? null : playback.loop;
    if (loop && this.#media.time >= loop.end) this.#media.time = loop.start;
    playback.position = off + this.#media.time;
    if (!playback.duration && this.#media.duration) playback.duration = this.#media.duration;
  }

  #halt() {
    cancelAnimationFrame(this.#raf);
    clearInterval(this.#tick);
  }

  #frame = () => {
    this.#halt();
    this.#sync();
    if (playback.playing && this.ready) {
      this.#raf = requestAnimationFrame(this.#frame);
      // animation frames stop in a hidden or minimised window; the beat loop must keep wrapping
      this.#tick = setInterval(() => this.#sync(), 100);
    }
  };

  #ended(m: Media) {
    if (m !== this.#media) return;
    if (playback.mixPreview) {
      playback.pause();
      playback.seek(playback.mixPreview.fromSecs);
      return;
    }
    // keep going through the list; at its end stop on the first frame
    if (!playback.next()) {
      playback.pause();
      playback.seek(0);
    }
  }

  /** Device-style CUE, pressed. Stopped at the cue point: preview from it while held. Anywhere
   * else (playing or not): stop and jump to the cue point. */
  cuePress() {
    if (!this.ready || playback.mixPreview) return;
    const atCue = !playback.playing && Math.abs(playback.position - playback.cue) <= AT_CUE_SECS;
    if (atCue) {
      this.#cuePreviewing = true;
      playback.play();
    } else {
      playback.pause();
      playback.seek(playback.cue);
    }
  }

  /** CUE released: a held preview stops and returns to the cue point. */
  cueRelease() {
    if (!this.#cuePreviewing) return;
    this.#cuePreviewing = false;
    playback.pause();
    playback.seek(playback.cue);
  }

  /** Moves the cue point to the playhead. */
  setCueHere() {
    if (this.ready && !playback.mixPreview) playback.setCue(playback.position);
  }

  /** 4-beat loop on / off. On, it starts at the last beat at or before the playhead. */
  toggleLoop() {
    if (playback.loop) {
      playback.loop = null;
      return;
    }
    if (!this.ready || playback.mixPreview) return;
    const pos = playback.position;
    const b = this.beats;
    let start = pos;
    let end: number;
    if (b.length > 1) {
      let i = 0;
      while (i + 1 < b.length && b[i + 1] <= pos + 0.02) i++;
      start = b[i];
      const last = b.length - 1;
      const step = b[Math.min(i + 1, last)] - b[Math.min(i, last - 1)];
      end = i + LOOP_BEATS <= last ? b[i + LOOP_BEATS] : start + LOOP_BEATS * (step > 0 ? step : 0.5);
    } else {
      const bpm = session.library?.tracks.find((t) => t.track_id === playback.trackId)?.bpm || 120;
      end = start + (LOOP_BEATS * 60) / bpm;
    }
    const len = this.#media.duration;
    if (len > 0 && end > len) end = len;
    if (end - start < 0.05) return;
    playback.loop = { start, end };
  }
}

export const engine = new Engine();
