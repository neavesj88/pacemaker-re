// playback commands (src-tauri/src/commands/playback.rs).
import { invoke } from "@tauri-apps/api/core";

export type Source = { path: string };

export type Beats = {
  /** Beat positions in seconds from the start of the file. */
  beats: number[];
  /** Index into `beats` of the first downbeat. */
  first_downbeat: number | null;
  /** Gain the device applies, 1 when there is no .str. */
  normalize_factor: number;
  /** Why there are no beats ("no .str file", "empty .str file", ...), or null. */
  problem: string | null;
};

/** The audio file of a library track, allowed for the <audio> element (turn the path into a
 * URL with convertFileSrc). `location` is `tracks.location`. */
export const playbackSource = (root: string, location: string) => invoke<Source>("playback_source", { root, location });

/** Allows a rendered mix preview (a file under the library or the temp folder) for playback. */
export const playbackAllowFile = (root: string, path: string) => invoke<Source>("playback_allow_file", { root, path });

/** Beat ticks of a track from its .str, in seconds. */
export const playbackBeats = (root: string, location: string) => invoke<Beats>("playback_beats", { root, location });
