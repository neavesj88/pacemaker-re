// Mock of the background jobs: one job at a time, one job-progress event per item on a timer,
// then job-done. Mirrors src-tauri/src/jobs.rs. Shared by every area's mock that runs a job.
import { emit } from "@tauri-apps/api/event";

export const jobLock = { running: false };

export function runJob(
  job: string,
  items: string[],
  each: (label: string, i: number) => { bpm?: number; error?: string },
) {
  jobLock.running = true;
  const n = { added: 0, skipped: 0, reanalysed: 0, failed: 0 };
  items.forEach((label, i) =>
    setTimeout(async () => {
      const r = each(label, i);
      const status = r.error ? "failed" : job === "add" ? (/skip/i.test(label) ? "skipped" : "added") : "reanalysed";
      if (status === "failed") n.failed++;
      else if (status === "skipped") n.skipped++;
      else if (status === "added") n.added++;
      else n.reanalysed++;
      await emit("job-progress", {
        job,
        label,
        done: i + 1,
        total: items.length,
        status,
        bpm: status === "skipped" || r.error ? null : (r.bpm ?? 128),
        detail: null,
        error: r.error ?? null,
      });
      if (i === items.length - 1) {
        jobLock.running = false;
        const parts = [
          n.added && `${n.added} added`,
          n.reanalysed && `${n.reanalysed} analysed`,
          n.skipped && `${n.skipped} already in the library`,
          n.failed && `${n.failed} failed`,
        ].filter(Boolean);
        await emit("job-done", {
          job,
          ok: n.added + n.reanalysed + n.skipped,
          failed: n.failed,
          summary: parts.join(", ") || "nothing to do",
          error: null,
        });
      }
    }, 700 * (i + 1)),
  );
}
