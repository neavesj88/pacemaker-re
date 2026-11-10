// export commands (src-tauri/src/commands/export.rs): mix audio rendering via crates/pm-mixer.
import { invoke } from "@tauri-apps/api/core";

/** Render part of a mix to a temporary WAV for preview playback; returns its path (play it with
 * playback.previewMix). `mix` is the editor's document (api/mixes.ts). Cross-area contract: the
 * mix audio area implements `export_render_preview`; the mix editor calls it. */
export const renderMixPreview = (root: string, mix: unknown, fromSec: number, toSec: number) =>
  invoke<{ path: string }>("export_render_preview", { root, mix, fromSec, toSec });

/** Render a whole mix to an audio file (format "ogg" like the original Editor, or "wav"). */
export const exportMixAudio = (root: string, mix: unknown, dest: string, format: "ogg" | "wav") =>
  invoke<{ path: string }>("export_mix_audio", { root, mix, dest, format });
