// Free-space planner startup: when its archive / removal job ends, the card has changed, so the
// library and the plan are read again. (The device area's own init.ts feeds the same job's
// progress and result line into the device store.)
import { JOB_SPACE } from "$lib/api/device";
import { onJobDone, subscribeAll, type Unlisten } from "$lib/api/events";
import { space } from "./space.svelte";

export default function init(): Unlisten {
  return subscribeAll([
    () =>
      onJobDone((e) => {
        if (e.job === JOB_SPACE) void space.afterRun();
      }),
  ]);
}
