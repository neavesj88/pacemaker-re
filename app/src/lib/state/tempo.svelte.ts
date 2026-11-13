// Tempo edits on tracks of the library: halve, double or set the BPM of the selection. A track's
// BPM comes from its beat grid (.str), so every edit rewrites both together through the grid
// editor's own maths on the backend (commands/grid/tempo.rs). The view that offers them (the
// tracks table's menu, the inspector) calls `tempo.askHalve(ids)`, `askDouble(ids)` or
// `askSet(id)`; the bulk-edit dialog is views/grid/Overlays.svelte.
import { gridTempoApply, gridTempoPlan } from "$lib/api/grid";
import { jobs } from "./jobs.svelte";
import { mixesNote } from "./mixNote";
import { session } from "./session.svelte";
import { changing, nothingToDo, opDescription, opTitle, rowLine, type TempoOp } from "./tempoLogic";

export const TEMPO_JOB = "tempo-edit";

class Tempo {
  /** The track whose BPM is being typed in the dialog, or null when it is closed. */
  settingId = $state<number | null>(null);

  askSet(trackId: number) {
    if (!session.library?.tracks.some((t) => t.track_id === trackId)) return;
    this.settingId = trackId;
  }

  closeDialog() {
    this.settingId = null;
  }

  askHalve(ids: number[]) {
    return this.ask({ kind: "halve" }, ids);
  }

  askDouble(ids: number[]) {
    return this.ask({ kind: "double" }, ids);
  }

  /** Step two of "Set BPM": the dialog has a valid tempo, now the usual write confirmation. */
  continueSet(trackId: number, bpm: number) {
    this.settingId = null;
    return this.ask({ kind: "set", bpm }, [trackId]);
  }

  /** Asks the backend what `op` would do to each track, then opens the write confirmation with
   *  one line per track saying so. The job recomputes with the same code when confirmed. */
  async ask(op: TempoOp, ids: number[]) {
    const lib = session.library;
    if (!lib || ids.length === 0) return;
    if (!jobs.guard()) return;
    let rows;
    try {
      rows = await gridTempoPlan(lib.root, ids, op);
    } catch (e) {
      session.error = String(e);
      return;
    }
    const todo = changing(rows);
    if (todo.length === 0) {
      session.error = nothingToDo(rows);
      return;
    }
    const n = todo.length;
    const note = await mixesNote(lib.root, todo.map((r) => r.track_id));
    jobs.ask({
      kind: TEMPO_JOB,
      root: lib.root,
      count: n,
      // every track asked for, the ones that stay as they are included, with the reason
      names: rows.map(rowLine),
      title: opTitle(op, n, lib.root),
      description: opDescription(op) + note,
      confirmLabel: `Change ${n}`,
      runningLabel: "Changing BPM",
      doneLabel: "BPM change finished",
      // only the tracks the confirmation names as changing, so a track shown as left alone stays alone
      // and each one's tempo as confirmed, so a grid changed meanwhile is skipped, not changed again
      start: () =>
        gridTempoApply(
          lib.root,
          todo.map((r) => r.track_id),
          op,
          todo.map((r) => ({ track_id: r.track_id, before: r.before as number })),
        ),
    });
  }
}

export const tempo = new Tempo();
