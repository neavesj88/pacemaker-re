// Cases area startup: follows the "cases-export" job. Exporting writes only into the folder the
// user chose, never to the device, so it does not go through the shared job runner (which asks
// for a device-write confirmation); the export dialog shows its own progress and result.
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { caseExport } from "$lib/state/caseExport.svelte";

export default function init(): Unlisten {
  return subscribeAll([() => onJobProgress((e) => caseExport.onProgress(e)), () => onJobDone((e) => caseExport.onDone(e))]);
}
