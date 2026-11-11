import { mixes } from "$lib/state/mixes.svelte";

// One confirmation dialog for the whole mix editor (rendered by ConfirmDialog.svelte). Anything
// destructive asks through here and names every affected item in words.
export type ConfirmRequest = {
  title: string;
  message: string;
  /** Every thing that will be affected, one line each. */
  items?: string[];
  confirmLabel: string;
  danger?: boolean;
  run: () => void | Promise<void>;
};

class Confirm {
  req = $state.raw<ConfirmRequest | null>(null);

  ask(r: ConfirmRequest) {
    this.req = r;
  }

  close() {
    this.req = null;
  }
}

export const confirm = new Confirm();

class Unsaved {
  /** What to do once the user has dealt with the unsaved changes; null when no prompt is open. */
  next = $state.raw<(() => void | Promise<void>) | null>(null);
}

export const unsaved = new Unsaved();

/** Runs `next` now if the open mix has no unsaved changes, else asks first (save / discard / cancel). */
export function guard(next: () => void | Promise<void>) {
  if (mixes.doc && mixes.dirty) unsaved.next = next;
  else void next();
}

export type NameRequest = {
  title: string;
  label: string;
  initial: string;
  confirmLabel: string;
  /** Throws a message to keep the dialog open (e.g. "a mix with that name already exists"). */
  run: (name: string) => void | Promise<void>;
};

class Namer {
  req = $state.raw<NameRequest | null>(null);
}

export const namer = new Namer();
