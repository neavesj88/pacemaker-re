// When a rendered preview still sounds like the open mix. Plain TypeScript (no runes) so the rule
// can be tested with a node script; preview.svelte.ts is the only caller.
import type { MixDoc } from "$lib/api/mixes";
import type { VocalMode } from "$lib/api/device";
import { sameAudio } from "../../state/sameaudio.ts";

// the rule itself lives in state/sameaudio.ts (the export settings use it too); re-exported for the tests
export { sameAudio };

/** True when audio rendered from `rendered` in Wah-tool mode `renderedMode` is what `doc` would
 * render now in mode `mode`. The mode is not part of the document: it comes from the card's Vocal
 * flag, which changes what every Wah tool sounds like. */
export function sameRender(rendered: MixDoc | null, renderedMode: VocalMode, doc: MixDoc | null, mode: VocalMode): boolean {
  return renderedMode === mode && sameAudio(rendered, doc);
}
