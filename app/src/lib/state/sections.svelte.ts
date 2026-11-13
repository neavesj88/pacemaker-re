// Sections (phrase detection): the app-side intro / build / drop / breakdown / outro estimates of
// the open library, and the job that makes them.
//
// For other features (the mix editor, a set builder):
//   sections.of(trackId)                      the track's sections, or null
//   sections.snap(trackId, secs, options)     the nearest section boundary to snap to, or null
//   sections.boundaries(trackId, options)     all boundaries, ascending
//   sections.neighbour(trackId, secs, dir)    the next / previous boundary
// Times are seconds in the beat grid's time base (the mix editor's source-time clip offsets).
// These are estimates from the audio and say how sure they are (`confidence`); a track without
// any (never analysed, no grid, no clear structure) gives null, and the caller keeps its own
// position.
import { analyseSections, clearSections, planSections, saveSectionsSettings, sectionsState, type SectionOut, type SectionsSettings, type SectionsState, type TrackSections } from "$lib/api/analysis";
import { baseName } from "$lib/format";
import { boundariesOf, entriesById, neighbourBoundary, RUN_DESCRIPTION, runTitle, snapToBoundary, type Boundary, type Snap, type SnapOptions } from "$lib/views/analysis/sectionLogic";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

class Sections {
  // raw: replaced as a whole, never edited in place; a deep proxy would wrap every entry and section list
  state = $state.raw<SectionsState | null>(null);
  /** The "Sections" panel is open. */
  panelOpen = $state(false);
  busy = $state(false);

  private byId = $derived(entriesById(this.state?.entries ?? []));

  /** Markers are drawn on waveforms (a setting). */
  showMarkers = $derived(this.state?.settings.show_markers ?? true);

  entry(trackId: number): TrackSections | null {
    return this.byId.get(trackId) ?? null;
  }

  /** The sections of a track, ascending and covering it end to end, or null when it has none. */
  of(trackId: number): SectionOut[] | null {
    return this.byId.get(trackId)?.sections ?? null;
  }

  /** Boundaries of a track (the start of every section but the first), optionally filtered. */
  boundaries(trackId: number, opts: SnapOptions = {}): Boundary[] {
    return boundariesOf(this.of(trackId) ?? [], opts);
  }

  /** The section boundary nearest to `secs` within `opts.maxSecs` (default 8 s), or null. */
  snap(trackId: number, secs: number, opts: SnapOptions = {}): Snap | null {
    return snapToBoundary(this.of(trackId) ?? [], secs, opts);
  }

  /** The next (`dir` 1) or previous (`dir` -1) boundary of a track, or null. */
  neighbour(trackId: number, secs: number, dir: 1 | -1, opts: SnapOptions = {}): Boundary | null {
    return neighbourBoundary(this.of(trackId) ?? [], secs, dir, opts);
  }

  /** Reads the sections of the open library again (after a job, a setting change, or opening). */
  async load() {
    const root = session.library?.root;
    if (!root) {
      this.state = null;
      return;
    }
    try {
      const s = await sectionsState(root);
      if (session.library?.root === root) this.state = s;
    } catch (e) {
      if (session.library?.root === root) {
        this.state = null;
        toast.show(`Could not read the sections: ${e}`, "bad");
      }
    }
  }

  openPanel() {
    this.panelOpen = true;
  }

  closePanel() {
    this.panelOpen = false;
  }

  /** Confirm analysing `ids` (default: every track). Without `recompute` only tracks never
   *  analysed, or whose file or grid changed since, are included; the list in the question is the
   *  backend's own plan and the run is started with exactly that list. */
  async askAnalyse(ids?: number[], recompute = false) {
    const lib = session.library;
    if (!lib || this.busy) return;
    if (!jobs.guard()) return;
    this.busy = true;
    try {
      let plan = await planSections(lib.root, ids, recompute);
      // tracks picked by hand that have a result already (even a flat or skipped one) are measured
      // again on request: the question below names every one of them, so nothing runs unseen
      if (plan.length === 0 && ids?.length && !recompute) {
        plan = await planSections(lib.root, ids, true);
        recompute = plan.length > 0;
      }
      if (plan.length === 0) {
        toast.show(
          ids?.length
            ? "Those tracks were already analysed and their beat grids have not changed. To find more or fewer sections, change the sensitivity in the Sections panel."
            : "Every track is already analysed and no beat grid has changed.",
          "info",
        );
        return;
      }
      const byId = new Map(lib.tracks.map((t) => [t.track_id, t]));
      const n = plan.length;
      jobs.ask({
        kind: "sections",
        root: lib.root,
        count: n,
        names: plan.map((id) => {
          const t = byId.get(id);
          return t ? t.title || baseName(t.location) : `Track ${id}`;
        }),
        title: runTitle(n, lib.root, recompute),
        description: RUN_DESCRIPTION,
        confirmLabel: `Analyse ${n}`,
        runningLabel: "Detecting sections",
        doneLabel: "Section detection finished",
        start: () => analyseSections(lib.root, plan, recompute),
      });
    } catch (e) {
      session.error = String(e);
    } finally {
      this.busy = false;
    }
  }

  /** Saves the settings; the sections are recomputed from the saved measurements, with no new decode. */
  async saveSettings(settings: SectionsSettings): Promise<boolean> {
    try {
      await saveSectionsSettings(settings);
      await this.load();
      return true;
    } catch (e) {
      toast.show(`Could not save the section settings: ${e}`, "bad");
      return false;
    }
  }

  /** Drops every saved result of the open library (the panel asks first). */
  async clear(): Promise<number | null> {
    const root = session.library?.root;
    if (!root) return null;
    try {
      const n = await clearSections(root);
      await this.load();
      return n;
    } catch (e) {
      toast.show(`Could not forget the sections: ${e}`, "bad");
      return null;
    }
  }
}

export const sections = new Sections();
session.onReset(() => {
  sections.state = null;
  sections.closePanel();
});
