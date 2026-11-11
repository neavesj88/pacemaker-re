// Grid area startup: the events of the read-only analysis behind a batch re-analyse preview.
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { gridBatch } from "$lib/state/gridBatch.svelte";

export default function init(): Unlisten {
  return subscribeAll([() => onJobProgress((e) => gridBatch.onProgress(e)), () => onJobDone((e) => void gridBatch.onDone(e))]);
}
