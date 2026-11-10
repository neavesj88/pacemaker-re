// The open library and the detected devices. Everything else reads `session.library`.
import { chooseLibraryFolder, listVolumes, openLibrary } from "$lib/api/library";
import type { LibraryView } from "$lib/api/types";
import { nav } from "./nav.svelte";

const resetHooks: (() => void)[] = [];

class Session {
  volumes = $state<string[]>([]);
  library = $state<LibraryView | null>(null);
  loading = $state(false);
  /** Shown in the red banner under the header. Any area may set it; "" clears it. */
  error = $state("");

  /** Tracks whose .str is missing, empty or unreadable (or never analysed), with the reason. */
  needs = $derived(new Map((this.library?.needs_analysis ?? []).map((n) => [n.track_id, n.reason])));

  /** A store registers here to clear its own state when a different library is opened (not on
   *  an in-place reload). Called at module load by the store itself; session imports no store. */
  onReset(fn: () => void) {
    resetHooks.push(fn);
  }

  async refreshVolumes() {
    // Detected devices are listed, never opened automatically: opening reads every track's .str,
    // and a docked device may have an ageing hard drive. The user chooses when to read it.
    this.volumes = await listVolumes();
  }

  /** Reads the library on `root`. `keepView` re-reads in place (after a job) without resetting
   *  the view, the open case or the filters. */
  async load(root: string, keepView = false) {
    this.loading = true;
    this.error = "";
    try {
      this.library = await openLibrary(root);
      if (!keepView) {
        nav.go("tracks");
        for (const fn of resetHooks) fn();
      }
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  async openFolder() {
    const dir = await chooseLibraryFolder();
    if (dir) await this.load(dir);
  }
}

export const session = new Session();
