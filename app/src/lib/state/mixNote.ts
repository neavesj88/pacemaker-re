// The sentence a confirmation adds when a tempo or beat-grid change touches tracks that mixes use.
// A mix clip keeps the tempo its track had when the mix was made, so a new grid or BPM leaves
// those mixes out of step until they are beatmatched again: the confirmation names them.
import { tracksMixesUsing } from "$lib/api/tracks";

/** " 2 mixes use some of these tracks (A, B): ...", or "" when no mix uses them. When the lookup
 *  fails the note says so, because staying silent would read as "no mix is affected". */
export async function mixesNote(root: string, ids: number[]): Promise<string> {
  try {
    const mixes = await tracksMixesUsing(root, ids);
    if (mixes.length === 0) return "";
    return ` ${mixes.length === 1 ? "1 mix uses" : `${mixes.length} mixes use`} some of these tracks (${mixes.join(", ")}): a mix clip keeps the tempo the track had when the mix was made, so ${mixes.length === 1 ? "that mix" : "those mixes"} may drift out of step until beatmatched again.`;
  } catch (err) {
    return ` Could not check which mixes use these tracks (${String(err)}); a mix keeps the tempo its tracks had when it was made.`;
  }
}
