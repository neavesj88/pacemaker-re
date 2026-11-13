// Every area's settings entries, discovered from `views/<area>/settings.ts` (default export: a
// list of SettingsEntry). A feature's settings appear here by adding that file; nothing in this
// folder is edited.
import type { SettingsEntry } from "./types";

const modules = import.meta.glob<{ default: SettingsEntry[] }>("../*/settings.ts", { eager: true });

export const settingsEntries: SettingsEntry[] = Object.values(modules).flatMap((m) => m.default);
