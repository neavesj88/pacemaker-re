// Energy (auto): the app-side 1-10 estimates of the open library, and the job that makes them.
// `energy.of(trackId)` is the read function for everything else (the Tracks column, the gap map,
// a later set builder): the auto estimate, or null when the track has none. It is not the user's
// own energy rating.
import { analyseEnergy, clearEnergy, energyState, planEnergy, saveEnergySettings, type EnergyEntry, type EnergySettings, type EnergyState } from "$lib/api/analysis";
import { baseName } from "$lib/format";
import { energyById, RUN_DESCRIPTION, runTitle } from "$lib/views/analysis/energyLogic";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

class Energy {
  // raw: replaced as a whole, never edited in place; a deep proxy would wrap every entry of a big library
  state = $state.raw<EnergyState | null>(null);
  /** The "Energy (auto)" panel is open. */
  panelOpen = $state(false);
  busy = $state(false);

  private byId = $derived(energyById(this.state?.entries ?? []));

  /** The auto energy (1.0 to 10.0) of a track, or null when it has none. */
  of(trackId: number): number | null {
    return this.byId.get(trackId)?.energy ?? null;
  }

  entry(trackId: number): EnergyEntry | null {
    return this.byId.get(trackId) ?? null;
  }

  /** Reads the estimates of the open library again (after a job, a setting change, or opening). */
  async load() {
    const root = session.library?.root;
    if (!root) {
      this.state = null;
      return;
    }
    try {
      const s = await energyState(root);
      if (session.library?.root === root) this.state = s;
    } catch (e) {
      if (session.library?.root === root) {
        this.state = null;
        toast.show(`Could not read the energy estimates: ${e}`, "bad");
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
   *  analysed or changed since are included; the list in the question is the backend's own plan
   *  and the run is started with exactly that list. */
  async askAnalyse(ids?: number[], recompute = false) {
    const lib = session.library;
    if (!lib || this.busy) return;
    if (!jobs.guard()) return;
    this.busy = true;
    try {
      let plan = await planEnergy(lib.root, ids, recompute);
      if (plan.length === 0 && !recompute && ids?.length) {
        // nothing new: tracks analysed before that came out too short or silent have no estimate
        // but are not "missing"; offer to measure exactly those again, by name in the question
        const without = ids.filter((id) => this.of(id) === null);
        if (without.length > 0) {
          plan = await planEnergy(lib.root, without, true);
          recompute = true;
        }
      }
      if (plan.length === 0) {
        toast.show("Those tracks already have an energy estimate.", "info");
        return;
      }
      const byId = new Map(lib.tracks.map((t) => [t.track_id, t]));
      const n = plan.length;
      jobs.ask({
        kind: "energy",
        root: lib.root,
        count: n,
        names: plan.map((id) => {
          const t = byId.get(id);
          return t ? t.title || baseName(t.location) : `Track ${id}`;
        }),
        title: runTitle(n, lib.root, recompute),
        description: RUN_DESCRIPTION,
        confirmLabel: `Analyse ${n}`,
        runningLabel: "Analysing energy",
        doneLabel: "Energy analysis finished",
        start: () => analyseEnergy(lib.root, plan, recompute),
      });
    } catch (e) {
      session.error = String(e);
    } finally {
      this.busy = false;
    }
  }

  /** Saves the settings; the numbers are recomputed from the saved measurements, with no new decode. */
  async saveSettings(settings: EnergySettings): Promise<boolean> {
    try {
      await saveEnergySettings(settings);
      await this.load();
      return true;
    } catch (e) {
      toast.show(`Could not save the energy settings: ${e}`, "bad");
      return false;
    }
  }

  /** Drops every saved estimate of the open library (the panel asks first). */
  async clear(): Promise<number | null> {
    const root = session.library?.root;
    if (!root) return null;
    try {
      const n = await clearEnergy(root);
      await this.load();
      return n;
    } catch (e) {
      toast.show(`Could not forget the energy estimates: ${e}`, "bad");
      return null;
    }
  }
}

export const energy = new Energy();
session.onReset(() => {
  energy.state = null;
  energy.closePanel();
});
