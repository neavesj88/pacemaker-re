// Mock of src-tauri/src/commands/import/request.rs (the track matcher's file access). The
// "file" is invented text that shows every kind of row against the synthetic library in db.ts:
// exact matches, a typo, a different mix, a guest credit, a title with no artist, missing tracks.
import type { Handlers } from "./types";

const SAMPLE = [
  "# invented request list for the browser preview",
  "1. Example Artist A - Demo Track 01",
  "Example Artist B - Demo Track 02 (Extended Mix)",
  "Exmaple Artist C - Demo Track 03",
  "Example Artist D - Demo Track 04 (feat. Guest Act)",
  "Demo Track 05",
  "Example Artist F - Demo Track 99",
  "Imaginary Artist - Nonexistent Song",
  "Imaginary Artist - Nonexistent Song",
  "Another Imaginary Act - Second Missing Song (Radio Edit)",
  "https://example.invalid/track/1",
].join("\n");

export const handlers: Handlers = {
  matcher_read_list: (a) => {
    if (!String(a.path ?? "")) throw "Cannot read the file: no path.";
    return { text: SAMPLE, bytes: SAMPLE.length, encoding: "utf-8" };
  },
  matcher_save_text: (a) => {
    const dest = String(a.dest ?? "");
    if (!/\.(txt|csv)$/i.test(dest)) throw "The file name must end in .txt or .csv.";
    if (/[\\/]device[\\/]/i.test(dest)) throw "Choose a folder outside the library: this list is not part of it.";
    return { path: dest, bytes: String(a.text ?? "").length };
  },
};
