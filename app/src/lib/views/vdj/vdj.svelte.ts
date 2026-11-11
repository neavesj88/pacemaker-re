// The VirtualDJ list export's open request, the user's saved options and the state of the running
// export. Anything can ask for the dialog with `vdj.openCase(id, name)` (the Cases view has a
// button); Overlays.svelte shows it. Options and the last folder are a saved user setting
// (browser storage of the app window, like the setlist's); the export itself is a backend job
// whose events `init.ts` feeds into `progress` / `finish` here.
import { type VdjOptions } from "$lib/api/vdj";
import { cleanOptions } from "./vdjLogic";

const STORAGE_KEY = "pacemaker-revival.vdj.v1";

function load(): { options: VdjOptions; dest: string } {
  try {
    const s = localStorage.getItem(STORAGE_KEY);
    if (s) {
      const o = JSON.parse(s) as { options?: unknown; dest?: unknown };
      return { options: cleanOptions(o.options), dest: typeof o.dest === "string" ? o.dest.slice(0, 1000) : "" };
    }
  } catch {
    // blocked or damaged storage: the defaults are used
  }
  return { options: cleanOptions(undefined), dest: "" };
}

export type Result = { text: string; bad: boolean };

const saved = load();

class VdjUi {
  request = $state<{ caseId: number; name: string } | null>(null);

  options = $state<VdjOptions>(saved.options);
  /** The folder to export into, as last typed or picked. */
  dest = $state(saved.dest);

  running = $state(false);
  /** 0 to 1 while running. */
  pct = $state(0);
  current = $state("");
  result = $state<Result | null>(null);
  /** Counts finished exports, so the dialog re-plans (the list and the copies now exist). */
  finished = $state(0);

  openCase(caseId: number, name: string) {
    this.request = { caseId, name };
  }
  close() {
    this.request = null;
  }

  begin() {
    this.running = true;
    this.pct = 0;
    this.current = "";
    this.result = null;
  }

  progress(done: number, total: number, label: string) {
    this.pct = total > 0 ? Math.min(1, done / total) : 0;
    this.current = label;
  }

  finish(result: Result) {
    this.running = false;
    this.pct = 0;
    this.current = "";
    this.result = result;
    this.finished += 1;
  }

  /** Remembers the options and the folder for the next time. */
  persist() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ options: $state.snapshot(this.options), dest: this.dest }));
    } catch {
      // storage blocked: the choices last until the window closes
    }
  }
}

export const vdj = new VdjUi();
