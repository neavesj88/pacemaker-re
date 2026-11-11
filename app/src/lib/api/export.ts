// export commands (src-tauri/src/commands/export.rs): mix audio rendering via crates/pm-mixer.
// Engine, algorithms and parameter mappings: notes/mix-audio.md.
import { invoke } from "@tauri-apps/api/core";
import type { MixDoc } from "./mixes";

/** A finished render. `path` is the contract; the rest is extra information. */
export type RenderedMix = {
  path: string;
  /** Length of the rendered audio in seconds. */
  durationSecs: number;
  /** Clips that could not be rendered (removed track, missing file) and were left out. */
  warnings: string[];
};

/** Render part of a mix to a temporary WAV for preview playback; returns its path (play it with
 * playback.previewMix). `mix` is the editor's document (api/mixes.ts), saved or not; `fromSec` and
 * `toSec` are MIX TIMELINE seconds (the file starts at `fromSec`). At most 15 minutes per call.
 * The file is one of four rotating temp files, so a preview stays valid for the next three
 * renders. Cross-area contract: the mix audio area implements `export_render_preview`; the mix
 * editor calls it. */
export const renderMixPreview = (root: string, mix: MixDoc, fromSec: number, toSec: number) =>
  invoke<RenderedMix>("export_render_preview", { root, mix, fromSec, toSec });

/** What `exportMixAudio` returns the moment the job has started. */
export type MixExportStarted = { path: string };

/** The `job` name of mix exports in the `job-progress` / `job-done` events. */
export const MIX_EXPORT_JOB = "mix-export";

/** Render a whole mix to an audio file (format "ogg" like the original Editor, or "wav").
 * This starts a background job and resolves AT ONCE with the destination; progress arrives as
 * `job-progress` events (job "mix-export", `done` / `total` in seconds of audio) and the end as
 * `job-done` (its `summary` is the finished line, `failed` counts clips left out). The file
 * exists under `path` only after `job-done`. Rejects when another job is running, the format is
 * unknown, or `dest` is inside the device volume or in a folder that does not exist. Rendering
 * starts at the mix's first audible frame. */
export const exportMixAudio = (root: string, mix: MixDoc, dest: string, format: "ogg" | "wav") =>
  invoke<MixExportStarted>("export_mix_audio", { root, mix, dest, format });
