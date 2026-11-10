// The playback transport: what is loaded, whether it plays, where it is. Owned by the playback
// area (the engine in lib/player reads and drives these fields); every other area, the mix
// editor and the grid editor included, only calls the methods and reads the fields. This is
// the one place the "now playing" state lives, so views never talk to the engine directly.

export type MixPreview = { mixId: number; fromSecs: number };

class Playback {
  /** Track loaded in the player, or null. */
  trackId = $state<number | null>(null);
  playing = $state(false);
  /** Seconds from the start of the loaded track (or of the mix when previewing one). */
  position = $state(0);
  /** Seconds, 0 until the engine knows it. */
  duration = $state(0);
  /** Set while a mix is being previewed through the transitions instead of a single track. */
  mixPreview = $state<MixPreview | null>(null);

  load(trackId: number, autoplay = false) {
    this.mixPreview = null;
    this.trackId = trackId;
    this.position = 0;
    this.duration = 0;
    this.playing = autoplay;
  }

  play() {
    this.playing = this.trackId !== null || this.mixPreview !== null;
  }

  pause() {
    this.playing = false;
  }

  toggle() {
    if (this.playing) this.pause();
    else this.play();
  }

  /** Move to `secs`; the engine clamps it. */
  seek(secs: number) {
    this.position = Math.max(0, secs);
  }

  /** Preview a mix from `fromSecs` (mix timeline). */
  previewMix(mixId: number, fromSecs = 0) {
    this.trackId = null;
    this.mixPreview = { mixId, fromSecs };
    this.position = fromSecs;
    this.playing = true;
  }

  stop() {
    this.playing = false;
    this.trackId = null;
    this.mixPreview = null;
    this.position = 0;
    this.duration = 0;
  }
}

export const playback = new Playback();
