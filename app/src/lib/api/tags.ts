// tags commands (src-tauri/src/commands/tags.rs): tags, colour labels and energy ratings. They
// are kept by the app (a JSON file per library in the app config dir), never written to the
// device. Labels are keyed by the track's stable id on the backend; here everything is by track_id.
import { invoke } from "@tauri-apps/api/core";
import type { LabelColour, LabelOp } from "$lib/state/labels";

/** The labels of one track as the backend returns them. */
export type TrackLabelsRow = { track_id: number; tags: string[]; colour: LabelColour | null; energy: number | null };

export type TagsView = {
  /** Only tracks that have any label. */
  tracks: TrackLabelsRow[];
  /** Labelled things in the file that match no track on this card (deleted and re-added tracks). */
  orphans: number;
  /** Set when the file could not be read (tags then start empty). */
  notice: string | null;
};

export const loadTags = (root: string) => invoke<TagsView>("tags_load", { root });

/** Applies the ops to every listed track, all or nothing; resolves to each track's labels afterwards. */
export const applyTags = (root: string, trackIds: number[], ops: LabelOp[]) =>
  invoke<TrackLabelsRow[]>("tags_apply", { root, trackIds, ops });

/** Renames a tag everywhere; onto an existing tag it merges the two. */
export const renameTag = (root: string, from: string, to: string) => invoke<TagsView>("tags_rename_tag", { root, from, to });

export const deleteTag = (root: string, tag: string) => invoke<TagsView>("tags_delete_tag", { root, tag });
