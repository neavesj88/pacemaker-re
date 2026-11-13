// Duplicate finder startup: follows the fingerprint job (own progress panel in the view) and the
// merge job (progress in the shared job strip; this reloads the groups and the case list when it ends).
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { duplicates } from "$lib/state/duplicates.svelte";

export default function init(): Unlisten {
  return subscribeAll([() => onJobProgress((e) => duplicates.onProgress(e)), () => onJobDone((e) => void duplicates.onDone(e))]);
}
