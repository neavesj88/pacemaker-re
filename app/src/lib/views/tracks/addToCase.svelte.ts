// The "Add to case" picker's open state. The table's context menu and the inspector call
// `addToCase.ask(ids)`; Overlays.svelte shows the picker.
class AddToCase {
  ids = $state.raw<number[] | null>(null);

  ask(ids: number[]) {
    if (ids.length) this.ids = [...ids];
  }

  close() {
    this.ids = null;
  }
}

export const addToCase = new AddToCase();
