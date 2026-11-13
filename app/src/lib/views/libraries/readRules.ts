// How the right pane's library is read, as plain functions over a host (the Libraries store), so
// the rules run under plain node in `npm test` (readRules.test.mjs) with a stubbed reader:
//
// - `invalidateReads` forgets what was read (a job wrote, a rescan, a different open library,
//   coming back to the view) and reads the library on the right again at once only when
//   `readsAtOnce` says so: a library on this computer or a folder opened by hand, never a detected
//   Pacemaker, which waits for its Read button.
// - `readRight` lists one library. While a read is running a further request is not run beside it
//   and not lost: the latest one is kept (`again`) and run once when the read in flight ends, if
//   the library on the right is still that one.
import { rootKey, sameRoot } from "../../state/place.ts";
import type { PaneData } from "./librariesLogic.ts";

export type ReadHost = {
  /** The library on the right ("" = none chosen). */
  rightRoot: string;
  /** The root being read now, "" when none. */
  reading: string;
  readError: string;
  /** What was read for the right pane, by `rootKey`. Replaced, never changed in place. */
  loaded: Map<string, PaneData>;
  /** A read asked for while another was running ("" = none). */
  again: string;
  /** Takes the right pane's selection back to empty. */
  clearRightSelection(): void;
};

export type ReadDeps = {
  /** Lists the library at `root` (music.db only). */
  fetchPane(root: string): Promise<PaneData>;
  /** Whether a library is read as soon as it is chosen or invalidated (see `readsAtOnce`). */
  readsAtOnce(root: string): boolean;
};

export function invalidateReads(host: ReadHost, deps: ReadDeps, keepSelection = false): void {
  host.loaded = new Map();
  host.readError = "";
  if (!keepSelection) host.clearRightSelection();
  if (host.rightRoot && deps.readsAtOnce(host.rightRoot)) void readRight(host, deps, host.rightRoot);
}

export async function readRight(host: ReadHost, deps: ReadDeps, root: string): Promise<void> {
  if (host.reading) {
    host.again = root; // read once more when the running read ends, so a refresh is not lost
    return;
  }
  host.reading = root;
  host.readError = "";
  try {
    const p = await deps.fetchPane(root);
    const next = new Map(host.loaded);
    next.set(rootKey(root), p);
    host.loaded = next;
  } catch (e) {
    host.readError = String(e);
  } finally {
    host.reading = "";
    const again = host.again;
    host.again = "";
    if (again && host.rightRoot && sameRoot(again, host.rightRoot)) void readRight(host, deps, again);
  }
}
