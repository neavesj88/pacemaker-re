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

/** The player bar volume (0..1) as the live mix's listening level in dB: 1 is 0 dB (as exported),
 * the bottom of the slider is silence (-60 dB). */
export function volumeToDb(v: number): number {
  return v <= 0.001 ? -60 : Math.max(-60, 20 * Math.log10(Math.min(1, v)));
}

/** Ids of the Wah tools of `doc` that `marked` (the app's Vocal marks of this mix) names, sorted: what the
 * engine, the range preview and the export are told, so all three render the same tools as the vocal filter. */
export function markedToolIds(doc: MixDoc, marked: readonly string[]): string[] {
  const set = new Set(marked);
  return doc.clips.flatMap((c) => c.processes.filter((p) => set.has(p.id)).map((p) => p.id)).sort();
}

/** Whether the engine would find anything to play: a clip that is not muted and has an audible length
 * (the rule of `audible_clips` in pm-mixer). Without one Play does nothing, so the page says so up front. */
export function hasAudibleClips(doc: MixDoc): boolean {
  return doc.clips.some((c) => !c.clipBypassed && c.visibleEnd > c.visibleStart && c.visibleEnd > 0);
}

/** What Play says when the mix has no clip at all. */
export const ADD_TRACK_NOTICE = "Add a track before playing.";

/** What Play says when every clip is muted or has no length (the engine would only say it could not build). */
export const NO_AUDIBLE_NOTICE = "No clip in this mix can be heard: every clip is muted or has no length.";
