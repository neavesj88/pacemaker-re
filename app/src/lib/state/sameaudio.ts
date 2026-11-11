// Whether two documents of the mix editor sound the same. Shared by the editor's preview (is the
// rendered audio out of date?) and the export settings (is the loudness analysis still about this
// mix?). Pure: no Svelte, no Tauri.
import type { MixDoc } from "$lib/api/mixes";

/** True when two documents sound the same: everything but the name, the library row and the
 * reference tempo, which never change the audio. Untouched parts are shared between documents,
 * so a reference comparison per field is enough. */
export function sameAudio(a: MixDoc | null, b: MixDoc | null): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  for (const k of new Set([...Object.keys(x), ...Object.keys(y)])) {
    if (k === "name" || k === "meta" || k === "tempoBpm") continue;
    if (x[k] !== y[k]) return false;
  }
  return true;
}
