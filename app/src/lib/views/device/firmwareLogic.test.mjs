// Run: node --experimental-strip-types --test src/lib/views/device/firmwareLogic.test.mjs   (from app/)
import assert from "node:assert/strict";
import { test } from "node:test";
import { OFFER_DEFAULT, backupStamp, bundledLabel, canCopy, fwNumber, isRevivalFirmware, normaliseOffer, offerUpdate, recoveryButton, recoveryState } from "./firmwareLogic.ts";

test("the setting defaults on and only an explicit false turns it off", () => {
  assert.equal(OFFER_DEFAULT, true);
  assert.equal(normaliseOffer(null), true);
  assert.equal(normaliseOffer("false"), true, "junk: on");
  assert.equal(normaliseOffer(false), false);
  assert.equal(normaliseOffer(true), true);
});

test("firmware numbers are whole numbers or nothing", () => {
  assert.equal(fwNumber("180096073"), 180096073);
  assert.equal(fwNumber(" 180096073 "), 180096073);
  assert.equal(fwNumber(180096073), 180096073);
  for (const bad of [null, undefined, "", "unknown", "1.5", "-3", NaN, 1.5]) assert.equal(fwNumber(bad), null, String(bad));
});

test("the update is offered only for a higher number and with the setting on", () => {
  assert.equal(offerUpdate(180096074, "180096069", true), true);
  assert.equal(offerUpdate(180096074, "180096074", true), false, "equal: nothing to update");
  assert.equal(offerUpdate(180096073, "180096074", true), false, "unit is newer");
  assert.equal(offerUpdate(180096074, "180096069", false), false, "setting off");
  assert.equal(offerUpdate(null, "180096069", true), false, "nothing bundled");
  assert.equal(offerUpdate(180096074, null, true), false, "unit number unreadable");
  assert.equal(offerUpdate(180096074, "unknown", true), false);
  assert.equal(offerUpdate(180096074, "99", true), true, "numeric compare, not text: 99 < 180096074");
  assert.equal(offerUpdate(99, "180096069", true), false, "numeric compare, not text");
});

test("the included-firmware button: update when newer, the warning when equal, install otherwise", () => {
  assert.equal(bundledLabel(180096074, "180096069", true), "Update to 180096074");
  assert.equal(bundledLabel(180096074, "180096074", false), "Firmware warning…");
  assert.equal(bundledLabel(180096074, "180096075", false), "Install 180096074…");
  assert.equal(bundledLabel(180096074, null, false), "Install 180096074…");
  assert.equal(bundledLabel(180096074, "180096069", false), "Install 180096074…", "offer setting off");
});

test("Revival files get the warning, an old stock image does not", () => {
  assert.equal(isRevivalFirmware("pacemaker-revival-180096069.pfw", "180096069"), true, "named for it");
  assert.equal(isRevivalFirmware("PACEMAKER-REVIVAL-x.pfw", "?"), true);
  assert.equal(isRevivalFirmware("firmware.pfw", "180096070"), true, "above the last stock number");
  assert.equal(isRevivalFirmware("firmware.pfw", "180096069"), false);
  assert.equal(isRevivalFirmware("stock.pfw", "13000"), false);
  assert.equal(isRevivalFirmware("stock.pfw", ""), false);
});

test("Copy firmware needs both boxes", () => {
  assert.equal(canCopy(true, true), true);
  assert.equal(canCopy(true, false), false);
  assert.equal(canCopy(false, true), false);
  assert.equal(canCopy(false, false), false);
});

test("lines", () => {
  assert.match(backupStamp(new Date(2026, 10, 7, 9, 5).getTime()), /^2026-11-07 09:05$/);
  assert.equal(recoveryState(true), "On the card");
  assert.equal(recoveryState(false), "Not on the card");
  assert.equal(recoveryButton(true), "Remove recovery file");
  assert.equal(recoveryButton(false), "Add recovery file");
});
