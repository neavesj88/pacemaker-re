// The export items of the case menus (a case's context menu, the case page, the smart case page):
// the formats with the one used last first, then "Export with options…" which opens the folder
// dialog (copy the audio, relative paths, what to do with files already there).
import type { CaseExportFormat } from "$lib/api/cases";
import { caseExport } from "$lib/state/caseExport.svelte";
import type { MenuItem } from "$lib/ui/ContextMenu.svelte";

export const FORMATS: CaseExportFormat[] = ["m3u8", "csv", "rekordbox"];

export const PLAIN_LABELS: Record<CaseExportFormat, string> = { m3u8: "M3U8 playlist…", csv: "CSV list…", rekordbox: "rekordbox XML…" };

export function exportMenuItems(opts: {
  run: (format: CaseExportFormat) => void;
  options: () => void;
  disabled?: boolean;
  labels?: Record<CaseExportFormat, string>;
}): MenuItem[] {
  const labels = opts.labels ?? PLAIN_LABELS;
  const usual = caseExport.saved.format;
  const order = [usual, ...FORMATS.filter((f) => f !== usual)];
  return [
    ...order.map((f) => ({ label: labels[f] + (f === usual && caseExport.saved.folder ? "  (last used)" : ""), onclick: () => opts.run(f), disabled: opts.disabled })),
    { label: "Export with options…", onclick: opts.options },
  ];
}
