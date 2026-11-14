// Which tracks the cases (crates) of the open library hold, in case order, for the case filter, the
// "Cases" column and the case sort of the track tables. Read from the cases area's own command
// (`case_tracks`) once per change of the case list, not once per table. Reads only.
import { caseTracks } from "$lib/api/cases";
import { session } from "./session.svelte";

class CaseMembers {
  /** Track ids of each case, in case order. */
  order = $state.raw<ReadonlyMap<number, readonly number[]>>(new Map());
  loading = $state(false);
  #key = "";
  #seq = 0;

  /** The cases that hold a track, by name, alphabetically; rebuilt when a case changes. */
  #names = $derived.by(() => {
    const names = new Map(session.library?.cases.map((c) => [c.id, c.name]) ?? []);
    const out = new Map<number, string[]>();
    for (const [caseId, ids] of this.order) {
      const name = names.get(caseId);
      if (name === undefined) continue;
      for (const id of ids) {
        const list = out.get(id);
        if (list) list.push(name);
        else out.set(id, [name]);
      }
    }
    for (const list of out.values()) list.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
    return out;
  });

  /** Names of the cases holding the track. */
  namesOf(trackId: number): string[] {
    return this.#names.get(trackId) ?? [];
  }

  /** The tracks of a case in case order, or null while they are not read. */
  tracksOf(caseId: number): readonly number[] | null {
    return this.order.get(caseId) ?? null;
  }

  /** Reads every case's tracks when the case list or the library changed since the last read. */
  async ensure() {
    const lib = session.library;
    if (!lib) return;
    const key = `${lib.root}|${lib.cases.map((c) => `${c.id}:${c.track_count}:${c.play_time_secs}`).join(",")}`;
    if (key === this.#key) return;
    this.#key = key;
    const seq = ++this.#seq;
    this.loading = true;
    try {
      const entries = await Promise.all(lib.cases.map(async (c) => [c.id, await caseTracks(lib.root, c.id)] as const));
      if (seq !== this.#seq) return;
      this.order = new Map(entries);
    } catch {
      // the next change of the case list tries again
      if (seq === this.#seq) this.#key = "";
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  reset() {
    this.#seq++;
    this.#key = "";
    this.order = new Map();
    this.loading = false;
  }
}

export const caseMembers = new CaseMembers();
session.onReset(() => caseMembers.reset());
