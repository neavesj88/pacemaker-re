// Settings-preset commands of the device area (src-tauri/src/commands/device/preset.rs): save the
// card's settings.dat editor values as a named preset, look at a preset against a card (a named
// diff) and apply it. Presets are files of this app (its config folder), never on the card.
// Re-exported from api/device.ts.
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

export type PresetSetting = {
  id: number;
  group: string;
  title: string;
  kind: "bool" | "list";
  /** bool: 0 or 1; list: the option index. */
  value: number;
  /** The option's name when the preset was saved. */
  label: string;
};

export type Preset = {
  format: string;
  version: number;
  name: string;
  /** "YYYYMMDDTHHMMSSZ" (UTC), or "" when unknown. */
  saved_utc: string;
  /** The software version of the card it was saved from (information only). */
  firmware: string | null;
  settings: PresetSetting[];
};

export type PresetSummary = {
  name: string;
  file: string;
  settings: number;
  saved_utc: string;
  firmware: string | null;
};

export type PresetList = {
  presets: PresetSummary[];
  /** Files in the presets folder that could not be read, each with the reason. */
  unreadable: string[];
};

/** `exists`: a preset of that name is stored and nothing was written; ask, then repeat with overwrite. */
export type SaveResult = { saved: boolean; exists: boolean; summary: PresetSummary | null };

export type PresetRowStatus = "change" | "same" | "absent" | "unknown" | "invalid";

export type PresetDiffRow = {
  id: number;
  group: string;
  title: string;
  status: PresetRowStatus;
  /** What the card has now, in words (null when the card's file lacks the setting). */
  current: string | null;
  /** What the preset says, in words. */
  wanted: string;
  /** Why it cannot be applied (absent / unknown / invalid rows). */
  detail: string | null;
};

export type PresetDiff = {
  name: string;
  rows: PresetDiffRow[];
  change: number;
  same: number;
  blocked: number;
  /** Editable settings on the card the preset does not mention; applying leaves them alone. */
  untouched: number;
};

export type PresetApplyReport = { changed: number; written: number[] };

export const devicePresetsList = () => invoke<PresetList>("device_presets_list");
export const devicePresetGet = (name: string) => invoke<Preset>("device_preset_get", { name });
/** Saves the card's current editable settings as a preset (to the app's config folder only). */
export const devicePresetSaveCurrent = (root: string, name: string, overwrite: boolean) =>
  invoke<SaveResult>("device_preset_save_current", { root, name, overwrite });
/** Deletes the stored preset (its own file only). Returns whether there was one. */
export const devicePresetDelete = (name: string) => invoke<boolean>("device_preset_delete", { name });
export const devicePresetExport = (name: string, dest: string) => invoke<void>("device_preset_export", { name, dest });
/** Reads and checks a preset file without storing it. */
export const devicePresetReadFile = (path: string) => invoke<Preset>("device_preset_read_file", { path });
export const devicePresetImport = (path: string, name: string | null, overwrite: boolean) =>
  invoke<SaveResult>("device_preset_import", { path, name, overwrite });
export const devicePresetDiff = (root: string, name: string) => invoke<PresetDiff>("device_preset_diff", { root, name });
/** Writes the ticked settings of the preset to the card. The backend diffs again on the card as it is now. */
export const devicePresetApply = (root: string, name: string, ids: number[]) =>
  invoke<PresetApplyReport>("device_preset_apply", { root, name, ids });

export async function pickPresetFile(): Promise<string | null> {
  const f = await open({ title: "Choose a settings preset", filters: [{ name: "Settings preset", extensions: ["json"] }] });
  return typeof f === "string" ? f : null;
}

export async function pickPresetSavePath(name: string): Promise<string | null> {
  const safe = name.replace(/[^A-Za-z0-9 _-]+/g, "").trim().replace(/\s+/g, "-") || "preset";
  const f = await save({
    title: "Save the preset as a file",
    defaultPath: `${safe}.json`,
    filters: [{ name: "Settings preset", extensions: ["json"] }],
  });
  return typeof f === "string" ? f : null;
}
