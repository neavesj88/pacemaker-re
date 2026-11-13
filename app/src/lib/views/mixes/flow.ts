// Library-level actions of the mix editor: new, open, rename, duplicate, save as, delete, import /
// export .pmk (one or many), export audio. Each asks the user the way it must (unsaved changes, naming what is deleted,
// file dialogs) and reports the outcome with a toast.
import { open, save } from "@tauri-apps/plugin-dialog";
import { exportCancel, exportCueTarget, exportMixAudio, type AudioFormat, type ExportExtras } from "$lib/api/export";
import { extOf, needsDiskCheck, replacements, resolveDest, saveFilters } from "$lib/state/exportprefs";
import {
  mixDeleteMany,
  mixDuplicate,
  mixExportMany,
  mixExportPlan,
  mixExportPmk,
  mixImportPmk,
  mixRename,
  type MixSummary,
} from "$lib/api/mixes";
import { mixExport } from "$lib/state/mixexport.svelte";
import { mixes } from "$lib/state/mixes.svelte";
import { tracklist } from "$lib/state/tracklist.svelte";
import { baseName } from "$lib/state/tracklistprefs";
import { toast } from "$lib/state/toast.svelte";
import { errText } from "./actions";
import { confirm, guard, namer } from "./confirm.svelte";
import { listMeta } from "./listmeta.svelte";
import { listPrefs } from "./listPrefs.svelte";
import { copyName, deleteLine, deleteSummary, exportSummary, planLine } from "./mixlist";
import { preview } from "./preview.svelte";
import { mv } from "./view.svelte";

/** Saves the open mix and refreshes the library-row cache (new modified date and play time). */
export async function saveMix() {
  const id = mixes.doc?.id;
  await mixes.save();
  if (id !== undefined) listMeta.afterSave(id, mixes.doc?.id === id ? mixes.doc.meta : null);
}

/** Opens a mix in the editor, after dealing with unsaved changes in the one now open. */
export function openMix(id: number) {
  guard(async () => {
    try {
      preview.stop();
      await mixes.open(id);
      mv.reset();
      listMeta.set(id, mixes.doc?.meta ?? null);
    } catch (e) {
      toast.show(`Could not open the mix: ${errText(e)}`, "bad", 9000);
    }
  });
}

/** Back to the list. */
export function closeMix() {
  guard(() => {
    preview.stop();
    mixes.close();
    void mixes.refreshList();
  });
}

export function newMix() {
  guard(() => {
    namer.req = {
      title: "New mix",
      label: "Name",
      initial: "",
      confirmLabel: "Create",
      run: async (name) => {
        preview.stop();
        await mixes.create(name);
        mv.reset();
        listMeta.set(mixes.doc?.id ?? -1, mixes.doc?.meta ?? null);
      },
    };
  });
}

export function renameMix(m: MixSummary) {
  namer.req = {
    title: "Rename mix",
    label: `New name for “${m.name}”`,
    initial: m.name,
    confirmLabel: "Rename",
    run: async (name) => {
      const root = mixes.root;
      if (!root) throw new Error("no library is open");
      if (name !== m.name) await mixRename(root, m.id, name);
      await reloadList();
      toast.show(`Renamed to “${name}”`, "ok");
    },
  };
}

/** Re-reads the mix list and the library-row facts shown beside it. */
export async function reloadList() {
  await mixes.refreshList();
  await listMeta.refresh();
}

/** Asks for a name, then makes a copy of the SAVED mix (document and row facts). `done` gets the
 * new mix's id so the list can pick it. */
export function duplicateMix(m: MixSummary, done?: (id: number) => void) {
  namer.req = {
    title: "Duplicate mix",
    label: `Name for the copy of “${m.name}”`,
    initial: copyName(mixes.list.map((x) => x.name), m.name),
    confirmLabel: "Duplicate",
    run: async (name) => {
      const root = mixes.root;
      if (!root) throw new Error("no library is open");
      const made = await mixDuplicate(root, m.id, name);
      await reloadList();
      done?.(made.id);
      toast.show(`Made “${made.name}” from “${m.name}”`, "ok");
    },
  };
}

/** Save As: the open mix, with its unsaved edits, becomes a NEW mix and the editor switches to
 * it. The mix it was made from keeps what was last saved. */
export function saveMixAs() {
  const doc = mixes.doc;
  if (!doc || mixes.busy) return;
  namer.req = {
    title: "Save as a new mix",
    label: `Name for the new mix (“${doc.name}” keeps its last saved content)`,
    initial: copyName(mixes.list.map((x) => x.name), doc.name),
    confirmLabel: "Save as",
    run: async (name) => {
      preview.stop();
      const saved = await mixes.saveAs(name);
      mv.reset();
      listMeta.set(saved.id, mixes.doc?.meta ?? null);
      await reloadList();
      toast.show(`Saved as “${saved.name}”. “${doc.name}” was not changed.`, "ok", 7000);
    },
  };
}

/** Deletes the mixes after a confirmation that lists every one. The backend keeps any mix that
 * was renamed since the list was drawn and says which. */
export function deleteMixes(ms: MixSummary[], done?: () => void) {
  if (ms.length === 0) return;
  const items = ms.map((m) => ({ id: m.id, name: m.name }));
  confirm.ask({
    title: ms.length === 1 ? "Delete mix" : `Delete ${ms.length} mixes`,
    message:
      ms.length === 1
        ? `Delete the mix “${ms[0].name}” from the library? This cannot be undone. The tracks in it are not deleted.`
        : `Delete these ${ms.length} mixes from the library? This cannot be undone. The tracks in them are not deleted.`,
    items: ms.map(deleteLine),
    confirmLabel: ms.length === 1 ? "Delete mix" : `Delete ${ms.length} mixes`,
    danger: true,
    run: async () => {
      const root = mixes.root;
      if (!root) throw new Error("no library is open");
      const r = await mixDeleteMany(root, items);
      for (const d of r.deleted) listMeta.forget(d.id);
      await reloadList();
      done?.();
      const s = deleteSummary(r);
      toast.show(s.text, s.ok ? "ok" : "bad", s.ok ? 4000 : 14000);
    },
  });
}

const PMK = [{ name: "Pacemaker mix", extensions: ["pmk"] }];

export async function importPmk() {
  const root = mixes.root;
  if (!root) return;
  try {
    const picked = await open({ multiple: false, directory: false, title: "Import a .pmk mix", filters: PMK });
    if (typeof picked !== "string") return;
    const id = await mixImportPmk(root, picked);
    await reloadList();
    const m = mixes.list.find((x) => x.id === id);
    toast.show(`Imported “${m?.name ?? "mix"}”`, "ok");
  } catch (e) {
    toast.show(`Import failed: ${errText(e)}`, "bad", 12000);
  }
}

/** Exports the SAVED mix as .pmk (the file on the device, not unsaved edits). */
export async function exportPmk(m: MixSummary) {
  const root = mixes.root;
  if (!root) return;
  try {
    const dest = await save({ title: "Export the mix as .pmk", defaultPath: `${m.name}.pmk`, filters: PMK });
    if (!dest) return;
    await mixExportPmk(root, m.id, dest);
    toast.show(`Exported “${m.name}”`, "ok");
  } catch (e) {
    toast.show(`Export failed: ${errText(e)}`, "bad", 9000);
  }
}

/** Exports several saved mixes as .pmk files into a folder the user picks. The confirmation lists
 * every file in words (new, replaces, skipped) from the same plan the export then runs. */
export async function exportMixes(ms: MixSummary[]) {
  const root = mixes.root;
  if (!root || ms.length === 0) return;
  try {
    const dir = await open({ multiple: false, directory: true, title: `Choose the folder for ${ms.length} exported mixes` });
    if (typeof dir !== "string") return;
    const plan = await mixExportPlan(ms.map((m) => ({ id: m.id, name: m.name })), dir, listPrefs.value.onExisting);
    const replacing = plan.filter((p) => p.action === "replace").length;
    const writing = plan.filter((p) => p.action !== "skip").length;
    confirm.ask({
      title: `Export ${ms.length} mixes`,
      message:
        `${writing} file${writing === 1 ? "" : "s"} will be written to ${dir}. ` +
        (replacing ? `${replacing} existing file${replacing === 1 ? "" : "s"} will be replaced. ` : "No existing file is replaced. ") +
        "The saved mixes are exported, not unsaved edits.",
      items: plan.map(planLine),
      confirmLabel: writing === 0 ? "Close" : `Export ${writing}`,
      danger: replacing > 0,
      run: async () => {
        if (writing === 0) return;
        const r = await mixExportMany(root, dir, plan);
        const s = exportSummary(r, dir);
        toast.show(s.text, s.ok ? "ok" : "bad", s.ok ? 6000 : 14000);
      },
    });
  } catch (e) {
    toast.show(`Export failed: ${errText(e)}`, "bad", 12000);
  }
}

/** Stops the running export. The render ends at its next block and leaves no file; the end is
 *  reported by the job-done event like any other (init.ts). */
export async function cancelExport() {
  if (!mv.exporting) return;
  try {
    await exportCancel();
    toast.show("Cancelling the export…", "info", 3000);
  } catch (e) {
    toast.show(`Could not cancel the export: ${errText(e)}`, "bad", 7000);
  }
}

/** Renders the open mix (including unsaved edits) to an audio file the user chooses: the part of
 *  the mix and the loudness settings are the export dialog's (state/mixexport.svelte.ts). */
export async function exportAudio() {
  const root = mixes.root;
  const doc = mixes.doc;
  if (!root || !doc || mv.exporting || mixExport.analysing) return;
  if (doc.clips.length === 0) {
    toast.show("There is nothing to export: add a track first.", "info");
    return;
  }
  mixExport.ensureLoaded();
  const extras = mixExport.extras();
  if (!extras) {
    toast.show("Fix the range in the export settings first.", "bad", 6000);
    return;
  }
  if (mixExport.problem) {
    toast.show(mixExport.problem, "bad", 8000);
    return;
  }
  const wanted = mixExport.prefs.format;
  let picked: string | null = null;
  try {
    picked = await save({
      title: mixExport.partial ? "Export part of the mix as audio" : "Export the mix as audio",
      defaultPath: `${doc.name}${mixExport.partial ? " (part)" : ""}.${extOf(wanted)}`,
      filters: saveFilters(wanted),
    });
  } catch (e) {
    toast.show(`Could not open the save dialog: ${errText(e)}`, "bad", 7000);
    return;
  }
  if (!picked) return;
  // a name with a known extension is that format (typed or picked in the dialog); any other name
  // gets the chosen format's extension, so the file is never a different format than its name says
  const { path: dest, format } = resolveDest(picked, wanted);
  tracklist.ensureLoaded();
  // What the save window did not ask about and an export would replace is asked here, once, in one
  // list: the audio file when the extension was added after the window closed (its overwrite prompt
  // never saw that name), and the .cue beside it.
  let found: Awaited<ReturnType<typeof exportCueTarget>> | null = null;
  try {
    if (needsDiskCheck(picked, dest, tracklist.writeCue)) found = await exportCueTarget(dest);
  } catch (e) {
    toast.show(`Export failed: ${errText(e)}`, "bad", 12000);
    return;
  }
  const { items, replaceCue } = replacements(picked, dest, format, tracklist.writeCue, tracklist.cueIfExists, found);
  if (items.length > 0) {
    confirm.ask({
      title: items.length > 1 ? "Replace existing files" : "Replace an existing file",
      message: "Exporting will replace what is listed once the render has finished.",
      items,
      confirmLabel: "Replace and export",
      run: () => startExport(root, doc, dest, format, replaceCue, extras),
    });
    return;
  }
  await startExport(root, doc, dest, format, replaceCue, extras);
}

async function startExport(root: string, doc: NonNullable<typeof mixes.doc>, dest: string, format: AudioFormat, replaceCue: boolean, extras: ExportExtras) {
  if (mv.exporting) return;
  mv.exporting = true;
  mv.exportPct = 0;
  mv.exportPhase = mixExport.active ? "measuring" : "rendering";
  toast.show(`${mixExport.active ? "Measuring and rendering" : "Rendering"} “${doc.name}” to ${format.toUpperCase()}…`, "info", 6000);
  try {
    // resolves when the job has started; init.ts reports the end from the job-done event
    const started = await exportMixAudio(root, doc, dest, format, tracklist.writeCue ? tracklist.snapshot() : undefined, replaceCue, extras);
    if (started.cuePath) {
      const name = baseName(started.cuePath);
      toast.show(started.cueReplaces ? `${name} will be replaced when the render finishes.` : `The CUE sheet ${name} is written when the render finishes.`, "info", 8000);
    }
  } catch (e) {
    mv.exporting = false;
    toast.show(`Export failed: ${errText(e)}`, "bad", 12000);
  }
}
