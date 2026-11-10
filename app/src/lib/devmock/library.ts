// Mock of src-tauri/src/commands/library.rs.
import { cases, needs, tracks } from "./db";
import { mixes } from "./mixes";
import type { Handlers } from "./types";

export const handlers: Handlers = {
  list_volumes: () => ["P:\\"],
  open_library: (a) => {
    const root = a.root ?? "P:\\";
    return {
      root,
      problems: root.includes("broken") ? ["user_version is 12, device requires 677"] : [],
      tracks,
      mixes,
      cases: cases.map(({ ids: _ids, ...c }) => c),
      needs_analysis: [...needs].map(([track_id, reason]) => ({ track_id, reason })),
    };
  },
};
