// The open library and the detected devices. Everything else reads `session.library`.
import {
  chooseLibraryFolder,
  computerLibraryCreate,
  chooseComputerLibraryFolder,
  computerLibraryGet,
  computerLibrarySet,
  listVolumes,
  openLibrary,
  type ComputerLibraryInfo,
  type ComputerLibrarySettings,
} from "$lib/api/library";
import type { Collection, LibraryKind, LibraryView, NeedsAnalysis, Track } from "$lib/api/types";
import { nav } from "./nav.svelte";
import { placeWords, startupRoot } from "./place";

const resetHooks: (() => void)[] = [];

/** One open library. Its identity is stable for as long as it is open (an in-place reload makes a
 *  new one), so an effect that reads `session.library` re-runs on a load and nothing else. The three
 *  lists a write can change are shallow signals: replacing one re-runs only its readers. They are
 *  `$state.raw` on purpose: a deep proxy cost 489 ms and about 90 MB on the first read of 30k
 *  tracks and about 180 ms on every later full read, against about 10 ms and nothing for a plain
 *  array. A track is therefore never changed in place; `Session.patchTracks` swaps in a new object. */
class OpenLibrary implements LibraryView {
  readonly root: string;
  readonly kind: LibraryKind;
  readonly problems: string[];
  readonly needs_analysis: NeedsAnalysis[];
  tracks = $state.raw<Track[]>([]);
  mixes = $state.raw<Collection[]>([]);
  cases = $state.raw<Collection[]>([]);

  constructor(v: LibraryView) {
    this.root = v.root;
    this.kind = v.kind;
    this.problems = v.problems;
    this.needs_analysis = v.needs_analysis;
    this.tracks = v.tracks;
    this.mixes = v.mixes;
    this.cases = v.cases;
  }
}

class Session {
  volumes = $state<string[]>([]);
  /** The library on this computer: where it is and whether it exists. null until read. */
  computer = $state<ComputerLibraryInfo | null>(null);
  #library = $state.raw<OpenLibrary | null>(null);
  loading = $state(false);
  /** Shown in the red banner under the header. Any area may set it; "" clears it. */
  error = $state("");

  /** The open library (read only: change it through `patchTracks`, `setCases` and `setMixes`). */
  get library(): LibraryView | null {
    return this.#library;
  }

  /** Where the open library is. A Pacemaker until one is open, so no sentence says "this library"
   *  by mistake. */
  get kind(): LibraryKind {
    return this.#library?.kind ?? "pacemaker";
  }

  /** The open library is a folder on this computer, not a Pacemaker. */
  get isComputer(): boolean {
    return this.kind === "computer";
  }

  /** The words that name the open library's place in a sentence ("on the Pacemaker" / "in this
   *  library"). */
  get place() {
    return placeWords(this.kind);
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

  /** Re-reads where the library on this computer is and whether it exists (after a setting changed). */
  async refreshComputer() {
    try {
      this.computer = await computerLibraryGet();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** App start: lists the devices (never opens one) and opens the library on this computer when it
   *  exists and "open at start" is on. */
  async start() {
    this.loading = true;
    try {
      await Promise.all([this.refreshVolumes(), this.refreshComputer()]);
    } finally {
      this.loading = false;
    }
    const root = startupRoot(this.computer);
    if (root) await this.load(root);
  }

  /** Saves the settings of the library on this computer. The library at the old path is not moved
   *  or deleted. False (with the reason in the error banner) when the backend refused them. */
  async saveComputer(settings: ComputerLibrarySettings): Promise<boolean> {
    this.error = "";
    try {
      this.computer = await computerLibrarySet(settings);
      return true;
    } catch (e) {
      this.error = String(e);
      return false;
    }
  }

  /** "Change…": picks a folder for the library on this computer and saves it as the path. */
  async chooseComputerFolder(): Promise<boolean> {
    const dir = await chooseComputerLibraryFolder();
    if (!dir) return false;
    return this.saveComputer({ path: dir, open_at_start: this.computer?.settings.open_at_start ?? true });
  }

  /** Makes the library on this computer (when it is not there) and returns its root, null on failure. */
  async createComputer(): Promise<string | null> {
    this.error = "";
    try {
      const root = await computerLibraryCreate();
      await this.refreshComputer();
      return root;
    } catch (e) {
      this.error = String(e);
      return null;
    }
  }

  /** Welcome: "Start a library on this computer". */
  async startComputer() {
    const root = await this.createComputer();
    if (root) await this.load(root);
  }

  /** Welcome: "Copy my Pacemaker's library to this computer". Makes and opens the library on this
   *  computer, then goes to the Sync view with the Pacemaker as the other side, to get its tracks,
   *  mixes and cases. The Pacemaker is only read when the user confirms the plan there. */
  async copyFromPacemaker(volume: string) {
    const root = await this.createComputer();
    if (!root) return;
    await this.load(root);
    if (this.library?.root === root) nav.go("sync", { direction: "get", other: volume });
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
