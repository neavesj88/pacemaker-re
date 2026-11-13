// Libraries startup: the end of the "device-sync" job reaches this view's store when the run was a
// drag-and-drop copy started here (the Sync view's store ignores that run, and the device store
// shows the strip and the toast for it).
import { onJobDone, subscribeAll, type Unlisten } from "$lib/api/events";
import { libs } from "./libraries.svelte";

export default function init(): Unlisten {
  return subscribeAll([() => onJobDone((e) => void libs.onDone(e))]);
}
