// The smart-case settings (how the Pacemaker copies are kept current, how smart cases are
// exported). They are the app's own preferences, kept in its config dir; see
// src-tauri/src/commands/cases/smart_settings.rs. Read the value from `smartSettings.value`.
import { getSmartSettings, setSmartSettings, type SmartSettings } from "$lib/api/cases";
import { session } from "./session.svelte";

export const DEFAULT_SETTINGS: SmartSettings = { refresh_mode: "ask", export_smart: true, export_folder: true };

class SmartSettingsStore {
  value = $state<SmartSettings>({ ...DEFAULT_SETTINGS });
  /** Set when the settings file could not be fully read (the defaults are used for that part). */
  warning = $state<string | null>(null);
  loaded = $state(false);
  #loading: Promise<void> | null = null;

  /** Reads the settings once; later calls wait for the first. */
  ensure(): Promise<void> {
    if (this.loaded) return Promise.resolve();
    this.#loading ??= getSmartSettings()
      .then((s) => {
        this.value = s.settings;
        this.warning = s.warning;
      })
      .catch((e) => {
        session.error = String(e);
      })
      .finally(() => {
        this.loaded = true;
        this.#loading = null;
      });
    return this.#loading;
  }

  /** Saves one or more settings; the screen changes at once and goes back if the save fails. */
  async set(patch: Partial<SmartSettings>) {
    await this.ensure();
    const before = { ...this.value };
    this.value = { ...this.value, ...patch };
    try {
      const s = await setSmartSettings($state.snapshot(this.value) as SmartSettings);
      this.value = s.settings;
      this.warning = s.warning;
    } catch (e) {
      this.value = before;
      session.error = String(e);
    }
  }
}

export const smartSettings = new SmartSettingsStore();
