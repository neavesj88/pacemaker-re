// Date and rating of each mix for the library list. `mix_list` returns only name, length and
// track count, so the library row (MixMeta) is read per mix with `mix_open`, one at a time in the
// background, and cached here. A request to widen `mix_list` is in the hand-off note; this cache
// then becomes unnecessary.
import { mixOpen, mixSetMeta, type MixMeta } from "$lib/api/mixes";
import { session } from "$lib/state/session.svelte";

class ListMeta {
  byId = $state.raw<Map<number, MixMeta>>(new Map());
  #loading = false;
  #queue: number[] = [];

  /** Stores a row the editor already has (after opening, saving or rating a mix). */
  set(id: number, meta: MixMeta | null) {
    if (!meta) return;
    const m = new Map(this.byId);
    m.set(id, meta);
    this.byId = m;
  }

  forget(id: number) {
    const m = new Map(this.byId);
    m.delete(id);
    this.byId = m;
  }

  /** Reads the rows that are missing, one at a time. A call made while another runs replaces
   * the waiting list, so only the mixes currently listed are read. */
  async fill(ids: number[]) {
    this.#queue = ids.filter((id) => !this.byId.has(id));
    if (this.#loading) return;
    this.#loading = true;
    try {
      while (this.#queue.length > 0) {
        const root = session.library?.root;
        const id = this.#queue.shift()!;
        if (!root || this.byId.has(id)) continue;
        try {
          const doc = await mixOpen(root, id);
          if (session.library?.root === root) this.set(id, doc.meta);
        } catch {
          // an unreadable mix shows dashes in the list; opening it reports why
        }
      }
    } finally {
      this.#loading = false;
    }
  }

  /** Sets the rating of a mix (a library-row write, immediate, like the Editor's star column). */
  async rate(id: number, rating: number) {
    const root = session.library?.root;
    const cur = this.byId.get(id);
    if (!root) return;
    let meta = cur;
    if (!meta) meta = (await mixOpen(root, id)).meta ?? undefined;
    if (!meta) throw new Error("this mix has no library row");
    const next = { ...meta, rating };
    await mixSetMeta(root, id, next);
    this.set(id, next);
  }

  clear() {
    this.#queue = [];
    this.byId = new Map();
  }
}

export const listMeta = new ListMeta();
session.onReset(() => listMeta.clear());
