// History area startup: snapshot the play counters into the play log each time a library is read.
import type { Unlisten } from "$lib/api/events";
import { playlog } from "$lib/state/playlog.svelte";

export default function init(): Unlisten {
  return playlog.start();
}
