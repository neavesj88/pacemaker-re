// Stems area startup: the model download and the stem run report through the shared job events
// (jobs "stems-download" and "stems"), plus two events of their own with the rich progress and
// the finished result. The shared job runner ignores job names it did not start.
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { onStemsProgress, onStemsResult } from "$lib/api/stems";
import { stems } from "$lib/state/stems.svelte";

export default function init(): Unlisten {
  return subscribeAll([
    () => onJobProgress((e) => stems.onJobProgress(e)),
    () => onJobDone((e) => void stems.onJobDone(e)),
    () => onStemsProgress((e) => stems.onProgress(e)),
    () => onStemsResult((e) => stems.onResult(e)),
  ]);
}
