// Run: node --experimental-strip-types --test src/lib/views/cases/holdersLogic.test.mjs
// Synthetic names only.
import assert from "node:assert/strict";
import { test } from "node:test";
import { holdingRows } from "./holdersLogic.ts";

const hold = (case_id, name, position, of, smart_copy_of = null) => ({ case_id, name, position, of, smart_copy_of });

test("each case is listed with the track's place in it", () => {
  const r = holdingRows({ track_id: 1, cases: [hold(1, "Sample Case", 3, 10), hold(2, "Other Case", 1, 1)], smart: [] });
  assert.deepEqual(r.cases, [
    { case_id: 1, name: "Sample Case", note: "3 of 10" },
    { case_id: 2, name: "Other Case", note: "1 of 1" },
  ]);
  assert.deepEqual(r.smart, []);
});

test("a smart case that has a Pacemaker copy holding the track is shown once, as the copy", () => {
  const smart = [
    { id: 7, name: "Rated", on_device: true, device_case_id: 5 },
    { id: 8, name: "Fresh", on_device: true, device_case_id: 6 },
    { id: 9, name: "Rules only", on_device: false, device_case_id: null },
  ];
  const r = holdingRows({ track_id: 1, cases: [hold(5, "★ Rated", 2, 4, 7)], smart });
  assert.equal(r.cases[0].note, "2 of 4, Pacemaker copy of smart case “Rated”");
  assert.deepEqual(r.smart.map((s) => [s.id, s.note]), [
    [8, "rules match; its Pacemaker copy does not hold it yet"],
    [9, "rules match; not a case on the Pacemaker"],
  ]);
});

test("no cases and no smart cases is an empty answer", () => {
  assert.deepEqual(holdingRows({ track_id: 1, cases: [], smart: [] }), { cases: [], smart: [] });
});
