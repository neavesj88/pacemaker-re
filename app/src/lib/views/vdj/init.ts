// VirtualDJ export startup: follows the "virtualdj-export" job. The export writes nothing to the
// device, so it does not go through the shared job runner (which asks for a device-write
// confirmation); the dialog shows its own progress and a toast says how it ended.
import { VDJ_EXPORT_JOB } from "$lib/api/vdj";
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { toast } from "$lib/state/toast.svelte";
import { vdj } from "./vdj.svelte";

export default function init(): Unlisten {
  return subscribeAll([
    () =>
      onJobProgress((e) => {
        if (e.job === VDJ_EXPORT_JOB) vdj.progress(e.done, e.total, e.label);
      }),
    () =>
      onJobDone((e) => {
        if (e.job !== VDJ_EXPORT_JOB) return;
        const bad = !!e.error || e.failed > 0;
        const text = e.error ? `Export failed: ${e.error}` : e.summary || "Exported";
        vdj.finish({ text, bad });
        toast.show(text, e.error ? "bad" : bad ? "info" : "ok", 12000);
      }),
  ]);
}
