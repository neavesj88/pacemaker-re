// Mock of the native dialog plugin: a file pick returns three invented audio paths (one that
// is skipped, one that fails), a folder pick returns the fake drive. Shared across areas.
import type { Handlers } from "./types";

export const handlers: Handlers = {
  "plugin:dialog|open": (a) => {
    const o = a.options ?? {};
    // an image pick (the export dialog's cover) returns an invented picture
    if ((o.filters ?? []).some((f: { extensions?: string[] }) => f.extensions?.includes("png"))) return "C:\\Pictures\\cover art.png";
    // the folder for the library on this computer
    if (String(o.title ?? "").includes("on this computer")) return "C:\\Users\\demo\\Music\\My DJ Library";
    if (o.multiple) return ["C:\\Music\\New Artist - New Tune.mp3", "C:\\Music\\Another - Skip Me.flac", "C:\\Music\\bad file.wav"];
    // "Open folder…": the fake drive, or the folder a preview session set in window.__devmockFolder
    return (globalThis as { __devmockFolder?: string }).__devmockFolder ?? "P:\\";
  },
  // a save dialog returns a fake path under a made-up folder, named after the suggested file
  "plugin:dialog|save": (a) => {
    const d = String(a.options?.defaultPath ?? "export");
    return `C:\\Exports\\${d.split(/[\\/]/).pop()}`;
  },
};
