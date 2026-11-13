// Tags and colour labels of cases and mixes. Same store file and rules as the labels of tracks
// (commands/tags.rs, state/labels.ts); a case or mix has no energy. App-side only: nothing is
// written to the device, so these work on a read-only card and never need the write gate.
import { SvelteMap } from "svelte/reactivity";
import { applyThingLabels, loadThingLabels, type ThingKind, type ThingLabelsRow } from "$lib/api/tags";
import { isColour, labelsEmpty, tagVocabulary, tracksWithTag, type LabelOp, type TrackLabels } from "./labels";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

const toLabels = (r: ThingLabelsRow): TrackLabels => ({ tags: [...r.tags], colour: isColour(r.colour) ? r.colour : null, energy: null });

class ThingLabels {
  /** Labels by case (or mix) id; ones with none are absent. */
  byId = new SvelteMap<number, TrackLabels>();
  /** The library these labels were read for. */
  root = $state<string | null>(null);
  #seq = 0;

  constructor(readonly kind: ThingKind) {}

  /** Every tag in use with the number of cases (or mixes) that carry it. */
  vocabulary = $derived(tagVocabulary([...this.byId.values()]));

  get(id: number): TrackLabels | undefined {
    return this.byId.get(id);
  }

  /** Ids carrying `tag` (ignoring case). */
  withTag(tag: string): number[] {
    return tracksWithTag(this.byId, tag);
  }

  clear() {
    this.#seq++;
    this.byId.clear();
    this.root = null;
  }

  /** Reads the labels of `root`; a result that arrives after another library opened is dropped. */
  async load(root: string) {
    const seq = ++this.#seq;
    try {
      const v = await loadThingLabels(root, this.kind);
      if (seq !== this.#seq || session.library?.root !== root) return;
      this.byId.clear();
      for (const r of v.things) {
        const l = toLabels(r);
        if (!labelsEmpty(l)) this.byId.set(r.id, l);
      }
      this.root = root;
      if (v.notice) toast.show(v.notice, "bad");
    } catch (e) {
      if (seq === this.#seq) toast.show(`Could not read the ${this.kind} labels: ${e}`, "bad");
    }
  }

  /** Applies the ops to every id (all or nothing). True when saved; a failure is a toast and
   *  leaves everything as it was. */
  async apply(ids: number[], ops: LabelOp[]): Promise<boolean> {
    const root = session.library?.root;
    if (!root || ids.length === 0 || ops.length === 0) return false;
    try {
      const rows = await applyThingLabels(root, this.kind, ids, ops);
      if (session.library?.root !== root) return false;
      for (const r of rows) {
        const l = toLabels(r);
        if (labelsEmpty(l)) this.byId.delete(r.id);
        else this.byId.set(r.id, l);
      }
      return true;
    } catch (e) {
      toast.show(`Could not save the ${this.kind} labels: ${e}`, "bad");
      return false;
    }
  }
}

export const caseLabels = new ThingLabels("case");
export const mixLabels = new ThingLabels("mix");
session.onReset(() => {
  caseLabels.clear();
  mixLabels.clear();
});

/** Which case / mix label editor is open: `thingEditor.edit("case", ids)`. */
class ThingEditor {
  kind = $state<ThingKind>("case");
  ids = $state<number[] | null>(null);

  edit(kind: ThingKind, ids: number[]) {
    this.kind = kind;
    this.ids = ids.length ? [...ids] : null;
  }

  close() {
    this.ids = null;
  }
}

export const thingEditor = new ThingEditor();
