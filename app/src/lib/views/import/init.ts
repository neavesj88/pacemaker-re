// Import area startup: job events, OS file drops and the watch-folder import (picked up by
// shell/startup.ts).
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { onFileDrag } from "$lib/api/import";
import { jobs } from "$lib/state/jobs.svelte";
import { watch } from "$lib/state/watch.svelte";

export default function init(): Unlisten {
  const stopWatch = watch.start();
  const stopEvents = subscribeAll([
    () => onJobProgress((e) => jobs.onProgress(e)),
    () => onJobDone((e) => jobs.onDone(e)),
    () =>
      onFileDrag((s) => {
        if (s.type === "hover") jobs.dragging = true;
        else if (s.type === "leave") jobs.dragging = false;
        else {
          jobs.dragging = false;
          jobs.askAdd(s.paths);
        }
      }),
  ]);
  return () => {
    stopWatch();
    stopEvents();
  };
}
