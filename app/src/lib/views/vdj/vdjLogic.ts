// Pure logic of the VirtualDJ list export dialog: the saved options, the words used for what will
// happen to each track, the summary line, the gate on the Export button and the request. The
// plan itself (names, numbering, what is left out) is computed by the backend, once, for the
// preview and for the export; nothing here recomputes it.
//
// Types only from the api module, so this runs under plain node:
// `node --experimental-strip-types --test src/lib/views/vdj/vdjLogic.test.mjs`.
import type { NameFiles, PlanEntry, VdjOptions, VdjPlan, VdjRequest } from "$lib/api/vdj";

export const DEFAULT_OPTIONS: VdjOptions = { copy_audio: true, name_files: "artist_title", ordered: true, no_duplicates: true, path_base: "" };

const NAME_FILES: NameFiles[] = ["artist_title", "original"];

/** Options from browser storage (or anything): every field checked, anything unusable falls back
 * to its default, so a damaged or older saved value never breaks the dialog. */
export function cleanOptions(raw: unknown): VdjOptions {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const bool = (k: keyof VdjOptions, d: boolean) => (typeof o[k] === "boolean" ? (o[k] as boolean) : d);
  const d = DEFAULT_OPTIONS;
  return {
    copy_audio: bool("copy_audio", d.copy_audio),
    name_files: NAME_FILES.includes(o.name_files as NameFiles) ? (o.name_files as NameFiles) : d.name_files,
    ordered: bool("ordered", d.ordered),
    no_duplicates: bool("no_duplicates", d.no_duplicates),
    path_base: typeof o.path_base === "string" ? o.path_base.slice(0, 500) : d.path_base,
  };
}

/** "1.5 GB", "340 MB", "12 kB", "0 B". Powers of 1024, one decimal under 10 of a unit. */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "kB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${i === 0 || v >= 10 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** What happens to one track, in words (rows never rely on a colour or an icon alone). */
export function entryWords(e: PlanEntry): string {
  switch (e.status) {
    case "copy":
      return `Copied as ${e.file} (${formatBytes(e.size)})`;
    case "reuse":
      return `${e.file} is already in the folder: kept as it is, not copied again`;
    case "reference":
      return "Listed where it is on the Pacemaker, not copied";
    case "left_out":
      return `Left out of the list: ${e.reason || "it cannot be exported"}`;
  }
}

/** The one-line summary under the track list. */
export function planLine(p: VdjPlan): string {
  const listed = p.copy_count + p.reuse_count + p.reference_count;
  const parts: string[] = [];
  if (p.copy_audio) {
    parts.push(`copies ${plural(p.copy_count, "file", "files")} (${formatBytes(p.copy_bytes)})`);
    if (p.reuse_count > 0) parts.push(`keeps ${p.reuse_count} already in the folder`);
  } else {
    parts.push(`points ${plural(p.reference_count, "track", "tracks")} at the Pacemaker, copies nothing`);
  }
  if (p.left_out_count > 0) parts.push(`leaves out ${p.left_out_count}`);
  return `The list holds ${plural(listed, "track", "tracks")}: ${parts.join(", ")}.`;
}

/** The label of the Export button: how much it will write. */
export function startLabel(p: VdjPlan | null): string {
  if (!p) return "Export";
  const listed = p.copy_count + p.reuse_count + p.reference_count;
  return `Export ${plural(listed, "track", "tracks")}`;
}

/** Whether Export may be pressed, and when not, what to tell the user. */
export function startGate(args: { plan: VdjPlan | null; planError: string; planning: boolean; replace: boolean; running: boolean; folder: string }): { ok: boolean; why: string } {
  const { plan, planError, planning, replace, running, folder } = args;
  if (running) return { ok: false, why: "An export is running." };
  if (!folder.trim()) return { ok: false, why: "Choose the folder to export into." };
  if (planError) return { ok: false, why: planError };
  if (planning || !plan) return { ok: false, why: "Working out what will be exported…" };
  if (plan.copy_count + plan.reuse_count + plan.reference_count === 0) return { ok: false, why: "None of these tracks can be exported." };
  if (plan.list_exists && !replace) return { ok: false, why: "Tick the box to replace the existing list file, or change the list name." };
  return { ok: true, why: "" };
}

/** The tracks of the case that are selected (case order kept). */
export function selectedInCase(selected: readonly number[], caseIds: readonly number[]): number[] {
  const s = new Set(selected);
  return [...new Set(caseIds)].filter((id) => s.has(id));
}

/** A first list name from the case's name. */
export function suggestListName(caseName: string): string {
  const n = caseName.replace(/\s+/g, " ").trim();
  return n || "Pacemaker case";
}

export function buildRequest(args: { root: string; caseId: number; onlyTrackIds: number[]; listName: string; destDir: string; options: VdjOptions }): VdjRequest {
  return {
    root: args.root,
    case_id: args.caseId,
    only_track_ids: args.onlyTrackIds,
    list_name: args.listName.trim(),
    dest_dir: args.destDir.trim(),
    options: { ...args.options, path_base: args.options.copy_audio ? args.options.path_base.trim() : "" },
  };
}
