// What the Tracks view does with tracks, shared by the table's context menu, the keyboard and
// the inspector. Everything outside this area goes through its cross-area contract.
import { setRating } from "$lib/api/metadata";
import { revealTrack } from "$lib/api/tracks";
import type { Track } from "$lib/api/types";
import { metadataEditor } from "$lib/state/metadata.svelte";
import { nav } from "$lib/state/nav.svelte";
import { cases } from "$lib/state/cases.svelte";
import { jobs } from "$lib/state/jobs.svelte";
import { playback } from "$lib/state/playback.svelte";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";

// An empty queue: next / previous walk the Tracks table as it is shown when they are pressed,
// not a list left over from playing somewhere else.
export const playTrack = (trackId: number) => playback.load(trackId, true, []);

export const editMetadata = (ids: number[]) => metadataEditor.edit([...ids]);

// The architecture note names deep-link keys "<area>.<name>", the cross-area brief names them
// plainly; both are passed so whichever the receiving view reads is found.
export const addToMix = (ids: number[]) => nav.go("mixes", { "mixes.addTracks": [...ids], addTracks: [...ids] });

export const editGrid = (trackId: number) => nav.go("grid", { "grid.trackId": trackId, trackId });

/** Opens the Set builder with these tracks (it orders them by key and tempo; nothing is written). */
export const buildSet = (ids: number[]) => nav.go("setbuilder", { "setbuilder.trackIds": [...ids] });

export async function revealInExplorer(trackId: number) {
  const root = session.library?.root;
  if (!root) return;
  try {
    await revealTrack(root, trackId);
  } catch (e) {
    toast.show(`Could not show the file: ${e}`, "bad");
  }
}

/** Sets the star rating at once (every table showing the track updates) and puts the old one back
 *  if the device refuses it. */
export async function rate(track: Track, stars: number) {
  const root = session.library?.root;
  if (!root || stars === track.rating) return;
  if (!jobs.canWrite) {
    toast.show("Ratings can't be saved while a job runs or the library is read-only.", "bad");
    return;
  }
  const before = track.rating;
  const id = track.track_id;
  session.patchTracks([id], { rating: stars });
  try {
    await setRating(root, track.track_id, stars);
    // the smart case counts were first asked for on the optimistic change, which may be before this write landed
    cases.scheduleSmartCounts();
  } catch (e) {
    // only if nothing else changed the rating meanwhile
    if (session.library?.tracks.find((t) => t.track_id === id)?.rating === stars) session.patchTracks([id], { rating: before });
    toast.show(`Could not save the rating: ${e}`, "bad");
  }
}
