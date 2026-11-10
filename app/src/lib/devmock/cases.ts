// Mock of src-tauri/src/commands/cases.rs.
import { cases } from "./db";
import type { Handlers } from "./types";

const favourites = new Set<number>();

export const handlers: Handlers = {
  case_tracks: (a) => cases.find((c) => c.id === a.caseId)?.ids ?? [],
  cases_toggle_favourite: (a) => (favourites.delete(a.trackId) ? false : (favourites.add(a.trackId), true)),
  cases_favourite_ids: () => [...favourites],
};
