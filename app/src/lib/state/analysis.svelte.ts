// Analysis tools started from the UI: key detection ("Detect keys", with its settings), setting a
// key by hand, and re-analysing tracks. The view that offers them (a menu entry, the inspector,
// the sidebar's need-analysis block) calls `analysis.askDetectKeys(ids)`, `analysis.askSetKey(ids)`
// or `jobs.askReanalyse(ids)`; the dialogs are views/analysis/Overlays.svelte, and the shared job
// pipeline handles the confirmation, progress strip and library reload for the jobs.
import { detectKeys, setKeys } from "$lib/api/analysis";
import {
  canonicalKeyName,
  confidenceFloor,
  detectionPlan,
  detectLine,
  DEFAULT_KEY_SETTINGS,
  KEY_SETTINGS_STORAGE,
  normaliseKeySettings,
  trackLabel,
  type KeySettings,
} from "./keyLogic";
import { jobs } from "./jobs.svelte";
import { loadPref, savePref } from "./prefStore";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

function loadSettings(): KeySettings {
  return loadPref(KEY_SETTINGS_STORAGE, normaliseKeySettings, () => ({ ...DEFAULT_KEY_SETTINGS }));
}

/** Which dialog is open and for which tracks. */
export type KeyDialog = { kind: "detect" | "set"; ids: number[] };

class Analysis {
  /** Tracks of the open library that have no key (the ones "Detect keys" fills in). */
  keyless = $derived((session.library?.tracks ?? []).filter((t) => !t.key.trim()));

  /** How key detection treats existing keys and weak results; remembered between runs. */
  settings = $state<KeySettings>(loadSettings());

  dialog = $state<KeyDialog | null>(null);

  saveSettings() {
    this.settings = normaliseKeySettings($state.snapshot(this.settings));
    savePref(KEY_SETTINGS_STORAGE, this.settings);
  }

  /** The tracks of the open library with these ids, in library order. */
  tracksOf(ids: number[]) {
    const wanted = new Set(ids);
    return (session.library?.tracks ?? []).filter((t) => wanted.has(t.track_id));
  }

  /** Opens the detection dialog for `ids` (default: every track without a key), where the two
   *  settings are shown and changed before the usual confirmation. */
  askDetectKeys(ids?: number[]) {
    if (!session.library) return;
    const list = ids ? this.tracksOf(ids) : this.keyless;
    if (list.length === 0) {
      session.error = "There are no tracks to detect a key for.";
      return;
    }
    this.dialog = { kind: "detect", ids: list.map((t) => t.track_id) };
  }

  /** Opens the dialog that sets one key on all of `ids` by hand. */
  askSetKey(ids: number[]) {
    if (!session.library) return;
    const list = this.tracksOf(ids);
    if (list.length === 0) return;
    this.dialog = { kind: "set", ids: list.map((t) => t.track_id) };
  }

  closeDialog() {
    this.dialog = null;
  }

  /** Step two of detection: closes the dialog and asks the usual write confirmation, listing
   *  every track the run will analyse and what happens to its key. */
  continueDetect() {
    const d = this.dialog;
    const lib = session.library;
    if (!d || d.kind !== "detect" || !lib) return;
    this.saveSettings();
    const s = this.settings;
    const { targets, kept } = detectionPlan(this.tracksOf(d.ids), s);
    this.dialog = null;
    if (targets.length === 0) {
      session.error = "Every one of those tracks already has a key, and replacing keys is switched off.";
      return;
    }
    const n = targets.length;
    const floor = confidenceFloor(s);
    jobs.ask({
      kind: "keys",
      root: lib.root,
      count: n,
      names: targets.map((t) => detectLine(t, s)),
      title: `Detect the key of ${n} track${n === 1 ? "" : "s"} on ${lib.root} ?`,
      description:
        "Each track is decoded and its key worked out from the audio (for example Am or F#m). Only the key field of the library is written. Keys from audio are a good first guess, not a certainty." +
        (kept.length > 0 ? ` ${kept.length} track${kept.length === 1 ? " already has a key and keeps" : "s already have a key and keep"} it.` : "") +
        (s.overwrite ? " Keys already set, including ones set by hand, are replaced." : ""),
      confirmLabel: `Detect ${n}`,
      runningLabel: "Detecting keys",
      doneLabel: "Key detection finished",
      start: () =>
        detectKeys(
          lib.root,
          targets.map((t) => t.track_id),
          s.overwrite,
          floor,
        ),
    });
  }

  /** Writes `keyText` (any musical key) to every track in `ids`, in one transaction, and shows
   *  the new key in the open tables at once. Returns false and says why when it did not save. */
  async setKey(ids: number[], keyText: string): Promise<boolean> {
    const lib = session.library;
    const name = canonicalKeyName(keyText);
    if (!lib || !name) {
      toast.show(`"${keyText.trim()}" is not a musical key (for example Am, F#m, Bb or C).`, "bad");
      return false;
    }
    if (!jobs.canWrite) {
      toast.show("Keys can't be saved while a job runs or the library is read-only.", "bad");
      return false;
    }
    const tracks = this.tracksOf(ids);
    try {
      const changed = await setKeys(
        lib.root,
        tracks.map((t) => ({ track_id: t.track_id, key: name })),
      );
      session.patchTracks(tracks.map((t) => t.track_id), { key: name });
      const n = tracks.length;
      toast.show(
        changed === 0
          ? `${n === 1 ? trackLabel(tracks[0]) : `${n} tracks`} already had the key ${name}`
          : `Key ${name} set on ${changed} track${changed === 1 ? "" : "s"}${changed < n ? ` (${n - changed} already had it)` : ""}`,
        "ok",
      );
      return true;
    } catch (e) {
      toast.show(`Could not save the key: ${e}`, "bad");
      return false;
    }
  }
}

export const analysis = new Analysis();
