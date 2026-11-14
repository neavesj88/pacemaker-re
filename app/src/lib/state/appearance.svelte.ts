// The interface size (the webview zoom) and how long error messages stay. Both are kept per
// computer in the browser storage like the player bar height, and read when this module loads,
// which the root layout does before the first view is built, so the size is applied at start.
import { loadPref, savePref } from "./prefStore";
import {
  ERROR_SECS_DEFAULT,
  ERROR_SECS_KEY,
  SCALE_DEFAULT,
  SCALE_KEY,
  clampScale,
  normaliseErrorSecs,
  normaliseScale,
  stepScale,
  zoomFactor,
} from "./appearanceLogic";

class Appearance {
  /** Interface size in per cent. */
  scale = $state(loadPref(SCALE_KEY, normaliseScale, () => SCALE_DEFAULT));
  /** Seconds an error message stays; 0 = until closed. */
  errorSecs = $state(loadPref(ERROR_SECS_KEY, normaliseErrorSecs, () => ERROR_SECS_DEFAULT));
  /** The webview could not be zoomed (the browser preview, or the permission is missing). */
  zoomProblem = $state("");

  constructor() {
    void this.apply();
  }

  /** Makes the webview show the saved size. */
  async apply() {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      await getCurrentWebview().setZoom(zoomFactor(this.scale));
      this.zoomProblem = "";
    } catch (e) {
      this.zoomProblem = String(e);
    }
  }

  setScale(pct: number) {
    this.scale = clampScale(pct);
    savePref(SCALE_KEY, this.scale);
    void this.apply();
  }

  step(dir: 1 | -1) {
    this.setScale(stepScale(this.scale, dir));
  }

  reset() {
    this.setScale(SCALE_DEFAULT);
  }

  setErrorSecs(secs: number) {
    this.errorSecs = normaliseErrorSecs(secs);
    savePref(ERROR_SECS_KEY, this.errorSecs);
  }
}

export const appearance = new Appearance();
