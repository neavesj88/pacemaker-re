// The track metadata editor (single or multiple tracks). Cross-area contract: any view opens it
// with metadataEditor.edit(ids); the metadata area renders the editor and saves.
class MetadataEditor {
  ids = $state<number[] | null>(null);

  edit(ids: number[]) {
    this.ids = ids.length ? ids : null;
  }

  close() {
    this.ids = null;
  }
}

export const metadataEditor = new MetadataEditor();
