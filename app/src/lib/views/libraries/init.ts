// Libraries startup: the end of the "device-sync" job reaches this view's store when the run was a
// drag-and-drop copy started here (the Sync view's store ignores that run, and the device store
// shows the strip and the toast for it). Also the "Pacemaker connected" check (state/connect.svelte.ts).
import { onJobDone, subscribeAll, type Unlisten } from "$lib/api/events";
import { connect } from "$lib/state/connect.svelte";
import { libs } from "./libraries.svelte";

export default function init(): Unlisten {
  const stopWatching = connect.start();
  const stopEvents = subscribeAll([() => onJobDone((e) => void libs.onDone(e))]);
  return () => {
    stopWatching();
    stopEvents();
  };
}
