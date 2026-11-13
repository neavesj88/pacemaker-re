// The "Energy source" setting (your rating, the audio estimate, or your rating else the estimate)
// and the one resolver every energy reader goes through. The setting is the app's own preference,
// kept in the app config dir by the backend (commands/tags/energy_source.rs) because the smart-case
// rules read it there when a smart case's Pacemaker copy is rewritten.
import { energySourceGet, energySourceSet } from "$lib/api/tags";
import { DEFAULT_ENERGY_SOURCE, energyUsed, resolveLevel, type EnergySource } from "./energyResolve";
import { energy } from "./energy.svelte";
import { labels } from "./labels.svelte";
import { toast } from "./toast.svelte";

class EnergySourceState {
  source = $state<EnergySource>(DEFAULT_ENERGY_SOURCE);
  /** Set when the saved choice could not be read (the default is then in force). */
  problem = $state("");
  loaded = $state(false);

  async load() {
    try {
      const v = await energySourceGet();
      this.source = v.source;
      this.problem = v.problem ?? "";
    } catch (e) {
      toast.show(`Could not read the energy source setting: ${e}`, "bad");
    } finally {
      this.loaded = true;
    }
  }

  /** Saves the choice; true when it is in force. Anything that shows a smart case's matches
   *  re-reads them when `source` changes. */
  async set(source: EnergySource): Promise<boolean> {
    const before = this.source;
    this.source = source; // the lists change at once; put back if the save fails
    try {
      const v = await energySourceSet(source);
      this.source = v.source;
      this.problem = "";
      return true;
    } catch (e) {
      this.source = before;
      toast.show(`Could not save the energy source: ${e}`, "bad");
      return false;
    }
  }

  /** Whole-level energy (1 to 10) of a track under the setting, or null. The one answer for the
   *  Tracks search and label filter, the gap map, the set builder and the printable setlist. */
  level(trackId: number): number | null {
    return resolveLevel(labels.get(trackId)?.energy ?? null, energy.of(trackId), this.source);
  }

  /** Your own rating of a track (1 to 10), or null. */
  mine(trackId: number): number | null {
    return labels.get(trackId)?.energy ?? null;
  }

  /** The estimate from the audio (1.0 to 10.0, unrounded), or null. */
  auto(trackId: number): number | null {
    return energy.of(trackId);
  }

  /** Which of the two the setting uses for this track, or null when it has no energy. */
  used(trackId: number): "mine" | "auto" | null {
    return energyUsed(this.mine(trackId), this.auto(trackId), this.source);
  }
}

export const energySource = new EnergySourceState();
