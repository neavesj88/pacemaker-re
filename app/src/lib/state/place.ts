// Pure rules about WHERE a library is: on a Pacemaker (a drive or card) or in a folder on this
// computer. Run by plain node in `npm test`. The kind itself comes from the backend
// (`LibraryView.kind`); `libraryKind` here mirrors `library_kind` in
// src-tauri/src/commands/library/kind.rs for the browser preview and is checked against the same
// table of paths by place.test.mjs.
import type { LibraryKind } from "$lib/api/types";

/** "pacemaker" when `root` is a volume root (a drive root, or a mount point), else "computer". */
export function libraryKind(root: string): LibraryKind {
  let s = root.trim().replace(/\\/g, "/");
  if (s.startsWith("//?/")) {
    s = s.slice(4);
    if (s.startsWith("UNC/")) return "computer";
  } else if (s.startsWith("//")) {
    return "computer";
  }
  if (/^[A-Za-z]:\/*$/.test(s)) return "pacemaker";
  if (!s.startsWith("/")) return "computer";
  const parts = s.split("/").filter(Boolean);
  if (parts.includes(".") || parts.includes("..")) return "computer";
  const bases = [["Volumes"], ["media"], ["run", "media"], ["mnt"]];
  const hit = bases.some((b) => b.every((p, i) => parts[i] === p) && [1, 2].includes(parts.length - b.length));
  return hit ? "pacemaker" : "computer";
}

/** The words that name the place in a sentence. */
export type Place = {
  /** "the Pacemaker" / "this library": "Delete the case from ___." */
  the: string;
  /** "on the Pacemaker" / "in this library". */
  on: string;
  /** "the Pacemaker's" / "this library's". */
  its: string;
  /** "the card" / "the library": where music.db and the audio are. */
  card: string;
  /** "the drive" / "the library folder". */
  drive: string;
  /** "the device" / "the library": where a write or a read of the open library goes. */
  device: string;
  /** "this card" / "this library". */
  thisCard: string;
};

const WORDS: Record<LibraryKind, Place> = {
  pacemaker: { the: "the Pacemaker", on: "on the Pacemaker", its: "the Pacemaker's", card: "the card", drive: "the drive", device: "the device", thisCard: "this card" },
  computer: { the: "this library", on: "in this library", its: "this library's", card: "the library", drive: "the library folder", device: "the library", thisCard: "this library" },
};

export const placeWords = (kind: LibraryKind | undefined): Place => WORDS[kind ?? "pacemaker"];

/** "This computer", "Folder (Gigs)" or "Pacemaker (G:)": how a library is named in the header and the
 *  pickers. `computerRoot` is where the library on this computer is: a library on this computer at
 *  any other path is a folder opened by hand and is named by its folder, so two entries are never
 *  both "This computer". Left out or null (not loaded yet), any library on this computer is "This computer". */
export function libraryLabel(kind: LibraryKind, root: string, computerRoot?: string | null): string {
  if (kind === "computer") {
    if (computerRoot == null || sameRoot(root, computerRoot)) return "This computer";
    return `Folder (${root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || root})`;
  }
  const m = /^([A-Za-z]:)[\\/]*$/.exec(root.trim());
  const name = m ? m[1] : root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || root;
  return `Pacemaker (${name})`;
}

/** Two spellings of one library root (`E:` and `e:/`, a trailing separator) are the same place. */
export const sameRoot = (a: string, b: string) => rootKey(a) === rootKey(b);
export const rootKey = (r: string) => r.trim().replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

/** `title` is the full path, shown as a tooltip. */
export type PickerOption = { value: string; label: string; title: string };

/** The library picker: "This computer" first (when a library exists there or the app can make one:
 *  `computerRoot` is its path, null when the path is unknown; "(new)" marks one not made yet), then
 *  each detected Pacemaker. `open` is the open library: when it is neither (a folder chosen by hand)
 *  it is added last, so the picker always shows what is open. */
export function pickerOptions(
  computerRoot: string | null,
  computerExists: boolean,
  volumes: string[],
  open: { root: string; kind: LibraryKind } | null = null,
): PickerOption[] {
  const out: PickerOption[] = [];
  if (computerRoot) out.push({ value: computerRoot, label: libraryLabel("computer", computerRoot, computerRoot) + (computerExists ? "" : " (new)"), title: computerRoot });
  for (const v of volumes) out.push({ value: v, label: libraryLabel("pacemaker", v), title: v });
  if (open && !out.some((o) => sameRoot(o.value, open.root))) out.push({ value: open.root, label: libraryLabel(open.kind, open.root, computerRoot), title: open.root });
  return out;
}

export type StartupInfo = { exists: boolean; path: string; settings: { open_at_start: boolean } } | null;

/** The library to open when the app starts: the one on this computer, only when it is there and
 *  the setting is on. A Pacemaker is never opened by itself (see `Session.refreshVolumes`). */
export function startupRoot(info: StartupInfo): string | null {
  return info && info.exists && info.settings.open_at_start ? info.path : null;
}

/** What the Welcome screen leads with. "start": no library on this computer yet. "open": there is
 *  one but it was not opened at start (the setting is off). "copyFrom" is offered on top of "start"
 *  when a Pacemaker is plugged in (the first one). */
export function welcomeOffer(info: StartupInfo, volumes: string[]): { start: boolean; open: boolean; copyFrom: string | null } {
  const start = !!info && !info.exists;
  return { start, open: !!info && info.exists, copyFrom: start && volumes.length > 0 ? volumes[0] : null };
}
