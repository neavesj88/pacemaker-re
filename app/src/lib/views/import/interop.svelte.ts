// State of the "other libraries" part of the Import view: the file being previewed, the plan the
// backend computed for it, which parts the user ticked, and the exports. Nothing is written
// until `ask()` is confirmed in the shared confirmation dialog (state/jobs.svelte.ts).
import {
  interopApply,
  interopExport,
  interopPreview,
  pickLibraryFile,
  pickSavePath,
  type ExportFormat,
  type ExportScope,
  type ImportOptions,
  type ImportPlan,
} from "$lib/api/import";
import { baseName } from "$lib/format";
import { jobs } from "$lib/state/jobs.svelte";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";

const stars = (n: number) => (n > 0 ? `${n}★` : "unrated");
const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;

class Interop {
  path = $state("");
  plan = $state<ImportPlan | null>(null);
  loading = $state(false);
  exporting = $state(false);
  error = $state("");
  ratings = $state(true);
  playlists = $state(true);
  /** Source-file indexes of the playlists the user left ticked. */
  chosen = $state<number[]>([]);

  fileName = $derived(baseName(this.path));
  label = $derived(this.plan?.source === "itunes" ? "iTunes / Music" : "rekordbox");

  /** Cases that would be created or extended with the current ticks. */
  caseSteps = $derived(
    this.playlists && this.plan
      ? this.plan.cases.filter((c) => (c.action === "create" || c.action === "extend") && this.chosen.includes(c.index))
      : [],
  );
  ratingSteps = $derived(this.ratings && this.plan ? this.plan.ratings : []);
  changes = $derived(this.ratingSteps.length + this.caseSteps.length);

  private options(): ImportOptions {
    return { ratings: this.ratings, playlists: this.playlists, playlist_indexes: [...this.chosen] };
  }

  /** Pick a library file and preview it. */
  async pick(title: string) {
    this.error = "";
    let path: string | null;
    try {
      path = await pickLibraryFile(title);
    } catch (e) {
      this.error = String(e);
      return;
    }
    if (!path) return;
    this.path = path;
    this.plan = null;
    await this.load(true);
  }

  /** (Re)compute the preview. `fresh` ticks every playlist; otherwise the ticks are kept. */
  async load(fresh = false) {
    const lib = session.library;
    if (!lib || !this.path) return;
    this.loading = true;
    this.error = "";
    try {
      // always ask for the full plan: ticking and unticking is done here, without a round trip
      const plan = await interopPreview(lib.root, this.path, { ratings: true, playlists: true, playlist_indexes: null });
      this.plan = plan;
      const doable = plan.cases.filter((c) => c.action === "create" || c.action === "extend").map((c) => c.index);
      this.chosen = fresh ? doable : this.chosen.filter((i) => doable.includes(i));
    } catch (e) {
      this.error = String(e);
      this.plan = null;
    } finally {
      this.loading = false;
    }
  }

  /** Re-read after the library changed (an add job, or this import). */
  async refresh() {
    if (this.path && this.plan) await this.load(false);
  }

  close() {
    this.path = "";
    this.plan = null;
    this.error = "";
    this.chosen = [];
  }

  toggleCase(index: number) {
    this.chosen = this.chosen.includes(index) ? this.chosen.filter((i) => i !== index) : [...this.chosen, index];
  }

  /** Confirmation naming every change; applying runs as the shared job. */
  ask() {
    const lib = session.library;
    const plan = this.plan;
    if (!lib || !plan || this.loading || this.changes === 0) return;
    const names: string[] = [];
    for (const r of this.ratingSteps) {
      names.push(`Rating of ${r.title || "(untitled)"}${r.artist ? " by " + r.artist : ""}: ${stars(r.from)} to ${stars(r.to)}`);
    }
    for (const c of this.caseSteps) {
      names.push(
        c.action === "create"
          ? `New case "${c.name}" with ${plural(c.tracks.length, "track")}`
          : `Add ${plural(c.tracks.length, "track")} to the existing case "${c.name}"`,
      );
    }
    const steps = (this.ratingSteps.length ? 1 : 0) + this.caseSteps.length;
    const path = this.path;
    const options = this.options();
    jobs.ask({
      kind: "interop",
      root: lib.root,
      count: steps,
      names,
      title: `Import from ${this.label} into ${lib.root} ?`,
      description:
        "Exactly the changes listed here are written: ratings in one step, then each case. Tracks are only ever added to an existing case, never removed, and no audio files are touched.",
      confirmLabel: `Apply ${plural(this.changes, "change")}`,
      runningLabel: "Importing",
      doneLabel: "Import finished",
      start: () => interopApply(lib.root, path, options),
    });
  }

  /** Save a list (or the rekordbox library) for other software. */
  async export(format: ExportFormat, where: ExportScope, name: string) {
    const lib = session.library;
    if (!lib || this.exporting) return;
    const safe = name.replace(/[\\/:*?"<>|]+/g, "_").trim() || "export";
    const [ext, label] =
      format === "m3u8" ? ["m3u8", "M3U8 playlist"] : format === "csv" ? ["csv", "CSV list"] : ["xml", "rekordbox XML"];
    this.exporting = true;
    this.error = "";
    try {
      const dest = await pickSavePath(`${safe}.${ext}`, label, ext);
      if (!dest) return;
      const r = await interopExport(lib.root, format, where, dest);
      // notes name the smart cases written or left out; longer than a plain "Saved", so it stays up longer
      const notes = r.notes ?? [];
      toast.show([`Saved ${plural(r.tracks, "track")} to ${baseName(r.path)}.`, ...notes].join(" "), "ok", notes.length ? 12000 : 4000);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.exporting = false;
    }
  }
}

export const interop = new Interop();
session.onReset(() => interop.close());
