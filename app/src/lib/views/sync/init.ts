// Sync startup: the end of the "device-sync" job reaches the Sync store (the device store shows
// the progress strip and the toast for the same job; this one keeps the result on the Sync view and
// reads the open library again when it was the target).
import { onJobDone, subscribeAll, type Unlisten } from "$lib/api/events";
import { sync } from "./sync.svelte";

export default function init(): Unlisten {
  return subscribeAll([() => onJobDone((e) => void sync.onDone(e))]);
}
