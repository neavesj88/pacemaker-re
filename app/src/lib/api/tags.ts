// tags commands (src-tauri/src/commands/tags.rs): tags, colour labels and energy ratings. They
// are kept by the app (a JSON file per library in the app config dir), never written to the
// device. Labels are keyed by the track's stable id on the backend; here everything is by track_id.
import { invoke } from "@tauri-apps/api/core";
import type { EnergySource } from "$lib/state/energyResolve";
import type { LabelColour, LabelOp } from "$lib/state/labels";

/** The labels of one track as the backend returns them. */
export type TrackLabelsRow = { track_id: number; tags: string[]; colour: LabelColour | null; energy: number | null };

/** The labels of a track that is no longer on the card; its labels are all there is to name it by. */
export type OrphanLabels = { tags: string[]; colour: LabelColour | null; energy: number | null };

export type TagsView = {
  /** Only tracks that have any label. */
  tracks: TrackLabelsRow[];
  /** Labelled things in the file that match no track on this card (deleted and re-added tracks). */
  orphans: number;
  /** Exactly the things "forget" removes, so the confirmation can list them. */
  orphan_labels: OrphanLabels[];
  /** Set when the file could not be read (tags then start empty). */
  notice: string | null;
};

export const loadTags = (root: string) => invoke<TagsView>("tags_load", { root });

/** Applies the ops to every listed track, all or nothing; resolves to each track's labels afterwards. */
export const applyTags = (root: string, trackIds: number[], ops: LabelOp[]) =>
  invoke<TrackLabelsRow[]>("tags_apply", { root, trackIds, ops });

/** Renames a tag everywhere; onto an existing tag it merges the two. */
export const renameTag = (root: string, from: string, to: string) => invoke<TagsView>("tags_rename_tag", { root, from, to });

/** Removes the labels of tracks that are no longer on the card. `expected` is how many were shown;
 *  if the card changed since, nothing is removed and the call fails. */
export const forgetUnattachedLabels = (root: string, expected: number) =>
  invoke<TagsView>("tags_forget_unattached", { root, expected });

export const deleteTag = (root: string, tag: string) => invoke<TagsView>("tags_delete_tag", { root, tag });

// ---- cases and mixes: tags and colour (energy is a track rating only) ----

/** Which list the labels belong to. */
export type ThingKind = "case" | "mix";

/** The tags and colour of one case or mix. */
export type ThingLabelsRow = { id: number; tags: string[]; colour: LabelColour | null };

export type ThingsView = {
  /** Only the cases (or mixes) that have any label. */
  things: ThingLabelsRow[];
  /** Labelled cases (or mixes) in the file that no longer exist. */
  orphans: number;
  notice: string | null;
};

export const loadThingLabels = (root: string, which: ThingKind) => invoke<ThingsView>("tags_things_load", { root, which });

/** Applies tag and colour changes to every listed case (or mix), all or nothing. Not a device write. */
export const applyThingLabels = (root: string, which: ThingKind, ids: number[], ops: LabelOp[]) =>
  invoke<ThingLabelsRow[]>("tags_things_apply", { root, which, ids, ops });

// ---- the "Energy source" setting ----

export type EnergySourceView = { source: EnergySource; problem: string | null };

export const energySourceGet = () => invoke<EnergySourceView>("energy_source_get");
export const energySourceSet = (source: EnergySource) => invoke<EnergySourceView>("energy_source_set", { source });
