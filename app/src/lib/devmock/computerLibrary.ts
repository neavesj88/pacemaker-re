// Mock of src-tauri/src/commands/library/computer.rs: the library on this computer. State lives
// in memory (`?computer=none` in the address starts without one, to see the Welcome screen).
import { libraryKind } from "../state/place";
import type { Handlers } from "./types";

/** Also the "This computer" library of the sync mock (device-sync.ts), so there is one computer library in the preview. */
export const DEFAULT = "C:\\Users\\demo\\Music\\Pacemaker Revival";
const mode = typeof location !== "undefined" ? new URLSearchParams(location.search).get("computer") : null;
const none = mode === "none";
/** `?computer=corrupt`: the settings file cannot be read (the default library is there, and must not open by itself). */
let corrupt = mode === "corrupt";

let settings = { path: "", open_at_start: true };
/** The folders that hold a library, like the disk would. */
const made = new Set<string>(none ? [] : [DEFAULT]);

const where = () => settings.path || DEFAULT;
const info = () => ({ settings: { ...settings }, path: where(), default_path: DEFAULT, exists: made.has(where()), warning: corrupt ? WARNING : null, unreadable: corrupt });
const WARNING = "C:\\Users\\demo\\AppData\\Roaming\\au.neaves.pacemaker-revival\\computer-library.json is not readable (expected value at line 1 column 1); defaults are used until you save.";

export const handlers: Handlers = {
  computer_library_get: () => info(),
  computer_library_set: (a) => {
    const path = String(a.settings?.path ?? "").trim();
    if (path && libraryKind(path) === "pacemaker") throw "That is a whole drive, which counts as a Pacemaker. Choose a folder on it instead.";
    if (path && !/^([A-Za-z]:[\\/]|\/|\\\\)/.test(path)) throw "Give the full path of the folder, for example C:\\Users\\you\\Music\\Pacemaker Revival.";
    settings = { path, open_at_start: a.settings?.open_at_start !== false };
    corrupt = false; // a save replaces the damaged file (the real one keeps a .bad copy)
    return info();
  },
  computer_library_create: () => {
    if (corrupt) throw "computer-library.json could not be read, so no library was made. Use the default location in Settings (a copy of the damaged file is kept) or choose a folder first.";
    made.add(where());
    return where();
  },
};
