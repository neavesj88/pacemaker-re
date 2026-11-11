// Watch-folder import: the settings, the last scan of the open library, the ignore list and the
// log of automatic adds. The scan only reads; what it finds goes to the ordinary add flow
// (state/jobs.svelte.ts), which asks first unless the user chose "add automatically". The files
// behind all of this are in the app's config folder (never on the device).
import { untrack } from "svelte";
import {
  watchIgnore,
  watchLogAdd,
  watchScan,
  watchSettingsGet,
  watchSettingsSet,
  watchState,
  watchUnignore,
  type WatchCandidate,
  type WatchIgnored,
  type WatchLogEntry,
  type WatchScan,
  type WatchSettings,
  type WatchState,
} from "$lib/api/import";
import type { DoneEvent } from "$lib/api/types";
import { baseName } from "$lib/format";
import {
  DEFAULT_WATCH_SETTINGS,
  autoDoneText,
  decide,
  logEntryFor,
  mergeQueued,
  scanSignature,
  type ScanReason,
} from "$lib/views/import/watchLogic";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

/** An add this store started on its own, waiting for the job to finish so it can be reported. */
type AutoRun = { root: string; names: string[] };

class WatchStore {
  settings = $state<WatchSettings>({ ...DEFAULT_WATCH_SETTINGS });
  /** Set when the saved settings file was damaged (and has been set aside). */
  settingsProblem = $state("");
  loaded = $state(false);

  /** The last scan of the open library, and what came with it. */
  scan = $state<WatchScan | null>(null);
  ignored = $state<WatchIgnored[]>([]);
  log = $state<WatchLogEntry[]>([]);
  stateProblem = $state("");
  scanning = $state(false);
  scanError = $state("");
  /** Bumped each time the lists change, for views that need to reset their ticks. */
  version = $state(0);

  candidates = $derived<WatchCandidate[]>(this.scan?.candidates ?? []);
  watching = $derived(this.settings.folders.length > 0);

  #root: string | null = null;
  /** The sets of files the user was already told about, per library root. */
  #announced = new Map<string, string>();
  /** Files an automatic add has tried this session; never tried automatically a second time. */
  #tried = new Set<string>();
  #auto: AutoRun | null = null;
  /** A scan asked for while another was running; runs when that one ends, so a refresh after a
   *  settings change, an un-ignore or an add is never lost. */
  #queued: ScanReason | null = null;
  #lastDone: DoneEvent | null = null;

  async loadSettings() {
    try {
      const v = await watchSettingsGet();
      this.settings = v.settings;
      this.settingsProblem = v.problem ?? "";
    } catch (e) {
      // keep the defaults on screen; saving will say whether the file can be written
      this.settingsProblem = `The watch folder settings could not be read: ${String(e)}`;
    } finally {
      this.loaded = true;
    }
  }

  /** Saves the settings; the stored (clamped) copy replaces ours. Throws the error text. */
  async save(next: WatchSettings) {
    this.settings = await watchSettingsSet(next);
    this.settingsProblem = "";
    // a changed folder list or minimum age changes what a scan finds: bring the list up to date
    // (never acting on it, so adding a folder cannot add its files)
    if (session.library) await this.scanNow("refresh");
  }

  #take(st: WatchState) {
    this.ignored = st.ignored;
    this.log = st.log;
    this.stateProblem = st.problem ?? "";
    this.version++;
  }

  /** Loads the remembered scan, ignore list and log of a library (no folder is read). */
  async loadState(root: string) {
    try {
      const st = await watchState(root);
      if (root !== this.#root) return;
      this.scan = st.last_scan && st.last_scan.root === root ? st.last_scan : null;
      this.#take(st);
    } catch (e) {
      this.stateProblem = `The watch folder list could not be read: ${String(e)}`;
    }
  }

  /** Searches the watched folders now and acts on the result as the settings say. Asked while a
   *  scan is running, it is remembered and run once that one has ended. */
  async scanNow(reason: ScanReason = "manual") {
    const root = session.library?.root;
    if (!root || !this.watching) return;
    if (this.scanning) {
      this.#queued = mergeQueued(this.#queued, reason);
      return;
    }
    this.scanning = true;
    this.scanError = "";
    try {
      const scan = await watchScan(root);
      if (root !== this.#root) return; // another library was opened meanwhile
      this.scan = scan;
      this.version++;
      await this.#act(reason, root, scan);
    } catch (e) {
      this.scanError = String(e);
    } finally {
      this.scanning = false;
      const next = this.#queued;
      this.#queued = null;
      if (next) void this.scanNow(next);
    }
  }

  async #act(reason: ScanReason, root: string, scan: WatchScan) {
    const sig = scanSignature(scan);
    const action = decide({
      reason,
      settings: this.settings,
      scan,
      canWrite: jobs.canWrite && !session.loading,
      tried: this.#tried,
      announced: sig !== "" && this.#announced.get(root) === sig,
    });
    if (action.kind === "none") return;
    if (action.kind === "toast") {
      this.#announced.set(root, sig);
      toast.show(action.text, action.tone === "bad" ? "bad" : "info", 9000);
    } else if (action.kind === "confirm") {
      // told only when the question really opened; if another one was waiting, the next scan asks again
      if (await jobs.askAdd(action.paths, { source: "a watched folder" })) this.#announced.set(root, sig);
    } else {
      const names = action.paths.map(baseName);
      this.#auto = { root, names };
      for (const p of action.paths) this.#tried.add(p);
      const started = await jobs.askAdd(action.paths, { source: "a watched folder", auto: true });
      if (started) {
        toast.show(`Adding ${names.length} new file${names.length === 1 ? "" : "s"} from the watched folders.`, "info", 6000);
      } else {
        // nothing started (another question or job in the way, or the backend refused): these
        // files are not "tried", so a later scan offers them to the automatic add again
        this.#auto = null;
        for (const p of action.paths) this.#tried.delete(p);
        if (reason === "manual") {
          toast.show("Nothing was added: another question or job is in the way. Scan again when it is done.", "info", 7000);
        }
      }
    }
  }

  /** The add flow's confirmation for files the user ticked in the list. */
  async review(files: WatchCandidate[]) {
    if (files.length === 0) return;
    await jobs.askAdd(
      files.map((f) => f.path),
      { source: "a watched folder" },
    );
  }

  async ignore(files: WatchCandidate[]) {
    const root = session.library?.root;
    if (!root || files.length === 0) return;
    const st = await watchIgnore(
      root,
      files.map((f) => ({ path: f.path, size: f.size })),
    );
    if (root !== this.#root) return;
    this.scan = st.last_scan && st.last_scan.root === root ? st.last_scan : this.scan;
    this.#take(st);
  }

  async unignore(paths: string[]) {
    const root = session.library?.root;
    if (!root || paths.length === 0) return;
    const st = await watchUnignore(root, paths);
    if (root !== this.#root) return;
    this.#take(st);
    // they are offered again by the next scan; do that now so the list shows them
    await this.scanNow("refresh");
  }

  #onLibrary(root: string | null) {
    if (root === this.#root) return;
    this.#root = root;
    this.scan = null;
    this.ignored = [];
    this.log = [];
    this.stateProblem = "";
    this.scanError = "";
    this.#auto = null;
    if (!root) return;
    void (async () => {
      await this.loadState(root);
      // the settings may still be loading at the very start; wait for them
      if (!this.loaded) await this.loadSettings();
      if (root !== this.#root) return;
      if (this.settings.scan_on_open && this.watching) await this.scanNow("open");
    })();
  }

  /** Reports and logs an automatic add when its job finishes, then brings the list up to date. */
  async #onDone(ev: DoneEvent) {
    if (ev.job !== "add") return;
    const auto = this.#auto;
    this.#auto = null;
    if (auto) {
      toast.show(autoDoneText(ev.summary, ev.failed, ev.error), ev.error || ev.failed > 0 ? "bad" : "ok", 9000);
      try {
        const st = await watchLogAdd(auto.root, logEntryFor("auto", auto.names, ev));
        if (auto.root === this.#root) this.#take(st);
      } catch (e) {
        session.error = `The watch folder log could not be saved: ${String(e)}`;
      }
    }
    // what is left to add (nothing acts on this scan)
    if (this.watching) await this.scanNow("refresh");
  }

  /** Called by the timer. Skipped while a job runs or a library is loading. */
  #tick() {
    if (!session.library || session.loading || jobs.busy || jobs.pending || this.scanning) return;
    void this.scanNow("timer");
  }

  /** Starts watching: loads the settings, scans when a library opens, runs the optional timer
   *  and reports automatic adds. Returns the teardown. */
  start(): () => void {
    void this.loadSettings();
    return $effect.root(() => {
      $effect(() => {
        const root = session.library?.root ?? null;
        untrack(() => this.#onLibrary(root));
      });
      $effect(() => {
        const minutes = this.settings.interval_minutes;
        if (!(minutes > 0) || !this.watching) return;
        const id = setInterval(() => this.#tick(), minutes * 60_000);
        return () => clearInterval(id);
      });
      $effect(() => {
        const done = jobs.job?.result ?? null;
        if (!done || done === this.#lastDone) return;
        this.#lastDone = done;
        untrack(() => void this.#onDone(done));
      });
    });
  }
}

export const watch = new WatchStore();
