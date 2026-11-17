// Pure rules of the Firmware card: whether the firmware shipped with the app is offered, which files
// get the warning dialog, and the lines the dialog and the recovery-file row show. Plain TypeScript so
// plain node runs it (firmwareLogic.test.mjs); firmware.svelte.ts owns the setting and the read.

/** Browser-storage key of "Offer the included firmware" (per computer, like the other app settings). */
export const OFFER_KEY = "pacemaker-revival.offerBundledFirmware";
export const OFFER_DEFAULT = true;

/** Saved setting (also the `normalise` for loadPref): only an explicit false turns it off. */
export const normaliseOffer = (raw: unknown): boolean => (typeof raw === "boolean" ? raw : OFFER_DEFAULT);

/** The first firmware number of the stock line; every number above it is a Revival build. */
export const STOCK_FIRMWARE = 180_096_069;

/** A firmware number as the card shows it ("180096069"); null when it is not a plain whole number. */
export function fwNumber(v: string | number | null | undefined): number | null {
  if (typeof v === "number") return Number.isSafeInteger(v) && v >= 0 ? v : null;
  const t = (v ?? "").trim();
  return /^\d+$/.test(t) ? Number(t) : null;
}

/** Whether the file being installed is a Revival build and so gets the warning dialog: the file is
 *  named for it, or its number is above the last stock one. A file that is neither (an older stock
 *  image restored by hand) is copied as before. */
export function isRevivalFirmware(fileName: string, fwVersion: string | number | null | undefined): boolean {
  if (/revival/i.test(fileName)) return true;
  const n = fwNumber(fwVersion);
  return n !== null && n > STOCK_FIRMWARE;
}

/** Offer the included firmware: it exists, the setting is on, and its number is above the unit's.
 *  An unreadable installed number gives no offer (there is nothing to compare). */
export function offerUpdate(bundled: number | null, installed: string | null | undefined, offer: boolean): boolean {
  const have = fwNumber(installed);
  return offer && bundled !== null && have !== null && bundled > have;
}

/** The label of the included-firmware button, which is always there when the app ships a file (so its
 * warning can be read at any time): an update when it is newer, the warning alone when it is already installed, else an install. */
export function bundledLabel(bundled: number, installed: string | null | undefined, offered: boolean): string {
  if (offered) return `Update to ${bundled}`;
  // equal: the bootloader skips a file with the installed number, so there is nothing to install, only the warning to read
  return fwNumber(installed) === bundled ? "Firmware warning…" : `Install ${bundled}…`;
}

/** The warning dialog's "Copy firmware" button needs both boxes. */
export const canCopy = (backup: boolean, risk: boolean): boolean => backup && risk;

/** "2026-11-17 14:05" in the computer's time zone: the date of the last backup this session. */
export function backupStamp(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** The recovery-file row: state text and button label for whether the file is on the card. */
export const recoveryState = (on: boolean): string => (on ? "On the card" : "Not on the card");
export const recoveryButton = (on: boolean): string => (on ? "Remove recovery file" : "Add recovery file");
