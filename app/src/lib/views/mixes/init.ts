// Mix area startup: follows the "mix-export" and "mix-loudness" jobs and the loudness reports. An
// export writes nothing to the device, so it does not go through the shared job runner (which
// asks for a device-write confirmation); the editor shows its own progress and a toast at the end.
import { MIX_EXPORT_JOB, MIX_LOUDNESS_JOB, onLoudnessReport } from "$lib/api/export";
import { onJobDone, onJobProgress, subscribeAll, type Unlisten } from "$lib/api/events";
import { mixExport } from "$lib/state/mixexport.svelte";
import { toast } from "$lib/state/toast.svelte";
import { mv } from "./view.svelte";

export default function init(): Unlisten {
  return subscribeAll([
    () =>
      onJobProgress((e) => {
        if (e.job === MIX_EXPORT_JOB && e.total > 0) {
          mv.exportPct = Math.min(1, e.done / e.total);
          mv.exportPhase = e.status === "measuring" ? "measuring" : "rendering";
        }
        if (e.job === MIX_LOUDNESS_JOB && e.total > 0) mixExport.analysePct = Math.min(1, e.done / e.total);
      }),
    () => onLoudnessReport((r) => mixExport.receive(r)),
    () =>
      onJobDone((e) => {
        if (e.job === MIX_LOUDNESS_JOB) {
          mixExport.analysisEnded(e.error, e.summary);
          return;
        }
        if (e.job !== MIX_EXPORT_JOB) return;
        mv.exporting = false;
        if (e.error) toast.show(`Export failed: ${e.error}`, "bad", 12000);
        else toast.show(e.summary || "Exported the mix", e.failed > 0 || e.ok === 0 ? "info" : "ok", 10000);
      }),
  ]);
}
