// Run: node --experimental-strip-types src/lib/views/device/reloadLogic.test.mjs
import assert from "node:assert/strict";
import { reloadPlan } from "./reloadLogic.ts";

assert.equal(reloadPlan("G:\\", "G:\\", false, true), "load", "open library, Device view on screen");
assert.equal(reloadPlan("G:\\", "g:/", false, true), "load", "two spellings of one drive");
assert.equal(reloadPlan("G:\\", "G:\\", false, false), "clear", "open library, another view: no scan of a view nobody sees");
// the incident: the Device view last read G:, the user then switched to This computer
assert.equal(reloadPlan("G:\\", "C:\\Users\\me\\Music\\Pacemaker Revival", false, true), "forget", "never read the old drive");
assert.equal(reloadPlan("G:\\", "C:\\Users\\me\\Music\\Pacemaker Revival", false, false), "forget");
assert.equal(reloadPlan("", "G:\\", false, true), "none", "the Device view never read anything");
assert.equal(reloadPlan("G:\\", null, false, true), "none", "no library open");
assert.equal(reloadPlan("G:\\", "G:\\", true, true), "none", "ejected: the drive is gone");

console.log("reloadLogic.test.mjs ok");
