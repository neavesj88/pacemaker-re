// The track matcher: the request list, its matches against the open library, and the user's
// decisions on them. The matching is the pure module in views/matcher/matchLogic.ts; this store
// only holds state, reads files through lib/api/matcher.ts and writes a case through the cases
// store. Nothing here touches the device except `saveOwnedAsCase` (one new case, one quick write).
import { pickMissingSavePath, pickRequestFile, readRequestList, saveMissingList } from "$lib/api/matcher";
import type { LibraryView, Track } from "$lib/api/types";
import { baseName } from "$lib/format";
import {
  type Choice,
  type Counts,
  type Index,
  type MatchOptions,
  type MatchRow,
  type Resolved,
  buildIndex,
  caseTrackIds,
  cleanOptions,
  countStatuses,
  matchItem,
  missingAsCsv,
  missingAsText,
  missingList,
  ownedList,
  resolve,
} from "./matchLogic";
import { type LineOrder, type ParsedList, type RequestItem, parseRequestList } from "./parseList";
import { cases } from "$lib/state/cases.svelte";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";
import { loadPref, savePref } from "$lib/state/prefStore";

const STORAGE_KEY = "pacemaker-revival.matcher.v1";

type Stored = { options: MatchOptions; lineOrder: LineOrder; includeUndecided: boolean };

function stored(raw: unknown): Stored {
  const o = (raw ?? {}) as Record<string, unknown>;
  return {
    options: cleanOptions(o.options),
    lineOrder: o.lineOrder === "title-artist" ? "title-artist" : "artist-title",
    includeUndecided: o.includeUndecided === true,
  };
}

const loadStored = (): Stored => loadPref(STORAGE_KEY, stored, () => stored(null));

/** One request with its match and the user's decision, as the table shows it. */
export type Line = { item: RequestItem; row: MatchRow; choice: Choice | undefined; resolved: Resolved };

class Matcher {
  text = $state("");
  fileName = $state("");
  parsed = $state<ParsedList | null>(null);
  /** The text `parsed` was made from; the text box differing from it means unapplied edits. */
  parsedText = $state("");
  rows = $state<MatchRow[]>([]);
  /** The user's decisions by request id. */
  choices = $state<Record<number, Choice>>({});

  options = $state<MatchOptions>(loadStored().options);
  lineOrder = $state<LineOrder>(loadStored().lineOrder);
  /** Whether the missing list also holds the lines still waiting for a decision. */
  includeUndecided = $state(loadStored().includeUndecided);

  running = $state(false);
  progress = $state(0);
  busy = $state(false);
  error = $state("");
  /** The library the rows were matched against; a different one means the rows are out of date. */
  matchedFor = $state.raw<LibraryView | null>(null);

  #index: Index | null = null;
  #indexFor: readonly Track[] | null = null;
  #token = 0;

  items = $derived(this.parsed?.items ?? []);

  lines = $derived.by<Line[]>(() => {
    if (this.rows.length !== this.items.length) return [];
    return this.items.map((item, i) => {
      const choice = this.choices[item.id];
      return { item, row: this.rows[i], choice, resolved: resolve(this.rows[i], choice) };
    });
  });

  counts = $derived<Counts>(countStatuses(this.rows, this.choices));
  owned = $derived(ownedList(this.items, this.rows, this.choices));
  caseIds = $derived(caseTrackIds(this.owned));
  missing = $derived(missingList(this.items, this.rows, this.choices, this.includeUndecided));
  /** Lines the missing list leaves out only because they have no decision yet. */
  undecided = $derived(this.counts.review);

  trackById = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));

  stale = $derived(this.rows.length > 0 && this.matchedFor !== null && this.matchedFor !== session.library);

  #persist() {
    savePref(STORAGE_KEY, { options: this.options, lineOrder: this.lineOrder, includeUndecided: this.includeUndecided });
  }

  /** Changes matching settings. Matches are worked out again (decisions are kept). */
  setOptions(patch: Partial<MatchOptions>) {
    this.options = cleanOptions({ ...this.options, ...patch });
    this.#persist();
    if (this.rows.length) void this.run();
  }

  setIncludeUndecided(v: boolean) {
    this.includeUndecided = v;
    this.#persist();
  }

  setLineOrder(order: LineOrder) {
    this.lineOrder = order;
    this.#persist();
    if (this.text.trim()) void this.match();
  }

  /** Replaces the request text (pasted or loaded) and clears everything worked out from the old one. */
  setText(text: string, fileName = "") {
    this.text = text;
    this.fileName = fileName;
    this.#token++;
    this.parsed = null;
    this.parsedText = "";
    this.rows = [];
    this.choices = {};
    this.matchedFor = null;
    this.running = false;
    this.error = "";
  }

  clear() {
    this.setText("");
  }

  /** Opens the file dialog and reads the list, then matches it. */
  async openFile() {
    this.error = "";
    let path: string | null;
    try {
      path = await pickRequestFile();
    } catch (e) {
      this.error = String(e);
      return;
    }
    if (!path) return;
    this.busy = true;
    try {
      const file = await readRequestList(path);
      this.setText(file.text, baseName(path));
      await this.match();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.busy = false;
    }
  }

  /** Parses the text and matches it against the library. */
  async match() {
    this.error = "";
    this.#token++;
    this.choices = {};
    this.rows = [];
    this.parsed = parseRequestList(this.text, { lineOrder: this.lineOrder });
    this.parsedText = this.text;
    if (this.parsed.error) {
      this.error = this.parsed.error;
      // an older run was aborted by the token bump above and will not reset this itself
      this.running = false;
      return;
    }
    await this.run();
  }

  /** Matches the parsed requests against the open library, in slices so the window stays alive.
   * Decisions already made are kept (this is also how a changed library or setting is applied). */
  async run() {
    const lib = session.library;
    const items = this.parsed?.items ?? [];
    if (!lib || !items.length) {
      this.#token++;
      this.rows = [];
      this.running = false;
      return;
    }
    const token = ++this.#token;
    this.running = true;
    this.progress = 0;
    if (this.#indexFor !== lib.tracks || !this.#index) {
      this.#index = buildIndex(lib.tracks);
      this.#indexFor = lib.tracks;
    }
    const index = this.#index;
    const opts = { ...this.options };
    const rows: MatchRow[] = [];
    for (let i = 0; i < items.length; i++) {
      rows.push(matchItem(index, items[i], opts));
      if (i % 25 === 24) {
        this.progress = (i + 1) / items.length;
        await new Promise((r) => setTimeout(r, 0));
        if (token !== this.#token) return;
      }
    }
    // a decision on a track that has left the library no longer stands
    const kept: Record<number, Choice> = {};
    const present = new Set(lib.tracks.map((t) => t.track_id));
    for (const [id, c] of Object.entries(this.choices)) {
      if (c.kind === "missing" || present.has(c.trackId)) kept[Number(id)] = c;
    }
    this.choices = kept;
    this.rows = rows;
    this.matchedFor = lib;
    this.running = false;
    this.progress = 1;
  }

  choose(itemId: number, choice: Choice) {
    this.choices = { ...this.choices, [itemId]: choice };
  }

  /** Takes back a decision: the row goes back to what the matcher concluded. */
  reset(itemId: number) {
    const { [itemId]: _gone, ...rest } = this.choices;
    this.choices = rest;
  }

  trackLabel(id: number | null): string {
    if (id === null) return "";
    const t = this.trackById.get(id);
    if (!t) return `(track ${id} is no longer in the library)`;
    return `${t.artist || "(no artist)"} - ${t.title || "(untitled)"}`;
  }

  /** Writes the owned tracks as one new case, in request order. The caller has shown the user
   * every track and the name first. Returns the new case id, or undefined if it was not written. */
  async saveOwnedAsCase(name: string): Promise<number | undefined> {
    const ids = this.caseIds;
    if (!ids.length) return undefined;
    const id = await cases.create(name, ids);
    if (id !== undefined) toast.show(`Saved case “${name}” with ${ids.length} track${ids.length === 1 ? "" : "s"}`, "ok");
    return id;
  }

  /** The missing list as text, exactly what the screen's Missing filter shows. */
  missingText = $derived(missingAsText(this.missing));

  async copyMissing(): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(this.missingText);
      toast.show(`Copied ${this.missing.length} missing track${this.missing.length === 1 ? "" : "s"}`, "ok");
      return true;
    } catch {
      return false;
    }
  }

  /** Saves the missing list as .txt or .csv through the save dialog. */
  async saveMissing(ext: "txt" | "csv") {
    const root = session.library?.root;
    if (!root || !this.missing.length) return;
    // a file name must not carry characters the save dialog or the file system refuses
    const stem = this.fileName.replace(/\.[^.]+$/, "").replace(/[<>:"/\\|?*]+/g, " ").trim() || "request list";
    try {
      const dest = await pickMissingSavePath(`Missing from ${stem}.${ext}`, ext);
      if (!dest) return;
      const text = ext === "csv" ? missingAsCsv(this.missing) : this.missingText;
      const saved = await saveMissingList(root, dest, text);
      toast.show(`Saved ${this.missing.length} missing track${this.missing.length === 1 ? "" : "s"} to ${baseName(saved.path)}`, "ok");
    } catch (e) {
      session.error = String(e);
    }
  }
}

export const matcher = new Matcher();
session.onReset(() => matcher.clear());
