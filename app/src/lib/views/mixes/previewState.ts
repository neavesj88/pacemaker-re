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

/** The one card that lists the tracks kept at their own tempo. Adding tracks one by one used to
 * show a full-width card per track, which stacked over the clip panel and the Add bar. */
export function keptNote(kept: readonly string[]): string {
  return `Kept at their own tempo, since matching would stretch them too much: ${kept.join("; ")}. Use "Sync to mix tempo" in the clip's panel to match anyway.`;
}

/** Adds `more` to the tracks already listed, leaving out one that is listed (the same line twice adds nothing). */
export function addKept(listed: readonly string[], more: readonly string[]): string[] {
  const out = [...listed];
  for (const k of more) if (!out.includes(k)) out.push(k);
  return out;
}

// ---------------------------------------------------------------------------------------------
// The words of the mix editor's toasts: one short sentence each (see state/toastText.ts for the rule).
// What does not fit (names, times, every warning) is returned as `details`, behind the card's Details button.
// ---------------------------------------------------------------------------------------------

const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;

/** What adding tracks says, in one card: how many went in, and a short tail for those placed later or kept at their own tempo. */
export function addTracksToast(r: { added: number; moved: readonly string[]; kept: readonly string[] }): { text: string; details?: string } {
  const lead = r.added === 1 ? "Track added" : `${r.added} tracks added`;
  const tail: string[] = [];
  if (r.moved.length) tail.push(r.added === 1 ? "placed later, decks busy" : `${r.moved.length} placed later, decks busy`);
  if (r.kept.length) tail.push(`${r.kept.length === 1 ? "1 track" : `${r.kept.length} tracks`} kept at own tempo`);
  const lines: string[] = [];
  if (r.moved.length) lines.push(`Both decks were busy where you asked, so it went later: ${r.moved.join("; ")}.`);
  if (r.kept.length) lines.push(keptNote(r.kept));
  return { text: tail.length ? `${lead}; ${tail.join("; ")}.` : lead, details: lines.length ? lines.join("\n") : undefined };
}

/** What Beat-match says: the tempo it matched to; the backend's warnings (a big stretch, no downbeat, a half-time match) are the details. */
export function beatmatchToast(targetBpm: number, notes: readonly string[]): { text: string; details?: string } {
  const head = `Matched to ${targetBpm.toFixed(1)} BPM`;
  if (notes.length === 0) return { text: head + "." };
  return { text: `${head}, with ${plural(notes.length, "warning")}.`, details: notes.map((n) => n.charAt(0).toUpperCase() + n.slice(1)).join(".\n") + "." };
}

/** What Match all says: how many tracks now play at the mix tempo, and how many were left as they were. */
export function matchAllToast(r: { applied: number; already: number; kept: readonly string[]; ramps: readonly string[] }, bpm: number): { text: string; kind: "ok" | "info"; details?: string } {
  const left = r.kept.length + r.ramps.length;
  const tempo = Math.round(bpm * 10) / 10;
  const lines: string[] = [];
  if (r.ramps.length) lines.push(`Left alone, they have a tempo ramp (use Sync on the clip to rescale it): ${r.ramps.join("; ")}.`);
  if (r.kept.length) lines.push(`Left at their own tempo, matching would stretch them too much: ${r.kept.join("; ")}.`);
  const details = lines.length ? lines.join("\n") : undefined;
  if (r.applied === 0 && left === 0) return { text: `Every track already plays at ${tempo} BPM.`, kind: "ok" };
  if (r.applied === 0) return { text: `${plural(left, "track")} kept: stretch or tempo ramp.`, kind: "info", details };
  const lead = `Matched ${plural(r.applied, "track")} to ${tempo} BPM`;
  return left > 0 ? { text: `${lead}; ${left} kept.`, kind: "info", details } : { text: lead + ".", kind: "ok" };
}
