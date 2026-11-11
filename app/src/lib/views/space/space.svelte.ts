// The free-space planner's state (device area): the settings (thresholds, target, archive folder),
// the plan the rules produce, which tracks are ticked, and the confirmation of a run.
//
// The plan comes from the backend with the settings as they are on screen, so what is listed is
// what the stored rules would list. Ticks start at the planner's suggestion; once the user ticks
// or unticks a row, later re-plans keep their choices instead of resetting them. The confirmation
// is built by `deviceSpaceRunPreview`, the same code the run repeats before it acts.
import {
  deviceSpacePlan,
  deviceSpaceRunPreview,
  deviceSpaceSettings,
  deviceSpaceSettingsSet,
  type SpaceItem,
  type SpacePlan,
  type SpaceRunPlan,
  type SpaceSettings,
} from "$lib/api/device";
import { session } from "$lib/state/session.svelte";
import { filterItems, keepTicks, runFlags, sortItems, suggestedIds, tickTotals, type RunMode, type SortKey } from "./spaceLogic";

export const DEFAULT_SPACE_SETTINGS: SpaceSettings = {
  rules: {
    never_played: true,
    stale_months: 6,
    low_rating_max: 2,
    unrated_is_low: false,
    duplicates: true,
    min_reasons: 1,
    only_no_case: true,
    only_no_mix: true,
    protect_favourites: true,
    protect_rating_min: 4,
    protect_new_days: 30,
    order: "certain",
  },
  goal_kind: "free",
  free_gb: 10,
  card_gb: 64,
  reserve_percent: 5,
  archive_dir: "",
  include_grids: true,
  require_archive: true,
};

export type Confirm = { mode: RunMode; plan: SpaceRunPlan; ids: number[] };

class SpaceStore {
  settings = $state<SpaceSettings>(structuredClone(DEFAULT_SPACE_SETTINGS));
  settingsLoaded = $state(false);
  settingsWarning = $state<string | null>(null);
  plan = $state<SpacePlan | null>(null);
  /** The target kind the plan on screen was made for. */
  planKind = $state<"free" | "card">("free");
  planning = $state(false);
  error = $state("");
  ticked = $state<Set<number>>(new Set());
  sortKey = $state<SortKey | null>(null);
  sortAsc = $state(true);
  filter = $state("");
  confirm = $state<Confirm | null>(null);
  previewing = $state(false);
  /** The user ticked or unticked a row: a re-plan keeps their ticks instead of re-suggesting. */
  #touched = false;
  #gen = 0;
  #saveTimer: ReturnType<typeof setTimeout> | undefined;
  #planTimer: ReturnType<typeof setTimeout> | undefined;

  items = $derived(this.plan?.items ?? []);
  /** The rows in view: filtered, then sorted. Without a sort key the planner's order. */
  rows = $derived(sortItems(filterItems(this.items, this.filter), this.sortKey, this.sortAsc));
  totals = $derived(tickTotals(this.items, this.ticked));

  clear() {
    clearTimeout(this.#saveTimer);
    clearTimeout(this.#planTimer);
    this.#gen++;
    this.plan = null;
    this.error = "";
    this.ticked = new Set();
    this.#touched = false;
    this.confirm = null;
    this.filter = "";
  }

  async loadSettings() {
    if (this.settingsLoaded) return;
    try {
      const v = await deviceSpaceSettings();
      this.settings = v.settings;
      this.settingsWarning = v.warning;
    } catch (e) {
      this.settingsWarning = String(e);
    }
    this.settingsLoaded = true;
  }

  /** Changes the settings (copy, edit, assign), saves them shortly after and re-plans. */
  update(edit: (s: SpaceSettings) => void) {
    const next = $state.snapshot(this.settings) as SpaceSettings;
    edit(next);
    this.settings = next;
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => void this.save(), 400);
    this.replanSoon();
  }

  /** Saves a pending settings edit now, so the backend reads the same settings the run was confirmed with. */
  async flushSettings() {
    if (this.#saveTimer === undefined) return;
    clearTimeout(this.#saveTimer);
    this.#saveTimer = undefined;
    await this.save();
  }

  async save() {
    try {
      // the fields clamp what is typed, and the backend clamps again on its side
      await deviceSpaceSettingsSet($state.snapshot(this.settings) as SpaceSettings);
    } catch (e) {
      this.error = String(e);
    }
  }

  replanSoon() {
    clearTimeout(this.#planTimer);
    this.#planTimer = setTimeout(() => void this.refresh(), 500);
  }

  /** Makes the plan for the open library with the settings on screen. */
  async refresh() {
    const root = session.library?.root;
    if (!root) return;
    const gen = ++this.#gen;
    this.planning = true;
    this.error = "";
    try {
      const settings = $state.snapshot(this.settings) as SpaceSettings;
      const plan = await deviceSpacePlan(root, settings);
      if (gen !== this.#gen) return; // a newer plan was asked for meanwhile
      this.plan = plan;
      this.planKind = settings.goal_kind;
      this.ticked = this.#touched ? keepTicks(this.ticked, plan.items) : suggestedIds(plan.items);
    } catch (e) {
      if (gen !== this.#gen) return;
      this.plan = null;
      this.ticked = new Set();
      this.error = String(e);
    } finally {
      if (gen === this.#gen) this.planning = false;
    }
  }

  sortBy(key: string) {
    const k = key as SortKey;
    if (this.sortKey === k) this.sortAsc = !this.sortAsc;
    else {
      this.sortKey = k;
      this.sortAsc = k === "label";
    }
  }

  toggle(id: number) {
    const next = new Set(this.ticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this.ticked = next;
    this.#touched = true;
  }

  /** Ticks every row in view. */
  tickShown() {
    this.ticked = new Set([...this.ticked, ...this.rows.map((r) => r.track_id)]);
    this.#touched = true;
  }

  tickNone() {
    this.ticked = new Set();
    this.#touched = true;
  }

  /** Back to what the planner suggests for the target. */
  tickSuggested() {
    this.ticked = suggestedIds(this.items);
    this.#touched = false;
  }

  /** The ticked ids in the planner's order. */
  tickedIds(): number[] {
    return this.items.filter((i: SpaceItem) => this.ticked.has(i.track_id)).map((i) => i.track_id);
  }

  /** Asks the backend what a run of `mode` would do with the ticked tracks and opens the confirmation. */
  async openConfirm(mode: RunMode): Promise<void> {
    const root = session.library?.root;
    if (!root || this.previewing) return;
    const ids = this.tickedIds();
    const flags = runFlags(mode);
    this.previewing = true;
    try {
      const plan = await deviceSpaceRunPreview(root, ids, flags.archive ? this.settings.archive_dir : null, this.settings.include_grids, flags.remove);
      this.confirm = { mode, plan, ids };
    } catch (e) {
      this.error = String(e);
    } finally {
      this.previewing = false;
    }
  }

  closeConfirm() {
    this.confirm = null;
  }

  /** After a run: the card changed, so the library, the plan and the ticks are read again. */
  async afterRun() {
    const root = session.library?.root;
    this.confirm = null;
    this.#touched = false;
    if (root) await session.load(root, true);
    await this.refresh();
  }
}

export const space = new SpaceStore();
session.onReset(() => space.clear());
