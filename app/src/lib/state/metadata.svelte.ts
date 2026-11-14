// The track metadata editor (single or multiple tracks). Cross-area contract: any view opens it
// with metadataEditor.edit(ids); the metadata area renders the editor and saves.
class MetadataEditor {
  ids = $state<number[] | null>(null);
  /** The track ids in the order of the table the editor was opened from: Previous / Next and
   *  "Track N of M" follow it. Null: the library's own order. */
  order = $state<number[] | null>(null);

  edit(ids: number[], order: number[] | null = null) {
    this.ids = ids.length ? ids : null;
    this.order = this.ids && order ? [...order] : null;
  }

  close() {
    this.ids = null;
    this.order = null;
  }
}

export const metadataEditor = new MetadataEditor();
