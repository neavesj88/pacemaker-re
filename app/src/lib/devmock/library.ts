// Mock of src-tauri/src/commands/library.rs.
import { needs } from "./db";
import { libraryKind } from "../state/place";
import { libraryFor, stripIds } from "./libraries";
import type { Handlers } from "./types";

export const handlers: Handlers = {
  list_volumes: () => ["P:\\"],
  app_notices: () => "# Third-party notices\n\nThe licences of the software in this app are listed here in the installed app.\n",
  open_library: (a) => {
    const root = a.root ?? "P:\\";
    const kind = libraryKind(root);
    // the computer's library is the shared mock one; the Pacemaker's is its own (devmock/libraries.ts),
    // so what a copy between them adds shows in both
    const lib = libraryFor(root);
    return {
      root,
      kind,
      problems: root.includes("broken") ? ["user_version is 12, device requires 677"] : [],
      // fresh objects every time, like the real IPC: the page replaces its rows by identity, so
      // handing back the mock's own rows would hide an in-place change from the table and Inspector
      tracks: lib.tracks.map((t) => ({ ...t })),
      mixes: lib.mixes.map(stripIds),
      cases: lib.cases.map(stripIds),
      needs_analysis: kind === "computer" ? [...needs].map(([track_id, reason]) => ({ track_id, reason })) : [],
    };
  },
};
