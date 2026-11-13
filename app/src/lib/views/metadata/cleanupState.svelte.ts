// The metadata clean-up tool: its saved recipes, which tracks it looks at, the preview of what
// the recipes would change (computed by the backend with the code that applies it), the
// per-change ticks, and the apply. Recipes and maps are saved as they are edited (app config
// dir, never the device).
import { untrack } from "svelte";
import {
  applyCleanup,
  cleanupDefaults,
  cleanupSettings,
  previewCleanup,
  saveCleanupSettings,
  type CleanupPreview,
  type CleanupSettings,
} from "$lib/api/metacleanup";
import { caseTracks, listCases, type CaseInfo } from "$lib/api/cases";
import { selection } from "$lib/state/selection.svelte";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";
import { acceptedOf, fieldsByTrack, idsOf, plural, tally, tickKey } from "./cleanupLogic";
import { tagWrite } from "./tagwriteState.svelte";

export type Scope =
  | { kind: "selection" }
  | { kind: "library" }
  | { kind: "case"; caseId: number }
  /** Tracks handed over by another part of the app (the track editor). */
  | { kind: "ids"; ids: number[] };

class CleanupState {
  settings = $state<CleanupSettings | null>(null);
  /** Set when the saved recipes could not be read and the defaults are shown instead. */
  warning = $state("");
  saved = $state<"" | "saving" | "saved" | "error">("");
  saveError = $state("");

  scope = $state<Scope>({ kind: "selection" });
  cases = $state<CaseInfo[]>([]);
  caseIds = $state<number[]>([]);

  preview = $state<CleanupPreview | null>(null);
  /** Changes the user turned off, by `tickKey(track, field)`. Kept across recomputes. */
  unticked = $state<Record<string, boolean>>({});
  busy = $state(false);
  applying = $state(false);
  error = $state("");

  #lastSaved = "";
  #saveTimer: ReturnType<typeof setTimeout> | undefined;
  #previewTimer: ReturnType<typeof setTimeout> | undefined;
  #ticket = 0;

  /** The tracks the scope names, as ids that exist in the open library. */
  ids = $derived.by((): number[] => {
    const lib = session.library;
    if (!lib) return [];
    const have = new Set(lib.tracks.map((t) => t.track_id));
    const sc = this.scope;
    if (sc.kind === "library") return lib.tracks.map((t) => t.track_id);
    if (sc.kind === "selection") return selection.trackIds.filter((id) => have.has(id));
    if (sc.kind === "case") return this.caseIds.filter((id) => have.has(id));
    return sc.ids.filter((id) => have.has(id));
  });

  /** Anything that changes what the preview shows. */
  signature = $derived(JSON.stringify([this.settings, this.ids]));

  totals = $derived(tally(this.preview, this.unticked));

  constructor() {
    session.onReset(() => {
      this.preview = null;
      this.unticked = {};
      this.caseIds = [];
      this.cases = [];
      this.scope = { kind: "selection" };
    });
  }

  async load() {
    if (this.settings) return;
    try {
      const r = await cleanupSettings();
      this.settings = r.settings;
      this.warning = r.warning ?? "";
      this.#lastSaved = JSON.stringify(r.settings);
    } catch (e) {
      this.error = String(e);
    }
  }

  async loadCases() {
    const root = session.library?.root;
    if (!root) return;
    try {
      this.cases = await listCases(root);
    } catch (e) {
      this.error = String(e);
    }
  }

  async setScope(scope: Scope) {
    this.scope = scope;
    this.caseIds = [];
    if (scope.kind === "case") {
      const root = session.library?.root;
      if (!root) return;
      try {
        this.caseIds = await caseTracks(root, scope.caseId);
      } catch (e) {
        this.error = String(e);
      }
    }
  }

  /** Called whenever `signature` changed: saves the recipes and recomputes the preview, a moment later. */
  schedule() {
    if (!this.settings) return;
    clearTimeout(this.#previewTimer);
    this.scheduleSave();
    this.#previewTimer = setTimeout(() => void this.refresh(), 350);
  }

  /** Saves the recipes a moment after they changed, without a preview (the Settings view has none). */
  scheduleSave() {
    if (!this.settings) return;
    clearTimeout(this.#saveTimer);
    if (JSON.stringify(this.settings) !== this.#lastSaved) {
      this.saved = "saving";
      this.#saveTimer = setTimeout(() => void this.#save(), 700);
    }
  }

  async #save() {
    const s = this.settings;
    if (!s) return;
    const text = JSON.stringify(s);
    try {
      await saveCleanupSettings($state.snapshot(s) as CleanupSettings);
      this.#lastSaved = text;
      this.saved = "saved";
      this.saveError = "";
    } catch (e) {
      this.saved = "error";
      this.saveError = String(e);
    }
  }

  async resetToDefaults() {
    try {
      this.settings = await cleanupDefaults();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Recomputes the preview with the current recipes over the current tracks (read-only). */
  async refresh() {
    const root = session.library?.root;
    const s = this.settings;
    if (!root || !s) return;
    const ids = untrack(() => this.ids);
    const ticket = ++this.#ticket;
    if (ids.length === 0) {
      this.preview = null;
      this.busy = false;
      return;
    }
    this.busy = true;
    this.error = "";
    try {
      const p = await previewCleanup(root, ids, $state.snapshot(s) as CleanupSettings);
      if (ticket === this.#ticket) this.preview = p;
    } catch (e) {
      if (ticket === this.#ticket) {
        this.error = String(e);
        this.preview = null;
      }
    } finally {
      if (ticket === this.#ticket) this.busy = false;
    }
  }

  setTicked(trackId: number, field: string, on: boolean) {
    const k = tickKey(trackId, field);
    if (on) delete this.unticked[k];
    else this.unticked[k] = true;
  }

  /** Writes the ticked changes to the library (music.db, through the write gate). With write-back
   *  switched on in the settings it then reads the audio files and prepares the tag write.
   *  Resolves to true when the tag write was prepared (the view then shows it). */
  async apply(): Promise<boolean> {
    const root = session.library?.root;
    const s = this.settings;
    if (!root || !s || this.applying) return false;
    const accepted = acceptedOf(this.preview, this.unticked);
    if (accepted.length === 0) return false;
    this.applying = true;
    this.error = "";
    try {
      const r = await applyCleanup(root, $state.snapshot(s) as CleanupSettings, accepted);
      await session.load(root, true);
      const dropped = r.stale + r.rejected;
      toast.show(
        `Cleaned ${plural(r.fields, "field")} on ${plural(r.tracks, "track")}` +
          (dropped ? `; ${dropped} left alone (changed since the preview, or too long)` : ""),
        dropped ? "info" : "ok",
        dropped ? 7000 : 4000,
      );
      await this.refresh();
      if (s.write_back && r.fields > 0) {
        const fields = [...new Set([...fieldsByTrack(accepted).values()].flat())];
        await tagWrite.checkTracks(idsOf(accepted), fields);
        return true;
      }
    } catch (e) {
      this.error = String(e);
    } finally {
      this.applying = false;
    }
    return false;
  }
}

export const cleanup = new CleanupState();
