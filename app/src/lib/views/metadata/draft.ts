// Pure helpers of the metadata editor: form values as strings, validation against the schema
// limits, and the diff that decides what is sent ("only changed fields are written").
import {
  MAX_SMALL_NUMBER,
  MAX_TEXT_CHARS,
  NUMBER_FIELDS,
  TEXT_FIELDS,
  type NumberField,
  type TextField,
  type TrackDetail,
  type TrackEdit,
} from "$lib/api/metadata";

export type FieldKey = TextField | NumberField;
export type Draft = Record<FieldKey, string> & { rating: number };

export const FIELD_LABELS: Record<FieldKey, string> = {
  title: "Title",
  artist: "Artist",
  album_artist: "Album artist",
  album: "Album",
  composer: "Composer",
  genre: "Genre",
  label: "Label",
  remixer: "Remixer",
  producer: "Producer",
  comments: "Comments",
  year: "Year",
  track_number: "Track number",
  number_of_tracks: "Tracks in album",
  disc_number: "Disc number",
  number_of_discs: "Discs in set",
};

const isNumberField = (k: FieldKey): k is NumberField => (NUMBER_FIELDS as readonly string[]).includes(k);

/** 0 means "not set" in the device schema, so it shows as an empty box. */
const show = (k: FieldKey, d: TrackDetail): string => {
  const v = d[k];
  return isNumberField(k) ? (v ? String(v) : "") : String(v);
};

export const toDraft = (d: TrackDetail): Draft => {
  const out = { rating: d.rating } as Draft;
  for (const k of [...TEXT_FIELDS, ...NUMBER_FIELDS]) out[k] = show(k, d);
  return out;
};

/** Multi-edit start state: a field shows the shared value, or an empty box when the tracks
 *  differ (those keys are returned in `mixed` so the box can say "Multiple values"). */
export const commonDraft = (details: TrackDetail[]): { draft: Draft; mixed: Set<FieldKey | "rating"> } => {
  const first = toDraft(details[0]);
  const mixed = new Set<FieldKey | "rating">();
  const rest = details.slice(1).map(toDraft);
  for (const k of [...TEXT_FIELDS, ...NUMBER_FIELDS] as FieldKey[]) {
    if (rest.some((d) => d[k] !== first[k])) {
      mixed.add(k);
      first[k] = "";
    }
  }
  if (rest.some((d) => d.rating !== first.rating)) {
    mixed.add("rating");
    first.rating = 0;
  }
  return { draft: first, mixed };
};

/** A problem with one field's text, or "" when it is fine. */
export const fieldError = (k: FieldKey, v: string): string => {
  if (isNumberField(k)) {
    if (v === "") return "";
    if (!/^\d+$/.test(v) || Number(v) > MAX_SMALL_NUMBER) return `${FIELD_LABELS[k]} must be a whole number from 0 to ${MAX_SMALL_NUMBER}`;
    return "";
  }
  if ([...v].length > MAX_TEXT_CHARS) return `${FIELD_LABELS[k]} is longer than ${MAX_TEXT_CHARS} characters`;
  if (v.includes("\0")) return `${FIELD_LABELS[k]} contains a NUL character`;
  return "";
};

export const draftErrors = (d: Draft, only?: Iterable<FieldKey>): string[] => {
  const keys = only ? [...only] : ([...TEXT_FIELDS, ...NUMBER_FIELDS] as FieldKey[]);
  return keys.map((k) => fieldError(k, d[k])).filter(Boolean);
};

/** The typed value of a field box (empty number = 0, unset). Call only when fieldError is "". */
const parse = (k: FieldKey, v: string): string | number => (isNumberField(k) ? (v === "" ? 0 : Number(v)) : v);

/** Single track: the fields of `draft` that differ from the stored track. */
export const diffSingle = (d: TrackDetail, draft: Draft): TrackEdit => {
  const edit: Record<string, string | number> = {};
  for (const k of [...TEXT_FIELDS, ...NUMBER_FIELDS] as FieldKey[]) {
    const v = parse(k, draft[k]);
    if (v !== d[k]) edit[k] = v;
  }
  if (draft.rating !== d.rating) edit.rating = draft.rating;
  return edit as TrackEdit;
};

/** Several tracks: exactly the fields the user touched, with the value typed. */
export const editFromTouched = (draft: Draft, touched: Iterable<FieldKey | "rating">): TrackEdit => {
  const edit: Record<string, string | number> = {};
  for (const k of touched) edit[k] = k === "rating" ? draft.rating : parse(k, draft[k]);
  return edit as TrackEdit;
};

export const isEmptyEdit = (e: TrackEdit): boolean => Object.keys(e).length === 0;
