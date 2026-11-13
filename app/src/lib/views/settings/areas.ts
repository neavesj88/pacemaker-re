// The groups of the Settings view, in the order they are shown. Plain data, so the node tests
// can read it without Svelte.
export const AREAS = ["Library", "Cases", "Mixes", "Analysis", "Import and export", "History", "Device", "Stems", "About"] as const;
export type SettingsArea = (typeof AREAS)[number];
