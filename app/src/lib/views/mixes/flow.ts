// Library-level actions of the mix editor: new, open, rename, delete, import / export .pmk,
// export audio. Each asks the user the way it must (unsaved changes, naming what is deleted,
// file dialogs) and reports the outcome with a toast.
import { open, save } from "@tauri-apps/plugin-dialog";
import { exportCancel, exportCueTarget, exportMixAudio, type AudioFormat, type ExportExtras } from "$lib/api/export";
import { extOf, needsDiskCheck, replacements, resolveDest, saveFilters } from "$lib/state/exportprefs";
import { mixDelete, mixExportPmk, mixImportPmk, mixRename, type MixSummary } from "$lib/api/mixes";
import { mixExport } from "$lib/state/mixexport.svelte";
import { mixes } from "$lib/state/mixes.svelte";
import { tracklist } from "$lib/state/tracklist.svelte";
import { baseName } from "$lib/state/tracklistprefs";
import { toast } from "$lib/state/toast.svelte";
import { errText } from "./actions";
import { confirm, guard, namer } from "./confirm.svelte";
import { mmss } from "./layout";
import { listMeta } from "./listmeta.svelte";
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
      await mixes.refreshList();
      toast.show(`Renamed to “${name}”`, "ok");
    },
  };
}

export function deleteMix(m: MixSummary) {
  confirm.ask({
    title: "Delete mix",
    message: `Delete the mix “${m.name}” from the library? This cannot be undone. The tracks in it are not deleted.`,
    items: [`“${m.name}”: ${m.track_count} track${m.track_count === 1 ? "" : "s"}, ${mmss(m.play_time_secs)}`],
    confirmLabel: "Delete mix",
    danger: true,
    run: async () => {
      const root = mixes.root;
      if (!root) throw new Error("no library is open");
      await mixDelete(root, m.id);
      listMeta.forget(m.id);
      await mixes.refreshList();
      toast.show(`Deleted “${m.name}”`, "ok");
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
    await mixes.refreshList();
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
