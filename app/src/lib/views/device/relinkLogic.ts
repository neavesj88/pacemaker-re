// Pure helpers of the Relink dialog. No imports from the app, so `node --experimental-strip-types`
// runs them in relinkLogic.test.mjs. The shapes mirror api/device-relink.ts.

export type Kind = "exact" | "same_name" | "tags" | "manual";
export type Confidence = "sure" | "likely" | "check";
export type Preselect = "none" | "sure" | "likely";

export type Cand = {
  path: string;
  name: string;
  size: number;
  kind: Kind;
  confidence: Confidence;
  action: "point" | "copy";
  dest: string;
  new_location: string;
  on_card: boolean;
};

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

/** The same sizes the Storage card shows ("12.4 MB"). */
export function sizeText(n: number): string {
  if (n >= GB) return `${(n / GB).toFixed(1)} GB`;
  if (n >= MB) return `${(n / MB).toFixed(1)} MB`;
  if (n >= KB) return `${(n / KB).toFixed(1)} KB`;
  return `${n} B`;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** Does a match start ticked under the user's "pre-tick" setting? A hand-picked file never does:
 *  picking it is the tick. */
export function isPreselected(c: Pick<Cand, "confidence" | "kind"> | undefined, mode: Preselect): boolean {
  if (!c || c.kind === "manual" || mode === "none") return false;
  if (c.confidence === "sure") return true;
  return mode === "likely" && c.confidence === "likely";
}

export function confidenceLabel(c: Confidence): string {
  return c === "sure" ? "Sure" : c === "likely" ? "Likely" : "Check";
}

/** What the evidence is, in a few words. */
export function kindLabel(k: Kind): string {
  switch (k) {
    case "exact":
      return "Same file name and size";
    case "same_name":
      return "Same file name, different size";
    case "tags":
      return "Same title, artist and length";
    default:
      return "Picked by you";
  }
}

/** What taking this file does to the card, in words. */
export function actionLine(c: Pick<Cand, "action" | "dest" | "on_card">): string {
  return c.action === "copy"
    ? `Copies the file onto the card as ${c.dest}${c.on_card ? " (it is already on the card, under another name)" : ""}`
    : `Points the track at ${c.dest}, where the file already is; nothing is copied`;
}

export type Chosen = { action: "point" | "copy"; size: number };

export function totals(items: Chosen[]): { copies: number; copyBytes: number; points: number } {
  let copies = 0;
  let copyBytes = 0;
  let points = 0;
  for (const i of items) {
    if (i.action === "copy") {
      copies++;
      copyBytes += i.size;
    } else points++;
  }
  return { copies, copyBytes, points };
}

export function totalsLine(t: { copies: number; copyBytes: number; points: number }): string {
  const parts: string[] = [];
  if (t.copies) parts.push(`copy ${plural(t.copies, "file")} (${sizeText(t.copyBytes)}) onto the card`);
  if (t.points) parts.push(`point ${plural(t.points, "track")} at files already on the card`);
  return parts.length ? `This will ${parts.join(" and ")}.` : "Nothing is ticked.";
}

/** Why the copies cannot start, or "" when they fit. The backend checks again per file. */
export function freeSpaceProblem(copyBytes: number, freeBytes: number | null | undefined): string {
  if (copyBytes === 0 || freeBytes == null || freeBytes <= 0) return "";
  const need = copyBytes + 8 * MB;
  return freeBytes < need ? `The card has ${sizeText(freeBytes)} free and the copies need ${sizeText(need)}.` : "";
}

/** One line of the confirmation: every ticked track, what happens to it. */
export function confirmLine(label: string, c: Pick<Cand, "action" | "path" | "dest" | "size">): string {
  return c.action === "copy"
    ? `${label}: copy ${c.path} (${sizeText(c.size)}) to .Pacemaker/Music/${c.dest}`
    : `${label}: use the file already at .Pacemaker/Music/${c.dest}`;
}

/** The library path before and after, so the change is visible: old -> new, or "stays the same". */
export function locationLine(oldLocation: string, newLocation: string): string {
  return oldLocation.toLowerCase() === newLocation.toLowerCase()
    ? `The track keeps its library path ${newLocation}`
    : `The track's library path changes from ${oldLocation} to ${newLocation}`;
}

/** The headline over the review list. */
export function matchSummary(total: number, matched: number, sure: number): string {
  if (total === 0) return "No track is missing its audio file.";
  if (matched === 0) return `None of the ${plural(total, "missing track")} has a match in this folder.`;
  const rest = matched === sure ? "all of them are sure" : `${sure} ${sure === 1 ? "is" : "are"} sure`;
  return `${matched} of ${plural(total, "missing track")} ${matched === 1 ? "has" : "have"} a match; ${rest}.`;
}

export type ScanProgressLike = { phase: number; done: number; total: number; label: string };

export function scanLine(p: ScanProgressLike | null): string {
  if (!p) return "Starting…";
  if (p.phase === 0) return p.done > 0 ? `Listing folders: ${plural(p.done, "audio file")} so far` : "Listing folders…";
  return p.total > 0 ? `Comparing files: ${p.done} of ${p.total}` : "Comparing files…";
}

export function scanPercent(p: ScanProgressLike | null): number {
  return p && p.phase === 1 && p.total > 0 ? Math.min(100, Math.round((p.done / p.total) * 100)) : 0;
}

/** The card's Music folder, with the separator the drive path already uses. */
export function cardMusicFolder(root: string): string {
  const sep = root.includes("\\") ? "\\" : "/";
  const base = root.replace(/[\\/]+$/, "");
  return `${base}${sep}.Pacemaker${sep}Music`;
}

/** Rows with a match first, in library order within each group. */
export function sortRows<T extends { candidates: unknown[]; track_id: number }>(rows: T[]): T[] {
  return [...rows].sort((a, b) => Number(b.candidates.length > 0) - Number(a.candidates.length > 0) || a.track_id - b.track_id);
}
