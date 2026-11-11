// Pure logic of exporting cases to a folder: the sentences that say what will happen to each file,
// the totals the confirm button shows, the first selection, and which choices are remembered.
// Types only are imported, so `exportLogic.test.mjs` runs it with plain node.
import type { CaseInfo, ExportAction, ExportAudioEntry, ExportListEntry, ExportPlan, ExportPrefs, ExportRequest, SmartCase } from "$lib/api/cases";

export const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;

/** 1.2 GB, 340 MB, 12 KB, 0 B (decimal units, like the file manager shows). */
export function fmtBytes(n: number): string {
  if (n < 1000) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n;
  let i = -1;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/** The folder part of a path (either separator), or "" when there is none. */
export function dirName(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (i < 0) return "";
  // keep the root: "C:\" and "/" are folders
  if (i === 0) return path.slice(0, 1);
  if (i === 2 && path[1] === ":") return path.slice(0, 3);
  return path.slice(0, i);
}

/** `folder` and `name` joined with the separator the folder already uses. */
export function joinPath(folder: string, name: string): string {
  if (!folder) return name;
  const sep = folder.includes("\\") && !folder.includes("/") ? "\\" : folder.includes("/") && !folder.includes("\\") ? "/" : "\\";
  return folder.endsWith("/") || folder.endsWith("\\") ? folder + name : folder + sep + name;
}

export const EXT: Record<ExportPrefs["format"], string> = { m3u8: "m3u8", csv: "csv", rekordbox: "xml" };
export const FORMAT_LABEL: Record<ExportPrefs["format"], string> = { m3u8: "M3U8 playlist", csv: "CSV list", rekordbox: "rekordbox XML" };

/** The name a save dialog suggests: the remembered folder (if any) and a file name made safe. */
export function saveSuggestion(prefs: Pick<ExportPrefs, "folder">, name: string, ext: string, fallback = "case"): string {
  const safe = name.replace(/[\\/:*?"<>|]+/g, "_").trim() || fallback;
  return joinPath(prefs.folder ?? "", `${safe}.${ext}`);
}

/** The sentence under a file in the preview: what will happen to it. */
export function actionText(action: ExportAction, kind: "list" | "audio", wanted: string | null = null, problem: string | null = null): string {
  switch (action) {
    case "write":
      return "New file";
    case "replace":
      return "Replaces the file that is there";
    case "renamed":
      return `A file with this name is there and is kept; saved as this instead${wanted ? ` (wanted “${wanted}”)` : ""}`;
    case "skip":
      return kind === "list" ? "A file with this name is there: left as it is, not written" : "A different file with this name is there: left as it is, not copied (the playlist keeps pointing at the original)";
    case "same":
      return "Already there, same size and same start, middle and end: left as it is";
    case "missing":
      return `Cannot be copied${problem ? `: ${problem}` : ""}`;
    case "blocked":
      return `Not written${problem ? `: ${problem}` : ": a folder with this name is in the way"}`;
  }
}

/** Whether the action puts a file on disk. */
export const writes = (a: ExportAction) => a === "write" || a === "replace" || a === "renamed";

export type Totals = {
  lists: number;
  audio: number;
  bytes: number;
  replaced: number;
  renamed: number;
  kept: number;
  same: number;
  problems: number;
};

export function totals(plan: ExportPlan): Totals {
  const t: Totals = { lists: 0, audio: 0, bytes: 0, replaced: 0, renamed: 0, kept: 0, same: 0, problems: 0 };
  for (const l of plan.lists) {
    if (writes(l.action)) t.lists++;
    if (l.action === "replace") t.replaced++;
    if (l.action === "renamed") t.renamed++;
    if (l.action === "skip") t.kept++;
    if (l.action === "blocked") t.problems++;
  }
  for (const a of plan.audio) {
    if (writes(a.action)) {
      t.audio++;
      t.bytes += a.size;
    }
    if (a.action === "replace") t.replaced++;
    if (a.action === "renamed") t.renamed++;
    if (a.action === "skip") t.kept++;
    if (a.action === "same") t.same++;
    if (a.action === "missing" || a.action === "blocked") t.problems++;
  }
  return t;
}

/** The text of the confirm button: exactly what it will do. */
export function confirmLabel(plan: ExportPlan): string {
  const t = totals(plan);
  if (t.lists === 0 && t.audio === 0) return "Nothing to write";
  const parts: string[] = [];
  if (t.lists > 0) parts.push(`write ${plural(t.lists, "playlist")}`);
  if (t.audio > 0) parts.push(`copy ${plural(t.audio, "audio file")} (${fmtBytes(t.bytes)})`);
  let s = parts.join(" and ");
  // the button itself says when it writes over files, not only the red list above it
  if (t.replaced > 0) s += `, replacing ${plural(t.replaced, "existing file")}`;
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Every existing file the export would write over, named: shown in red above the button. */
export function replaceList(plan: ExportPlan): string[] {
  return [...plan.lists.filter((l) => l.action === "replace").map((l) => l.file), ...plan.audio.filter((a) => a.action === "replace").map((a) => a.file)];
}

/** A one-line summary of the plan's totals beyond what the button says. */
export function planSummary(plan: ExportPlan): string {
  const t = totals(plan);
  const parts: string[] = [];
  if (t.replaced > 0) parts.push(`${plural(t.replaced, "existing file")} will be replaced`);
  if (t.renamed > 0) parts.push(`${plural(t.renamed, "file")} will get a numbered name because the name is taken`);
  if (t.kept > 0) parts.push(`${plural(t.kept, "existing file")} will be left as ${t.kept === 1 ? "it is" : "they are"}`);
  if (t.same > 0) parts.push(`${plural(t.same, "audio file")} ${t.same === 1 ? "is" : "are"} already there`);
  if (t.problems > 0) parts.push(`${plural(t.problems, "file")} cannot be written`);
  return parts.join("; ");
}

/** Cases to tick first when "export all" is asked: every case except the Pacemaker (★) copy of a
 *  smart case (the smart case's own file says the same); every smart case. */
export function defaultSelection(infos: CaseInfo[], smart: SmartCase[]): { caseIds: number[]; smartIds: number[] } {
  const copies = new Set(smart.map((s) => s.device_case_id).filter((x): x is number => x != null));
  return { caseIds: infos.filter((c) => !copies.has(c.id)).map((c) => c.id), smartIds: smart.map((s) => s.id) };
}

/** The request the backend previews and runs, from the dialog's choices. */
export function requestOf(p: ExportPrefs, caseIds: Iterable<number>, smartIds: Iterable<number>): ExportRequest {
  return {
    folder: (p.folder ?? "").trim(),
    format: p.format,
    case_ids: [...caseIds],
    smart_ids: [...smartIds],
    relative_paths: p.format === "m3u8" && p.relative_paths,
    copy_audio: p.copy_audio,
    audio_layout: p.audio_layout,
    audio_names: p.audio_names,
    conflict: p.conflict,
  };
}

/** "Replace" is a choice for one export, never a default: it comes back as "skip" (the saved
 *  choice of an older version included), so the next dialog cannot start out set to overwrite. */
export function safeConflict(c: ExportPrefs["conflict"]): ExportPrefs["conflict"] {
  return c === "replace" ? "skip" : c;
}

/** The choices a dialog starts from: the saved ones, without a saved "replace". */
export function startingPrefs(saved: ExportPrefs): ExportPrefs {
  return { ...saved, conflict: safeConflict(saved.conflict) };
}

/** What to save when an export starts: the dialog's choices, but the folder and format only if
 *  the user asked for them to be remembered (the policies are saved either way, except that
 *  "replace" is not kept). */
export function prefsToSave(draft: ExportPrefs, stored: ExportPrefs): ExportPrefs {
  const d = { ...draft, conflict: safeConflict(draft.conflict) };
  return d.remember ? d : { ...d, folder: stored.folder, format: stored.format };
}

/** Whether the shown plan was made for exactly the request that would be started now. */
export function planIsCurrent(plannedFor: string | null, request: ExportRequest): boolean {
  return plannedFor !== null && plannedFor === JSON.stringify(request);
}

/** After a one-file export (the quick menu items) the remembered folder and format follow it. */
export function afterQuickExport(stored: ExportPrefs, dest: string, format: ExportPrefs["format"]): ExportPrefs | null {
  if (!stored.remember) return null;
  const folder = dirName(dest);
  if (folder === (stored.folder ?? "") && format === stored.format) return null;
  return { ...stored, folder: folder || stored.folder, format };
}

/** Ordering for the preview's file list: problems first, then files that change something that
 *  exists, then the rest, each group in plan order. */
export function listRank(a: ExportAction): number {
  switch (a) {
    case "missing":
    case "blocked":
      return 0;
    case "replace":
      return 1;
    case "renamed":
      return 2;
    case "skip":
      return 3;
    case "write":
      return 4;
    case "same":
      return 5;
  }
}

export type Row = { kind: "list" | "audio"; file: string; text: string; detail: string; action: ExportAction };

/** The rows of the preview table: playlists first (with their track counts), then audio. */
export function rows(plan: ExportPlan): Row[] {
  const lists = plan.lists.map((l: ExportListEntry) => ({
    kind: "list" as const,
    file: l.file,
    text: actionText(l.action, "list", l.wanted),
    detail: `${l.smart ? "Smart case “" : "“"}${l.name}” · ${plural(l.tracks, "track")}`,
    action: l.action,
  }));
  const audio = plan.audio.map((a: ExportAudioEntry) => ({
    kind: "audio" as const,
    file: a.file,
    text: actionText(a.action, "audio", a.wanted, a.problem),
    detail: `${a.title} · ${fmtBytes(a.size)}${a.used_by > 1 ? ` · in ${a.used_by} playlists` : ""}`,
    action: a.action,
  }));
  return [...lists, ...audio];
}
