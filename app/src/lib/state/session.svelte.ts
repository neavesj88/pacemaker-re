// The open library and the detected devices. Everything else reads `session.library`.
import { chooseLibraryFolder, listVolumes, openLibrary } from "$lib/api/library";
import type { Collection, LibraryView, NeedsAnalysis, Track } from "$lib/api/types";
import { nav } from "./nav.svelte";

const resetHooks: (() => void)[] = [];

/** One open library. Its identity is stable for as long as it is open (an in-place reload makes a
 *  new one), so an effect that reads `session.library` re-runs on a load and nothing else. The three
 *  lists a write can change are shallow signals: replacing one re-runs only its readers. They are
 *  `$state.raw` on purpose: a deep proxy cost 489 ms and about 90 MB on the first read of 30k
 *  tracks and about 180 ms on every later full read, against about 10 ms and nothing for a plain
 *  array. A track is therefore never changed in place; `Session.patchTracks` swaps in a new object. */
class OpenLibrary implements LibraryView {
  readonly root: string;
  readonly problems: string[];
  readonly needs_analysis: NeedsAnalysis[];
  tracks = $state.raw<Track[]>([]);
  mixes = $state.raw<Collection[]>([]);
  cases = $state.raw<Collection[]>([]);

  constructor(v: LibraryView) {
    this.root = v.root;
    this.problems = v.problems;
    this.needs_analysis = v.needs_analysis;
    this.tracks = v.tracks;
    this.mixes = v.mixes;
    this.cases = v.cases;
  }
}

class Session {
  volumes = $state<string[]>([]);
  #library = $state.raw<OpenLibrary | null>(null);
  loading = $state(false);
  /** Shown in the red banner under the header. Any area may set it; "" clears it. */
  error = $state("");

  /** The open library (read only: change it through `patchTracks`, `setCases` and `setMixes`). */
  get library(): LibraryView | null {
    return this.#library;
  }

  /** Tracks whose .str is missing, empty or unreadable (or never analysed), with the reason. */
  needs = $derived(new Map((this.library?.needs_analysis ?? []).map((n) => [n.track_id, n.reason])));

  /** A store registers here to clear its own state when a different library is opened (not on
   *  an in-place reload). Called at module load by the store itself; session imports no store. */
  onReset(fn: () => void) {
    resetHooks.push(fn);
  }

  /** Shows `fields` on the tracks in `ids` at once (a rating, a key). The changed tracks become new
   *  objects in a new `tracks` array, the rest keep theirs, so a table redraws only those rows. */
  patchTracks(ids: Iterable<number>, fields: Partial<Track>) {
    const lib = this.#library;
    if (!lib) return;
    const want = new Set(ids);
    let hit = false;
    const next = lib.tracks.map((t) => {
      if (!want.has(t.track_id)) return t;
      hit = true;
      return { ...t, ...fields };
    });
    if (hit) lib.tracks = next;
  }

  /** Replaces the case counts after the case list was re-read. */
  setCases(cases: Collection[]) {
    if (this.#library) this.#library.cases = cases;
  }

  /** Replaces the mix list after the mix list was re-read. */
  setMixes(mixes: Collection[]) {
    if (this.#library) this.#library.mixes = mixes;
  }

  async refreshVolumes() {
    // Detected devices are listed, never opened automatically: opening reads every track's .str,
    // and a docked device may have an ageing hard drive. The user chooses when to read it.
    this.volumes = await listVolumes();
  }

  /** Reads the library on `root`. `keepView` re-reads in place (after a job) without resetting
   *  the view, the open case or the filters. */
  async load(root: string, keepView = false) {
    this.loading = true;
    this.error = "";
    try {
      this.#library = new OpenLibrary(await openLibrary(root));
      if (!keepView) {
        nav.go("tracks");
        for (const fn of resetHooks) fn();
      }
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  async openFolder() {
    const dir = await chooseLibraryFolder();
    if (dir) await this.load(dir);
  }
}

export const session = new Session();
