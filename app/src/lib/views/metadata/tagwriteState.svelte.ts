// Writing the library's tags into the audio files themselves (optional; nothing here runs unless
// the user asks). `check` reads the files and shows what would change, per file and field;
// `ask` turns the ticked part of that into the write job, behind the shared confirmation that
// names every file. The backend plans again when it writes, with the same code.
import { TAG_JOB_KIND, planTags, tagFormats, writeTags, type TagFormat, type TagItem, type TagPlan } from "$lib/api/metacleanup";
import { trackDetails } from "$lib/api/metadata";
import { jobs } from "$lib/state/jobs.svelte";
import { session } from "$lib/state/session.svelte";
import { FIELD_LABELS, type FieldKey } from "./draft";
import { NUMERIC_TAG_FIELDS, itemsFromDetails, itemsFromPlans, plural, tickKey } from "./cleanupLogic";

export const STATUS_WORDS: Record<string, string> = {
  write: "Will be written",
  same: "Already matches",
  unsupported: "File type not supported",
  missing: "File not on the device",
  error: "Cannot be written",
};

export const fieldLabel = (f: string): string => FIELD_LABELS[f as FieldKey] ?? f;

class TagWriteState {
  plans = $state<TagPlan[] | null>(null);
  /** Changes the user turned off, by `tickKey(track, field)`. */
  unticked = $state<Record<string, boolean>>({});
  checking = $state(false);
  error = $state("");
  formats = $state<TagFormat[]>([]);

  /** What the ticked part of the plans would send to the write job. */
  items = $derived(itemsFromPlans(this.plans ?? [], this.unticked, NUMERIC_TAG_FIELDS));

  constructor() {
    session.onReset(() => this.clear());
  }

  count(status: string): number {
    return (this.plans ?? []).filter((p) => p.status === status).length;
  }

  clear() {
    this.plans = null;
    this.unticked = {};
    this.error = "";
  }

  setTicked(trackId: number, field: string, on: boolean) {
    const k = tickKey(trackId, field);
    if (on) delete this.unticked[k];
    else this.unticked[k] = true;
  }

  async loadFormats() {
    if (this.formats.length) return;
    try {
      this.formats = await tagFormats();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Reads the files of these tracks and works out what writing the library's `fields` would change. */
  async checkTracks(ids: number[], fields: string[]) {
    const root = session.library?.root;
    if (!root) return;
    this.checking = true;
    this.error = "";
    try {
      const details = await trackDetails(root, ids);
      this.plans = await planTags(root, itemsFromDetails(details as unknown as Record<string, unknown>[], fields));
      this.unticked = {};
    } catch (e) {
      this.error = String(e);
      this.plans = null;
    } finally {
      this.checking = false;
    }
  }

  /** Opens the confirmation (every file named); the job starts only when the user confirms. */
  ask() {
    const root = session.library?.root;
    if (!root || !this.plans) return;
    const byId = new Map(this.plans.map((p) => [p.track_id, p]));
    const items: TagItem[] = this.items;
    if (!items.length) return;
    const names = items.map((it) => {
      const p = byId.get(it.track_id)!;
      const fields = p.changes.filter((c) => !this.unticked[tickKey(p.track_id, c.field)]).map((c) => fieldLabel(c.field));
      const notes = p.notes.length ? ` [${p.notes.join("; ")}]` : "";
      return `${p.label}: ${p.file} (${p.format}): ${fields.join(", ")}${notes}`;
    });
    const n = items.length;
    const ok = jobs.ask({
      kind: TAG_JOB_KIND,
      root,
      count: n,
      names,
      title: `Write tags into ${plural(n, "audio file")} on ${root} ?`,
      description:
        "Only the ticked fields of each file's tag are replaced; the audio itself is not touched, and ratings, cover art, Sound Check data and other tag items stay. Each file is rewritten as a copy whose audio bytes and untouched tag items are compared with the original, and only then swapped in, so a failure leaves the original as it was. An ID3v2.2 tag is upgraded to 2.4 (marked in the list). The Pacemaker reads its own library, not these tags: they are for other software. These files are changed:",
      confirmLabel: `Write ${n}`,
      runningLabel: "Writing tags",
      doneLabel: "Tag write finished",
      start: () => writeTags(root, items),
    });
    if (ok) this.clear();
  }
}

export const tagWrite = new TagWriteState();
