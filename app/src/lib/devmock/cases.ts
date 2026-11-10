// Mock of src-tauri/src/commands/cases.rs.
import { cases } from "./db";
import type { Handlers } from "./types";

export const handlers: Handlers = {
  case_tracks: (a) => cases.find((c) => c.id === a.caseId)?.ids ?? [],
};
