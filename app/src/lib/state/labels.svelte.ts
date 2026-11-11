// The labels (tags, colour, energy) of the open library's tracks. They are app-side data: the
// backend keeps them in a file in the app config dir (see commands/tags.rs), so nothing here
// needs the device to be writable and none of it appears on the Pacemaker. Pure rules are in
// labels.ts; this store holds the loaded state and does the calls.
import { SvelteMap } from "svelte/reactivity";
import { applyTags, deleteTag, forgetUnattachedLabels, loadTags, renameTag, type TagsView, type TrackLabelsRow } from "$lib/api/tags";
import { isColour, labelsEmpty, orphansWithTag, tagVocabulary, tracksWithTag, type LabelOp, type TrackLabels } from "./labels";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

const toLabels = (r: TrackLabelsRow): TrackLabels => ({
  tags: [...r.tags],
  colour: isColour(r.colour) ? r.colour : null,
  energy: r.energy,
});

class Labels {
  /** Labels by track id; tracks with none are absent. */
  byTrack = new SvelteMap<number, TrackLabels>();
  /** The library these labels were read for. */
  root = $state<string | null>(null);
  /** Labels in the file that match no track on this card (removed or added again). */
  orphanLabels = $state.raw<TrackLabels[]>([]);
  orphans = $derived(this.orphanLabels.length);
  /** Set when the labels file could not be read. */
  notice = $state("");
  #seq = 0;

  /** Entries for tracks that are on the card now. */
  live = $derived.by(() => {
    const ids = new Set((session.library?.tracks ?? []).map((t) => t.track_id));
    return [...this.byTrack].filter(([id]) => ids.has(id));
  });

  /** Every tag in use with its number of tracks, by name. */
  vocabulary = $derived(tagVocabulary(this.live.map(([, l]) => l)));

  get(id: number): TrackLabels | undefined {
    return this.byTrack.get(id);
  }

  /** Ids of the tracks (on the card) carrying `tag`. */
  withTag(tag: string): number[] {
    return tracksWithTag(this.live, tag);
  }

  clear() {
    this.#seq++;
    this.byTrack.clear();
    this.root = null;
    this.orphanLabels = [];
    this.notice = "";
  }

  #setView(v: TagsView, root: string) {
    this.byTrack.clear();
    for (const r of v.tracks) {
      const l = toLabels(r);
      if (!labelsEmpty(l)) this.byTrack.set(r.track_id, l);
    }
    this.root = root;
    this.orphanLabels = v.orphan_labels.map((o) => toLabels({ track_id: 0, ...o }));
    this.notice = v.notice ?? "";
  }

  /** Reads the labels of `root`. A result that arrives after another library was opened is dropped. */
  async load(root: string) {
    const seq = ++this.#seq;
    try {
      const v = await loadTags(root);
      if (seq === this.#seq && session.library?.root === root) this.#setView(v, root);
    } catch (e) {
      if (seq === this.#seq) toast.show(`Could not read the tags: ${e}`, "bad");
    }
  }

  /** Applies the ops to the tracks (all or nothing). Returns whether it was saved; a failure is
   *  shown as a toast and leaves everything as it was. */
  async apply(ids: number[], ops: LabelOp[]): Promise<boolean> {
    const root = session.library?.root;
    if (!root || ids.length === 0 || ops.length === 0) return false;
    try {
      const rows = await applyTags(root, ids, ops);
      if (session.library?.root !== root) return false;
      for (const r of rows) {
        const l = toLabels(r);
        if (labelsEmpty(l)) this.byTrack.delete(r.track_id);
        else this.byTrack.set(r.track_id, l);
      }
      return true;
    } catch (e) {
      toast.show(`Could not save the tags: ${e}`, "bad");
      return false;
    }
  }

  /** The labels of removed tracks that carry `tag`. */
  orphansWithTag(tag: string): TrackLabels[] {
    return orphansWithTag(this.orphanLabels, tag);
  }

  /** Removes the labels of tracks that are no longer on the card (the `orphanLabels` shown). */
  async forgetOrphans(): Promise<boolean> {
    return this.#manage((root) => forgetUnattachedLabels(root, this.orphanLabels.length), "forget the labels");
  }

  async rename(from: string, to: string): Promise<boolean> {
    return this.#manage((root) => renameTag(root, from, to), "rename the tag");
  }

  async remove(tag: string): Promise<boolean> {
    return this.#manage((root) => deleteTag(root, tag), "delete the tag");
  }

  async #manage(call: (root: string) => Promise<TagsView>, what: string): Promise<boolean> {
    const root = session.library?.root;
    if (!root) return false;
    try {
      const v = await call(root);
      if (session.library?.root === root) this.#setView(v, root);
      return true;
    } catch (e) {
      toast.show(`Could not ${what}: ${e}`, "bad");
      return false;
    }
  }
}

export const labels = new Labels();
session.onReset(() => labels.clear());

/** Which label dialog is open. Any view opens them: `labelEditor.edit(ids)` for the tag, colour
 *  and energy editor of those tracks, `labelEditor.manage()` for the tag manager. */
class LabelEditor {
  ids = $state<number[] | null>(null);
  managing = $state(false);

  edit(ids: number[]) {
    this.ids = ids.length ? [...ids] : null;
  }

  manage() {
    this.managing = true;
  }

  closeEditor() {
    this.ids = null;
  }

  closeManager() {
    this.managing = false;
  }
}

export const labelEditor = new LabelEditor();
