// Analysis jobs started from the UI. Today: key detection ("Detect keys"). The view that offers
// it (a button, a menu entry, the sidebar's need-analysis block) calls `analysis.askDetectKeys()`;
// the confirmation, progress strip and library reload come from the shared job pipeline.
import { detectKeys } from "$lib/api/analysis";
import { baseName } from "$lib/format";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";

class Analysis {
  /** Tracks of the open library that have no key (the ones "Detect keys" fills in). */
  keyless = $derived((session.library?.tracks ?? []).filter((t) => !t.key.trim()));

  /** Confirm detecting keys for `ids` (default: every track without a key). Tracks that already
   *  have a key are never overwritten from here. */
  askDetectKeys(ids?: number[]) {
    const lib = session.library;
    if (!lib) return;
    const wanted = ids ? new Set(ids) : null;
    const targets = this.keyless.filter((t) => !wanted || wanted.has(t.track_id));
    if (targets.length === 0) {
      session.error = "Every one of those tracks already has a key.";
      return;
    }
    const n = targets.length;
    jobs.ask({
      kind: "keys",
      root: lib.root,
      count: n,
      names: targets.map((t) => t.title || baseName(t.location)),
      title: `Detect the key of ${n} track${n === 1 ? "" : "s"} on ${lib.root} ?`,
      description:
        "Each track is decoded and its key worked out from the audio (for example Am or F#m). Only the key field of the library is written, and only for tracks that have none. Keys from audio are a good first guess, not a certainty.",
      confirmLabel: `Detect ${n}`,
      runningLabel: "Detecting keys",
      doneLabel: "Key detection finished",
      start: () => detectKeys(lib.root, targets.map((t) => t.track_id)),
    });
  }
}

export const analysis = new Analysis();
