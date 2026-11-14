// Watches the open library for an in-place reload (a snapshot restore, a Sync, a relink all read the library
// again without closing the views) and asks the mix editor to check that its mix is still the one in the
// library. The decision is `openMixVerdict` (mixlist.ts); the actions are in flow.ts.
import { untrack } from "svelte";
import { session } from "$lib/state/session.svelte";
import { checkOpenMixAfterReload } from "./flow";

export function watchLibraryReloads(): () => void {
  return $effect.root(() => {
    let seen: unknown = undefined;
    $effect(() => {
      const lib = session.library; // a new object for every read, whatever the reason
      const first = seen === undefined;
      if (lib === seen) return;
      seen = lib;
      if (first || !lib) return;
      untrack(() => void checkOpenMixAfterReload(lib.root));
    });
  });
}
