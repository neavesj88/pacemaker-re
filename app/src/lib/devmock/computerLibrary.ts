// Mock of src-tauri/src/commands/library/computer.rs: the library on this computer. State lives
// in memory (`?computer=none` in the address starts without one, to see the Welcome screen).
import { libraryKind } from "../state/place";
import type { Handlers } from "./types";

const DEFAULT = "C:\\Users\\demo\\Music\\Pacemaker Revival";
const none = typeof location !== "undefined" && new URLSearchParams(location.search).get("computer") === "none";

let settings = { path: "", open_at_start: true };
/** The folders that hold a library, like the disk would. */
const made = new Set<string>(none ? [] : [DEFAULT]);

const where = () => settings.path || DEFAULT;
const info = () => ({ settings: { ...settings }, path: where(), default_path: DEFAULT, exists: made.has(where()), warning: null });

export const handlers: Handlers = {
  computer_library_get: () => info(),
  computer_library_set: (a) => {
    const path = String(a.settings?.path ?? "").trim();
    if (path && libraryKind(path) === "pacemaker") throw "That is a whole drive, which counts as a Pacemaker. Choose a folder on it instead.";
    if (path && !/^([A-Za-z]:[\\/]|\/|\\\\)/.test(path)) throw "Give the full path of the folder, for example C:\\Users\\you\\Music\\Pacemaker Revival.";
    settings = { path, open_at_start: a.settings?.open_at_start !== false };
    return info();
  },
  computer_library_create: () => {
    made.add(where());
    return where();
  },
};
