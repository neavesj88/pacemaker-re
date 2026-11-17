// The firmware that ships with the app and the setting that decides whether the Firmware card offers
// it. Reading the shipped file looks in the app's own folder, never at a Pacemaker. The rules are in
// firmwareLogic.ts.
import { deviceBundledFirmware, type BundledFirmware } from "$lib/api/device";
import { loadPref, savePref } from "$lib/state/prefStore";
import { OFFER_DEFAULT, OFFER_KEY, normaliseOffer } from "./firmwareLogic";

class FirmwareOffer {
  /** Offer the included firmware when it is newer than the unit's (Settings > Device). */
  offer = $state(loadPref(OFFER_KEY, normaliseOffer, () => OFFER_DEFAULT));
  /** null: none shipped, or not read yet. */
  bundled = $state<BundledFirmware | null>(null);
  #read = false;

  setOffer(on: boolean) {
    this.offer = on;
    savePref(OFFER_KEY, on);
  }

  /** Reads the shipped file once per run (it cannot change while the app runs). */
  async load() {
    if (this.#read) return;
    this.#read = true;
    try {
      this.bundled = await deviceBundledFirmware();
    } catch {
      this.#read = false; // try again next time the card opens
      this.bundled = null;
    }
  }
}

export const firmwareOffer = new FirmwareOffer();
