// Import area startup: job events and OS file drops (picked up by shell/startup.ts).
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { onFileDrag } from "$lib/api/import";
import { jobs } from "$lib/state/jobs.svelte";

export default function init(): Unlisten {
  return subscribeAll([
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
}
