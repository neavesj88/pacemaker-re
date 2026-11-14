// Run: node --experimental-strip-types --test src/lib/views/cases/smartRefresh.test.mjs
// The sentences that name what a refresh of the Pacemaker copies of smart cases changes, and the
// signature that remembers an answered "Not now". Synthetic names only.
import assert from "node:assert/strict";
import { test } from "node:test";
import { changeParts, changed, decideCheck, doneSummary, itemLine, itemTrackLines, loopMessage, needsAttention, planSignature, remakeWhen, skipped, stuckLines, trackLine } from "./smartRefreshLogic.ts";

const tr = (id, title = `Sample Title ${id}`, artist = "Sample Artist") => ({ track_id: id, title, artist });
const item = (over = {}) => ({
  id: 1, name: "Example Rule", device_name: "★ Example Rule", action: "update", before: 5, after: 6,
  added: [], removed: [], reordered: false, renamed_from: null, recreated: false, error: null, ...over,
});
const report = (...items) => ({ items, changes: items.filter((i) => i.action === "create" || i.action === "update").length });

test("a track is named with its artist when there is one", () => {
  assert.equal(trackLine(tr(1)), "Sample Title 1 by Sample Artist");
  assert.equal(trackLine(tr(2, "Only Title", "")), "Only Title");
  assert.equal(trackLine(tr(3, "  ", "")), "(untitled)");
});

test("what differs is spelled out part by part", () => {
  assert.deepEqual(changeParts(item({ added: [tr(1), tr(2)], removed: [tr(3)], reordered: true, renamed_from: "★ other" })), ["2 added", "1 removed", "new order", "renamed back"]);
  assert.deepEqual(changeParts(item()), []);
});

test("every kind of item reads as one sentence, before and after the write", () => {
  const upd = item({ added: [tr(1), tr(2)], removed: [tr(3)] });
  assert.equal(itemLine(upd, false), "“Example Rule”: “★ Example Rule” will have 6 tracks (was 5; 2 added, 1 removed)");
  assert.equal(itemLine(upd, true), "“Example Rule”: “★ Example Rule” now has 6 tracks (was 5; 2 added, 1 removed)");
  const made = item({ action: "create", before: 0, after: 1, added: [tr(1)] });
  assert.equal(itemLine(made, false), "“Example Rule”: “★ Example Rule” will be made with 1 track");
  assert.equal(itemLine(item({ ...made, recreated: true }), true), "“Example Rule”: “★ Example Rule” made again (it was deleted on the Pacemaker) with 1 track");
  assert.equal(itemLine(item({ action: "unchanged" }), false), "“Example Rule”: “★ Example Rule” is up to date");
  assert.equal(itemLine(item({ action: "skip", error: "A rule names a case that no longer exists." }), false), "“Example Rule”: not refreshed, “★ Example Rule” left as it was. A rule names a case that no longer exists.");
});

test("the detail list names every track that comes and goes, added first", () => {
  const lines = itemTrackLines(item({ added: [tr(1), tr(2)], removed: [tr(3, "Gone", "")] }));
  assert.deepEqual(lines, [
    { sign: "+", text: "Sample Title 1 by Sample Artist" },
    { sign: "+", text: "Sample Title 2 by Sample Artist" },
    { sign: "-", text: "Gone" },
  ]);
  assert.equal(itemTrackLines(item({ reordered: true })).length, 0);
});

test("the summary after a refresh names each smart case and each skip", () => {
  const r = report(
    item({ id: 1, name: "A", added: [tr(1)], removed: [tr(2), tr(3)] }),
    item({ id: 2, name: "B", action: "create", after: 4, recreated: true }),
    item({ id: 3, name: "C", action: "unchanged" }),
    item({ id: 4, name: "D", action: "skip", error: "bad rule" }),
  );
  assert.equal(changed(r).length, 2);
  assert.equal(skipped(r).length, 1);
  assert.equal(
    doneSummary(r),
    "Refreshed 2 smart cases on the Pacemaker: “A” (1 added, 2 removed); “B” made again (4 tracks). Not refreshed: “D” (bad rule).",
  );
  assert.equal(doneSummary(report(item({ action: "unchanged" }))), "The Pacemaker copies of your smart cases were already up to date.");
  assert.equal(doneSummary(report(item({ action: "skip", error: "bad rule" }))), "No smart case was refreshed. Not refreshed: “Example Rule” (bad rule).");
  assert.equal(doneSummary(report(item({ added: [tr(1)], renamed_from: "x" }))), "Refreshed 1 smart case on the Pacemaker: “Example Rule” (1 added, renamed back).");
});

test("a pending write or a smart case that cannot run is worth surfacing, an up-to-date plan is not", () => {
  assert.equal(needsAttention(report(item({ action: "unchanged" }))), false);
  assert.equal(needsAttention(report()), false);
  assert.equal(needsAttention(report(item({ action: "unchanged" }), item({ action: "skip", error: "x" }))), true);
  assert.equal(needsAttention(report(item({ added: [tr(1)] }))), true);
  assert.deepEqual(stuckLines([item({ action: "skip", error: "bad rule" }), item({ name: "Other", action: "skip" })]), ["“Example Rule” was not refreshed: bad rule", "“Other” was not refreshed: unknown reason"]);
});

const mem = () => ({ dismissed: "", failed: "", written: new Set() });
const pending = (...ids) => report(item({ added: ids.map((n) => tr(n)) }));
const stuck = () => report(item({ action: "skip", error: "bad rule" }), item({ id: 2, action: "unchanged" }));

test("the automatic check follows the mode, and a skip-only plan is shown in ask mode and said once in auto mode", () => {
  assert.equal(decideCheck(pending(1), "off", mem()), "nothing");
  assert.equal(decideCheck(pending(1), "ask", mem()), "ask");
  assert.equal(decideCheck(pending(1), "auto", mem()), "write");
  assert.equal(decideCheck(report(item({ action: "unchanged" })), "auto", mem()), "nothing");
  assert.equal(decideCheck(stuck(), "ask", mem()), "ask", "a smart case that cannot run is surfaced, not only on the button");
  const m = mem();
  assert.equal(decideCheck(stuck(), "auto", m), "stuck", "nothing to write, so the skips are reported instead");
  m.failed = planSignature(stuck());
  assert.equal(decideCheck(stuck(), "auto", m), "nothing", "...once");
  assert.equal(decideCheck(report(item({ action: "skip", error: "another reason" })), "auto", m), "stuck", "a different skip is news again");
});

test("Not now and a failed write are remembered per plan, and forgotten once nothing is pending", () => {
  const m = mem();
  m.dismissed = planSignature(pending(1));
  assert.equal(decideCheck(pending(1), "ask", m), "nothing");
  assert.equal(decideCheck(pending(1, 2), "ask", m), "ask");
  m.failed = planSignature(pending(3));
  assert.equal(decideCheck(pending(3), "auto", m), "nothing");
  assert.equal(decideCheck(report(item({ action: "unchanged" })), "auto", m), "nothing");
  assert.deepEqual([m.dismissed, m.failed], ["", ""]);
  assert.equal(decideCheck(pending(1), "ask", m), "ask", "after settling, the same plan is asked again");
});

test("automatic mode never writes the same plan twice before things settle (two smart cases undoing each other)", () => {
  const m = mem();
  const a = pending(1);
  const b = pending(2);
  assert.equal(decideCheck(a, "auto", m), "write");
  m.written.add(planSignature(a)); // the store records it when it writes
  assert.equal(decideCheck(b, "auto", m), "write", "a different plan is fine");
  m.written.add(planSignature(b));
  assert.equal(decideCheck(a, "auto", m), "loop", "the first plan is back: stop instead of writing the card again");
  assert.match(loopMessage(a), /Automatic refresh of smart cases stopped: the changes to “Example Rule” were already written and came back/);
  // once the library settles (nothing left to write) the memory is cleared and a later identical plan is written normally
  assert.equal(decideCheck(report(item({ action: "unchanged" })), "auto", m), "nothing");
  assert.equal(m.written.size, 0);
  assert.equal(decideCheck(a, "auto", m), "write");
  // ask mode is the user's call every time, whatever was written before
  m.written.add(planSignature(a));
  assert.equal(decideCheck(a, "ask", m), "ask");
});

test("the signature is the same for the same pending changes and ignores what is up to date", () => {
  const a = report(item({ added: [tr(1)], removed: [tr(2)] }), item({ id: 2, action: "unchanged" }));
  const same = report(item({ added: [tr(1, "Renamed In The Library")], removed: [tr(2)] }));
  assert.equal(planSignature(a), planSignature(same), "track titles are not part of it, ids are");
  assert.notEqual(planSignature(a), planSignature(report(item({ added: [tr(1), tr(9)], removed: [tr(2)] }))));
  assert.notEqual(planSignature(a), planSignature(report(item({ added: [tr(1)], removed: [tr(2)], reordered: true }))));
  assert.notEqual(planSignature(a), planSignature(report(item({ action: "skip", error: "x" }))));
  assert.equal(planSignature(report(item({ action: "unchanged" }))), "", "nothing pending is the empty signature");
});

test("a deleted copy's note says when it comes back, per the setting", () => {
  assert.equal(remakeWhen("off"), "the next time you refresh");
  assert.equal(remakeWhen("ask"), "when you answer yes to the refresh question");
  assert.equal(remakeWhen("auto"), "automatically straight afterwards");
});

test("for a library on this computer the sentences say ★ case and name the library, never the Pacemaker", () => {
  const made = item({ action: "create", before: 0, after: 1, added: [tr(1)], recreated: true });
  assert.equal(itemLine(made, true, "computer"), "“Example Rule”: “★ Example Rule” made again (it was deleted in this library) with 1 track");
  assert.equal(doneSummary(report(item({ action: "unchanged" })), "computer"), "The ★ cases of your smart cases were already up to date.");
  assert.match(doneSummary(report(item({ added: [tr(1)] })), "computer"), /^Refreshed 1 smart case in this library: /);
  assert.doesNotMatch(loopMessage(report(item({ added: [tr(1)] })), "computer"), /Pacemaker/);
});
