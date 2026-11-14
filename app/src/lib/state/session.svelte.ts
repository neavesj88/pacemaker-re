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
import { allSaveable, unsavedWork, type UnsavedWork } from "./leaveGuard";
import { LoadSeq, reloadAfter } from "./loadSeq";
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
  /** The user asked for a different library and it is being read (not a reload in place after a job). */
  switching = $state(false);
  /** Where that read is going, for the "Reading..." line. "" when none. */
  loadingRoot = $state("");
  /** Unsaved work that opening another library would discard, while the user is asked about it. */
  leaving = $state.raw<{ work: UnsavedWork[]; target: string | null; done: (go: boolean) => void } | null>(null);
  /** The library whose Pacemaker was ejected: it is still on screen (read from memory) but nothing can
   *  be written to it and it is no longer offered. Opening a library again makes a new object. */
  #detached = $state.raw<unknown>(null);
  #seq = new LoadSeq();
  /** An in-place reload (after a job or an edit) was refused because a switch was being read: if the
   *  switch fails, the library that stays open is re-read so it does not show stale data. */
  #pendingReload = false;
  /** The error of a failed switch, kept through the re-read it owes (that read starts by clearing errors). */
  #carryError = "";
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

  /** The open library is an ejected Pacemaker. */
  get isDetached(): boolean {
    return !!this.#library && this.#library === this.#detached;
  }

  /** `lib` was ejected: see `#detached`. */
  markDetached(lib: unknown) {
    this.#detached = lib;
  }

  /** The library on this computer as an offer in pickers: null while the settings file is unreadable,
   *  because the path is then only a guess and must not be opened by one click. */
  get computerOffer(): ComputerLibraryInfo | null {
    return this.computer && !this.computer.unreadable ? this.computer : null;
  }

  /** The open library unless it was ejected: what a picker should offer as "the open library". */
  get attached(): LibraryView | null {
    return this.isDetached ? null : this.#library;
  }

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
    const ok = await this.saveComputer({ path: dir, open_at_start: this.computer?.settings.open_at_start ?? true });
    // the Welcome screen then offers to make a library there, rather than saying it was not found
    if (ok) this.pickedPath = dir;
    return ok;
  }
  /** A folder picked for the library on this computer in this run of the app. */
  pickedPath = $state<string | null>(null);

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
    if (!(await this.confirmLeave(this.computer?.path))) return;
    const root = await this.createComputer();
    if (root) await this.load(root, false, true);
  }

  /** Welcome: "Copy my Pacemaker's library to this computer". Makes and opens the library on this
   *  computer, then goes to the Sync view with the Pacemaker as the other side, to get its tracks,
   *  mixes and cases. The Pacemaker is only read when the user confirms the plan there. */
  async copyFromPacemaker(volume: string) {
    if (!(await this.confirmLeave(this.computer?.path))) return;
    const root = await this.createComputer();
    if (!root) return;
    await this.load(root, false, true);
    if (this.library?.root === root) nav.go("sync", { direction: "get", other: volume });
  }

  /** Asks the user before the open library is replaced, when something unsaved would be lost.
   *  Resolves true when there is nothing unsaved, or the user saved it or chose to discard it;
   *  false when they cancelled (the dialog is shell/LeaveDialog.svelte). `target` is the root about to
   *  be opened, named in the question. */
  confirmLeave(target?: string | null): Promise<boolean> {
    const work = unsavedWork();
    if (work.length === 0) return Promise.resolve(true);
    this.leaving?.done(false);
    return new Promise((resolve) => {
      this.leaving = {
        work,
        target: target ?? null,
        done: (go) => {
          this.leaving = null;
          resolve(go);
        },
      };
    });
  }

  /** The unsaved work can all be saved from the question (see `allSaveable`). */
  get leavingSaveable(): boolean {
    return !!this.leaving && allSaveable(this.leaving.work);
  }

  /** Reads the library on `root`. `keepView` re-reads in place (after a job) without resetting
   *  the view, the open case or the filters. Opening a different library (no `keepView`) first
   *  asks about unsaved work unless `confirmed` says the caller already did. Only the newest read
   *  takes effect: a reload that started before a switch, or while one is being read, cannot put
   *  the old library back. A failed read leaves the open library as it was. */
  async load(root: string, keepView = false, confirmed = false) {
    if (!keepView && !confirmed && !(await this.confirmLeave(root))) return;
    const ticket = this.#seq.begin(keepView);
    if (ticket === null) {
      this.#pendingReload = true;
      this.#carryError = "";
      return;
    }
    let opened = false;
    this.loading = true;
    this.switching = !keepView;
    this.loadingRoot = root;
    this.error = keepView ? this.#carryError : "";
    this.#carryError = "";
    try {
      const view = await openLibrary(root);
      if (!this.#seq.current(ticket)) return;
      this.#library = new OpenLibrary(view);
      opened = true;
      if (!keepView) {
        nav.go("tracks", {}, true);
        for (const fn of resetHooks) fn();
      }
    } catch (e) {
      if (this.#seq.current(ticket)) this.error = String(e);
    } finally {
      if (this.#seq.end(ticket)) {
        this.loading = false;
        this.switching = false;
        this.loadingRoot = "";
        // a switch that opened a new library makes the skipped reload moot; one that failed leaves the old
        // library open, which still needs the reload it was refused
        // (an ejected Pacemaker is not re-read: it is gone, and the read would only raise an error)
        const redo = reloadAfter(this.#pendingReload, opened, keepView, this.attached?.root ?? null);
        this.#pendingReload = false;
        if (redo) {
          this.#carryError = this.error;
          void this.load(redo, true);
        }
      }
    }
  }

  async openFolder() {
    const dir = await chooseLibraryFolder();
    if (dir) await this.load(dir);
  }
}

export const session = new Session();
