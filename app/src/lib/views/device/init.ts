// Device area startup: feeds the shared job events of "device-*" jobs into the device store.
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { device } from "./device.svelte";

export default function init(): Unlisten {
  return subscribeAll([() => onJobProgress((e) => device.onProgress(e)), () => onJobDone((e) => device.onDone(e))]);
}
