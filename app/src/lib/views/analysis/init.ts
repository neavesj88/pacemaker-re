// Analysis area startup (picked up by shell/startup.ts): reads the background analysis settings and
// status, and listens for its status and for the work it finishes.
import { onBgChanged, onBgStatus } from "$lib/api/bganalysis";
import { subscribeAll, type Unlisten } from "$lib/api/events";
import { bg } from "$lib/state/bganalysis.svelte";

export default function init(): Unlisten {
  void bg.load();
  return subscribeAll([() => onBgStatus((s) => bg.onStatus(s)), () => onBgChanged((c) => bg.onChanged(c))]);
}
