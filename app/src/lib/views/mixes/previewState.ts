// When a rendered preview still sounds like the open mix. Plain TypeScript (no runes) so the rule
// can be tested with a node script; preview.svelte.ts is the only caller.
import type { MixDoc } from "$lib/api/mixes";
import type { VocalMode } from "$lib/api/device";

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

/** True when audio rendered from `rendered` in Wah-tool mode `renderedMode` is what `doc` would
 * render now in mode `mode`. The mode is not part of the document: it comes from the card's Vocal
 * flag, which changes what every Wah tool sounds like. */
export function sameRender(rendered: MixDoc | null, renderedMode: VocalMode, doc: MixDoc | null, mode: VocalMode): boolean {
  return renderedMode === mode && sameAudio(rendered, doc);
}
