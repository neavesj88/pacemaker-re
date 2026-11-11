// Device area startup: feeds the shared job events of "device-*" jobs into the device store, and
// the undo history events (an edit was recorded, undone or cleared) into the undo store.
import { onRelinkScanProgress, onUndoChanged } from "$lib/api/device";
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { undo } from "$lib/state/undo.svelte";
import { device } from "./device.svelte";
import { relink } from "./relink.svelte";

export default function init(): Unlisten {
  void undo.loadSettings();
  return subscribeAll([
    () => onJobProgress((e) => device.onProgress(e)),
    () => onJobDone((e) => device.onDone(e)),
    () => onUndoChanged((e) => void undo.onChanged(e)),
    () => onRelinkScanProgress((e) => relink.onScanProgress(e)),
  ]);
}
