// Run: node --experimental-strip-types --test src/lib/views/grid/trackPick.test.mjs
//
// The logic of the grid editor's track picker. The picker replaced a native <select> that held one
// <option> per track of the library: at 20,000 tracks that was half a second of layout on a fast
// PC and seconds on a weak one, every time the editor opened. The picker draws only the matches
// it shows, so opening costs the same in any library.

export interface Pickable {
  track_id: number;
  artist: string;
  title: string;
  location: string;
}

/** The words a track is listed and searched by: "Artist - Title", or the title or file name alone. */
export function trackLabel(t: Pickable): string {
  return t.artist && t.title ? `${t.artist} - ${t.title}` : t.title || t.location;
}

/** How many matches are drawn at once; the rest are one more keystroke away. */
export const SHOWN = 100;

export interface Index {
  tracks: readonly Pickable[];
  /** Lower-case label of each track, made once per opening so a keystroke is one pass of `includes`. */
  folded: string[];
}

export function buildIndex(tracks: readonly Pickable[]): Index {
  return { tracks, folded: tracks.map((t) => trackLabel(t).toLowerCase()) };
}

export interface Matches {
  rows: Pickable[];
  /** Matches beyond the ones in `rows`. */
  more: number;
}

/** The tracks whose label holds every word of `query` (any order, any case), in library order. */
export function matchTracks(ix: Index, query: string, limit = SHOWN): Matches {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const rows: Pickable[] = [];
  let found = 0;
  for (let i = 0; i < ix.tracks.length; i++) {
    const label = ix.folded[i];
    if (words.every((w) => label.includes(w))) {
      if (found < limit) rows.push(ix.tracks[i]);
      found++;
    }
  }
  return { rows, more: found - rows.length };
}

/** The highlighted row after an arrow key: it stops at the ends rather than wrapping, so holding a key is predictable. */
export function moveActive(active: number, delta: number, count: number): number {
  if (count <= 0) return -1;
  return Math.min(count - 1, Math.max(0, active + delta));
}
