// The Libraries view's state: the library on the right, what each pane shows (tab, search,
// selection), the copy in flight (the plan the user is asked to confirm, then the running job).
//
// The left pane is always the open library (`session.library`). The right one is another library
// chosen from a picker. A detected Pacemaker is read only when the user presses the button for it
// (`read`); a library on this computer is read as soon as it is chosen. Reading lists the tracks,
// mixes and cases from music.db alone (`library_pane`), nothing else on the drive.
//
// A copy is planned by the backend for exactly the picked items (`syncPlanSelection`), shown in a
// dialog, and run as the same write job the Sync view uses (`syncApplySelection`): the run is that
// plan, or it does not run. When it ends the library that was written is read again.
import { libraryPane } from "$lib/api/library";
import { JOB_SYNC, syncApplySelection, syncPlanSelection, type SyncPlan, type SyncSelection } from "$lib/api/sync";
import type { DoneEvent, LibraryView, PaneLibrary } from "$lib/api/types";
import { nav } from "$lib/state/nav.svelte";
import { rootKey, sameRoot } from "$lib/state/place";
import { session } from "$lib/state/session.svelte";
import { device } from "../device/device.svelte";
import { isStale, type LibOption } from "../sync/syncLogic";
import {
  buildSelection, canCopy, emptySelected, otherSide, readsAtOnce, selectionSize, STALE_COPY_TEXT,
  type PaneData, type Selected, type Side, type Tab,
} from "./librariesLogic";

/** A copy waiting for the user's yes. */
export type Pending = {
  plan: SyncPlan;
  selection: SyncSelection;
  source: string;
  target: string;
  /** The pane the items came from. */
  from: Side;
};

export const paneOf = (v: LibraryView | PaneLibrary): PaneData => ({ root: v.root, kind: v.kind, tracks: v.tracks, mixes: v.mixes, cases: v.cases });

class LibrariesStore {
  /** The library on the right ("" = none chosen). */
  rightRoot = $state("");
  /** Libraries chosen with "Open folder…", offered in the picker beside the detected ones. */
  folders = $state<LibOption[]>([]);
  /** What was read for the right pane, by root. Kept while the picker moves back and forth, and
   *  dropped (`invalidate`) when anything may have changed it: a job that wrote, a rescan, a
   *  different open library, coming back to the view. A dropped Pacemaker waits for its button. */
  loaded = $state.raw<Map<string, PaneData>>(new Map());
  /** The root being read now, "" when none. */
  reading = $state("");
  readError = $state("");

  tab = $state<Record<Side, Tab>>({ left: "tracks", right: "tracks" });
  search = $state<Record<Side, string>>({ left: "", right: "" });
  selected = $state.raw<Record<Side, Selected>>({ left: emptySelected(), right: emptySelected() });
  /** Where the last plain or ctrl click was, per pane and tab, for shift-click ranges. */
  anchor: Record<Side, Record<Tab, number | null>> = {
    left: { tracks: null, mixes: null, cases: null },
    right: { tracks: null, mixes: null, cases: null },
  };
  /** The pane a drag is hovering over, and the case row under it, for the highlight. */
  over = $state<{ side: Side; caseName: string | null } | null>(null);

  planning = $state(false);
  pending = $state.raw<Pending | null>(null);
  running = $state(false);
  error = $state("");
  /** The sentence the last copy ended with, kept under the toolbar until the next one. */
  result = $state("");

  #gen = 0;
  /** The library the running job writes, so it is read again when it ends. */
  #target = "";

  get busy() {
    return this.planning || this.running || device.blocked;
  }

  /** What a pane lists: the open library on the left, what was read for the chosen one on the right. */
  data(side: Side): PaneData | null {
    if (side === "left") return session.library ? paneOf(session.library) : null;
    return this.rightRoot ? (this.loaded.get(rootKey(this.rightRoot)) ?? null) : null;
  }

  /** Clears what belongs to the library that was open: selections, searches, the dialog, and what was
   *  read for the right pane (it is read again on request). */
  reset() {
    this.loaded = new Map();
    this.tab = { left: "tracks", right: "tracks" };
    this.search = { left: "", right: "" };
    this.selected = { left: emptySelected(), right: emptySelected() };
    this.anchor = {
      left: { tracks: null, mixes: null, cases: null },
      right: { tracks: null, mixes: null, cases: null },
    };
    this.pending = null;
    this.over = null;
    this.error = "";
    this.result = "";
    this.#gen++;
    this.planning = false;
  }

  // ---- the right pane -------------------------------------------------------------------------

  /** Chooses the library on the right. A library on this computer (or a folder chosen by hand) is read
   *  now; a detected Pacemaker waits for `read`. */
  chooseRight(root: string, readNow = false) {
    this.rightRoot = root;
    this.readError = "";
    this.selected = { ...this.selected, right: emptySelected() };
    this.search = { ...this.search, right: "" };
    if (!this.loaded.has(rootKey(root)) && readsAtOnce(root, session.volumes, this.folders.map((f) => f.value), readNow)) void this.read(root);
  }

  /** Forgets everything read for the right pane because it may be out of date. A library on this
   *  computer is read again at once; a Pacemaker only when the user presses its button. */
  invalidate() {
    this.loaded = new Map();
    this.readError = "";
    this.selected = { ...this.selected, right: emptySelected() };
    if (this.rightRoot && readsAtOnce(this.rightRoot, session.volumes, this.folders.map((f) => f.value), false)) void this.read(this.rightRoot);
  }

  /** The Rescan button: looks for drives again and forgets what was read of the other library. */
  async rescan() {
    await session.refreshVolumes();
    this.invalidate();
  }

  /** Lists the library at `root` for the right pane (the button "Read the Pacemaker (G:)"). */
  async read(root: string) {
    if (this.reading) return;
    this.reading = root;
    this.readError = "";
    try {
      const p = await libraryPane(root);
      const next = new Map(this.loaded);
      next.set(rootKey(root), paneOf(p));
      this.loaded = next;
    } catch (e) {
      this.readError = String(e);
    } finally {
      this.reading = "";
    }
  }

  /** Adds a folder chosen by hand to the picker and shows it (a deliberate choice, so it is read). */
  addFolder(dir: string, label: string, kind: LibOption["kind"]) {
    if (!this.folders.some((f) => sameRoot(f.value, dir))) this.folders = [...this.folders, { value: dir, label, kind }];
    this.chooseRight(dir, true);
  }

  // ---- selection ------------------------------------------------------------------------------

  setSelection(side: Side, tab: Tab, ids: ReadonlySet<number>, anchor: number | null) {
    this.selected = { ...this.selected, [side]: { ...this.selected[side], [tab]: ids } };
    this.anchor[side][tab] = anchor;
  }

  // ---- swap -----------------------------------------------------------------------------------

  /** Opens the right library as the open one; the one that was open moves to the right. Opening
   *  reads the library in full (its analysis files too), as opening it anywhere does. */
  async swap() {
    const lib = session.library;
    const right = this.data("right");
    if (!lib || !right || this.busy) return;
    const old = paneOf(lib);
    await session.load(right.root);
    if (!session.library || !sameRoot(session.library.root, right.root)) return; // the banner says why
    const next = new Map(this.loaded);
    next.set(rootKey(old.root), old);
    this.loaded = next;
    this.rightRoot = old.root;
    this.readError = "";
    nav.go("libraries");
  }

  // ---- copy -----------------------------------------------------------------------------------

  /** Plans a copy of the given rows of `from`'s `tab` to the other pane's library and, if there is
   *  something to ask about, opens the dialog. `into` names a case on the other side the tracks go
   *  into too. */
  async copy(from: Side, tab: Tab, ids: Iterable<number>, into: { name: string; create: boolean } | null = null) {
    const src = this.data(from);
    const dst = this.data(otherSide(from));
    if (this.busy) return;
    if (!src || !dst) {
      this.error = "Both libraries have to be showing to copy between them.";
      return;
    }
    const selection = buildSelection(src, tab, ids, into);
    if (selectionSize(selection) === 0) {
      this.error = "Pick something to copy first.";
      return;
    }
    const gen = ++this.#gen;
    this.planning = true;
    this.error = "";
    this.result = "";
    try {
      const plan = await syncPlanSelection(src.root, dst.root, selection);
      if (gen !== this.#gen) return;
      this.pending = { plan, selection, source: src.root, target: dst.root, from };
    } catch (e) {
      if (gen === this.#gen) this.error = String(e);
    } finally {
      if (gen === this.#gen) this.planning = false;
    }
  }

  cancelPending() {
    this.pending = null;
  }

  /** The dialog's Copy: runs exactly the plan that was shown. */
  async confirm() {
    const p = this.pending;
    if (!p || this.busy || !canCopy(p.plan)) return;
    this.pending = null;
    this.error = "";
    this.result = "";
    this.#target = p.target;
    try {
      // the done event can beat the reply of a very short run, so the run is marked before the call
      this.running = true;
      device.begin(JOB_SYNC, "Starting the copy");
      await syncApplySelection(p.source, p.target, p.selection, p.plan.digest);
    } catch (e) {
      this.running = false;
      device.job = null;
      this.fail(String(e));
    }
  }

  fail(msg: string) {
    this.error = isStale(msg) ? STALE_COPY_TEXT : msg;
  }

  /** job-done for "device-sync" (only the run this store started). The device store shows the
   *  toast; this keeps the sentence on screen and reads the library that was written again. */
  async onDone(ev: DoneEvent) {
    // any other job (a Sync-view run, an import) may have written the library the right pane shows
    if (ev.job !== JOB_SYNC || !this.running) {
      this.invalidate();
      return;
    }
    this.running = false;
    const cancelled = !!ev.error && ev.error.includes("Cancelled:");
    if (ev.error && !cancelled) this.fail(ev.error);
    else this.result = cancelled ? ev.error!.slice(ev.error!.indexOf("Cancelled:")) : ev.summary;
    await this.refreshTarget();
  }

  /** Reads the library the job wrote again: the open one by the session, the other from its music.db. */
  async refreshTarget() {
    const t = this.#target;
    if (!t) return;
    const lib = session.library;
    if (lib && sameRoot(lib.root, t)) await session.load(lib.root, true);
    else if (this.rightRoot && sameRoot(this.rightRoot, t)) {
      await this.read(t);
    }
    void session.refreshComputer();
  }

  cancel() {
    device.cancel();
  }
}

export const libs = new LibrariesStore();
session.onReset(() => libs.reset());
