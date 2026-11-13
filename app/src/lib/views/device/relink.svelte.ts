// State of the Relink dialog (Health card -> missing audio): the folder to search, the matches
// the search found, which of them are ticked and which candidate each row uses. The pure parts
// (wording, totals, ordering) are in relinkLogic.ts. Nothing here writes to the card: the run is
// the "device-relink" job started by run().
import {
  deviceRelinkInspect,
  deviceRelinkRun,
  deviceRelinkScan,
  deviceRelinkScanCancel,
  deviceRelinkSettings,
  deviceRelinkSettingsSet,
  JOB_RELINK,
  pickRelinkFile,
  pickRelinkFolder,
  type RelinkCandidate,
  type RelinkPick,
  type RelinkPlan,
  type RelinkRow,
  type RelinkScanProgress,
  type RelinkSettings,
} from "$lib/api/device-relink";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";
import { device } from "./device.svelte";
import { cardMusicFolder, isPreselected, sortRows, tickAfterBulk, totals } from "./relinkLogic";

export type Stage = "choose" | "scanning" | "review" | "confirm";

/** The state of one row: whether it is ticked, which of its candidates it uses, and the file the
 *  user picked by hand (which replaces the list for that row). */
export type RowSel = { ticked: boolean; idx: number; manual: RelinkCandidate | null };

class RelinkStore {
  settings = $state<RelinkSettings | null>(null);
  settingsWarning = $state<string | null>(null);
  folder = $state("");
  stage = $state<Stage>("choose");
  progress = $state<RelinkScanProgress | null>(null);
  plan = $state<RelinkPlan | null>(null);
  sel = $state<Record<number, RowSel>>({});
  error = $state("");
  /** Reading a hand-picked file, or handing the picks to the job. */
  busy = $state(false);
  /** Which row is waiting for the inspection of a picked file. */
  inspecting = $state<number | null>(null);

  constructor() {
    // a different library: nothing found for the old one applies
    session.onReset(() => this.clear());
  }

  clear() {
    this.stage = "choose";
    this.plan = null;
    this.sel = {};
    this.progress = null;
    this.error = "";
    this.busy = false;
    this.inspecting = null;
  }

  /** Reads the saved options once and keeps them (throws if they cannot be read). */
  async loadSettings() {
    if (this.settings) return;
    const v = await deviceRelinkSettings();
    this.settings = v.settings;
    this.settingsWarning = v.warning;
  }

  /** Opens the dialog at its first step. The settings are read once and kept. */
  async open() {
    this.clear();
    device.relinkOpen = true;
    try {
      await this.loadSettings();
    } catch (e) {
      this.error = String(e);
      return;
    }
    if (!this.folder && this.settings) this.folder = this.settings.last_folder;
  }

  close() {
    if (this.stage === "scanning") void this.cancelSearch();
    device.relinkOpen = false;
  }

  async browse() {
    try {
      const d = await pickRelinkFolder();
      if (d) this.folder = d;
    } catch (e) {
      this.error = String(e);
    }
  }

  useCardMusic(root: string) {
    this.folder = cardMusicFolder(root);
  }

  /** Saves the options (with the folder last searched). Failing to must not stop a search, so it says so and returns. */
  async saveSettings() {
    if (!this.settings) return;
    try {
      const v = await deviceRelinkSettingsSet({ ...this.settings, last_folder: this.folder.trim() || this.settings.last_folder });
      this.settings = v.settings;
      this.settingsWarning = v.warning;
    } catch (e) {
      toast.show(`Could not save the options: ${String(e)}`, "bad", 8000);
    }
  }

  async search(root: string) {
    if (!this.settings || !this.folder.trim()) return;
    this.error = "";
    this.progress = null;
    this.stage = "scanning";
    await this.saveSettings();
    try {
      const plan = await deviceRelinkScan(root, this.folder.trim(), this.settings);
      this.plan = plan;
      const mode = this.settings.preselect;
      const sel: Record<number, RowSel> = {};
      for (const r of plan.rows) sel[r.track_id] = { ticked: isPreselected(r.candidates[0], mode), idx: 0, manual: null };
      this.sel = sel;
      this.stage = "review";
    } catch (e) {
      const msg = String(e);
      // a stopped search is the user's own doing, not an error
      if (!msg.includes("Cancelled")) this.error = msg;
      this.stage = "choose";
    }
  }

  async cancelSearch() {
    try {
      await deviceRelinkScanCancel();
    } catch (e) {
      this.error = String(e);
    }
  }

  onScanProgress(p: RelinkScanProgress) {
    if (this.stage === "scanning") this.progress = p;
  }

  get rows(): RelinkRow[] {
    return this.plan ? sortRows(this.plan.rows) : [];
  }

  /** The candidate a row would use: the hand-picked file, else the chosen one of the list. */
  candidate(row: RelinkRow): RelinkCandidate | undefined {
    const s = this.sel[row.track_id];
    return s?.manual ?? row.candidates[s?.idx ?? 0];
  }

  toggle(id: number) {
    const s = this.sel[id];
    if (s) s.ticked = !s.ticked;
  }

  /** Uses another candidate from the row's list; choosing it ticks the row. */
  choose(id: number, idx: number) {
    const s = this.sel[id];
    if (!s) return;
    s.idx = idx;
    s.manual = null;
    s.ticked = true;
  }

  /** The "Tick ..." buttons only add ticks; "Untick all" clears every row. */
  tickWhere(mode: "none" | "sure" | "likely") {
    for (const r of this.rows) {
      const s = this.sel[r.track_id];
      if (s) s.ticked = tickAfterBulk(s.ticked, this.candidate(r), mode);
    }
  }

  /** Reads a file the user picked for one track and uses it for that row. */
  async pickFile(root: string, row: RelinkRow) {
    if (!this.settings) return;
    this.error = "";
    let path: string | null;
    try {
      path = await pickRelinkFile();
    } catch (e) {
      this.error = String(e);
      return;
    }
    if (!path) return;
    this.inspecting = row.track_id;
    try {
      const c = await deviceRelinkInspect(root, row.track_id, path, this.settings);
      this.sel[row.track_id] = { ticked: true, idx: 0, manual: c };
    } catch (e) {
      // the file cannot be used (not audio, used by another track, ...): say why, keep the row as it was
      toast.show(`${row.label}: ${String(e).replace(/^invalid: /, "")}`, "bad", 9000);
    } finally {
      this.inspecting = null;
    }
  }

  /** Every ticked row with the file it uses. */
  get chosen(): { row: RelinkRow; cand: RelinkCandidate; manual: boolean }[] {
    const out: { row: RelinkRow; cand: RelinkCandidate; manual: boolean }[] = [];
    for (const row of this.rows) {
      const s = this.sel[row.track_id];
      const cand = this.candidate(row);
      if (s?.ticked && cand) out.push({ row, cand, manual: !!s.manual });
    }
    return out;
  }

  get totals() {
    return totals(this.chosen.map((c) => ({ action: c.cand.action, size: c.cand.size })));
  }

  review() {
    if (this.chosen.length > 0) this.stage = "confirm";
  }

  backToList() {
    this.stage = "review";
  }

  /** Hands the ticked matches to the job. The backend checks each one again before it acts. */
  async run(root: string) {
    if (!this.settings || this.busy || this.chosen.length === 0) return;
    this.busy = true;
    const picks: RelinkPick[] = this.chosen.map((c) => ({
      track_id: c.row.track_id,
      path: c.cand.path,
      manual: c.manual,
      expect_location: c.cand.new_location,
    }));
    try {
      device.begin(JOB_RELINK, "Relinking tracks");
      await deviceRelinkRun(root, picks, this.settings);
      device.relinkOpen = false;
      this.clear();
    } catch (e) {
      device.job = null;
      this.error = String(e);
      this.stage = "review";
    } finally {
      this.busy = false;
    }
  }
}

export const relink = new RelinkStore();
