// What the shell warns about before another library replaces the open one. The mix editor and the
// beat grid editor keep unsaved edits in memory, and both are cleared by a library change (their
// `session.onReset` hooks), so this is the one place that asks. Imported by LeaveDialog.svelte.
import { gridLine, mixLine, registerLeaveGuard } from "$lib/state/leaveGuard";
import { mixes } from "$lib/state/mixes.svelte";
import { saveMix } from "$lib/views/mixes/flow";
import { editor } from "$lib/views/grid/editor.svelte";

registerLeaveGuard(() => {
  const doc = mixes.doc;
  if (!doc || !mixes.dirty) return null;
  return {
    lines: [mixLine(doc.name, mixes.undoDepth)],
    save: async () => {
      await saveMix();
      return !mixes.dirty;
    },
  };
});

// the grid editor confirms its own write (what changes in the .str), so it is only ever discarded here
registerLeaveGuard(() => {
  if (!editor.dirty || !editor.loaded) return null;
  return { lines: [gridLine(editor.loaded.title || editor.loaded.location, editor.gridDirty, editor.cueDirty)] };
});
