// Library-row facts (date, rating, genre, ...) of each mix for the library list, its sort and its
// search. `mix_list` returns only name, length and track count; `mix_list_meta` returns the rows
// of every mix in one query, and they are cached here. The open editor and the rating stars
// write through `update`, which keeps the cache current.
import { mixListMeta, mixOpen, mixSetMeta, type MixMeta } from "$lib/api/mixes";
import { session } from "$lib/state/session.svelte";

class ListMeta {
  byId = $state.raw<Map<number, MixMeta>>(new Map());

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

  /** Reads the rows of every mix again (one query) and replaces the cache. Runs after any write
   * that is in flight, so a rating being saved is never overwritten by an older read. */
  refresh(): Promise<void> {
    const run = async () => {
      const root = session.library?.root;
      if (!root) return;
      try {
        const rows = await mixListMeta(root);
        if (session.library?.root === root) this.byId = new Map(rows.map((r) => [r.id, r.meta]));
      } catch {
        // the list shows dashes for the missing facts; opening a mix reports why
      }
    };
    const p = this.#chain.then(run, run);
    this.#chain = p.catch(() => undefined);
    return p;
  }

  /** Reads the rows if any of `ids` is not cached yet. */
  async fill(ids: number[]) {
    if (ids.some((id) => !this.byId.has(id))) await this.refresh();
  }

  /** Takes the date and play time a save stamped on the open mix; the rest of the cached row
   * (rating, genre, ...) is kept, because the open document's copy of it can be older. */
  afterSave(id: number, saved: MixMeta | null) {
    const cur = this.byId.get(id);
    if (!saved) return this.forget(id);
    this.set(id, cur ? { ...cur, dateModified: saved.dateModified, playTimeSecs: saved.playTimeSecs } : saved);
  }

  #chain: Promise<unknown> = Promise.resolve();

  /** Changes fields of a mix's library row (an immediate write, like the Editor's star column).
   * The row is read again first so a stale copy never overwrites other fields, and again after
   * the write so the cache holds the new modified date. Writes run one after another. */
  update(id: number, change: Partial<MixMeta>): Promise<MixMeta> {
    const run = async () => {
      const root = session.library?.root;
      if (!root) throw new Error("no library is open");
      const fresh = (await mixOpen(root, id)).meta;
      if (!fresh) throw new Error("this mix has no library row");
      await mixSetMeta(root, id, { ...fresh, ...change });
      const after = (await mixOpen(root, id)).meta ?? { ...fresh, ...change };
      if (session.library?.root === root) this.set(id, after);
      return after;
    };
    const p = this.#chain.then(run, run);
    this.#chain = p.catch(() => undefined);
    return p;
  }

  /** Sets the rating of a mix. */
  async rate(id: number, rating: number) {
    await this.update(id, { rating });
  }

  clear() {
    this.byId = new Map();
  }
}

export const listMeta = new ListMeta();
session.onReset(() => listMeta.clear());
