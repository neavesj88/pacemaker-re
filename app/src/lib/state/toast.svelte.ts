// Transient messages: `toast.show("Saved")`, `toast.error("Couldn't save the mix.", e)`. Rendered by
// ui/Toast.svelte in the shell. The words follow the rule in toastText.ts (one short sentence); what
// does not fit goes in `details`, shown behind the card's Details button. A message about a failure
// stays long enough to read (a setting, see appearanceLogic.toastMs) and can be closed. A card stays
// while the pointer or focus is on it.
import { appearance } from "./appearance.svelte";
import { addToast, toastMs } from "./appearanceLogic";
import { autoMs, condense, errorToast, TOAST_HARD, wordCount } from "./toastText";

export type ToastKind = "info" | "ok" | "bad";
export type ToastAction = { label: string; run: () => void };
export type ToastOpts = {
  /** The long form: names, numbers, the whole error. Shown behind a Details button. */
  details?: string;
  /** One button on the card (Undo, Open ...). It closes the card after it runs. */
  action?: ToastAction;
  /** A card with the same key is replaced instead of stacking (a count that changes with each add). */
  key?: string;
};
export type ToastItem = { id: number; text: string; kind: ToastKind; count: number; details?: string; action?: ToastAction; key?: string };

class Toasts {
  items = $state<ToastItem[]>([]);
  #next = 1;
  #timers = new Map<number, ReturnType<typeof setTimeout>>();
  #stay = new Map<number, number | null>();

  show(text: string, kind: ToastKind = "info", opts: ToastOpts = {}): number {
    if (import.meta.env?.DEV && wordCount(text) > TOAST_HARD) console.warn(`[toast] ${wordCount(text)} words, over the ${TOAST_HARD} limit: ${text}`);
    // text built at run time (a Rust summary) that is still too long: the first sentence, the rest in Details
    const c = condense(text);
    const details = opts.details ?? c.details;
    const id = this.#next++;
    this.items = addToast(this.items, { id, text: c.text, kind, details, action: opts.action, key: opts.key });
    const stay = toastMs(kind, autoMs(c.text), appearance.errorSecs);
    this.#stay.set(id, stay);
    this.release(id);
    // the card kept by a same-text replace is the new id; the old one is gone from the list
    for (const old of [...this.#stay.keys()]) if (!this.items.some((t) => t.id === old)) this.#forget(old);
    return id;
  }

  /** A failure: `head` says what could not be done, the first clause of `e` says why, the rest is Details. */
  error(head: string, e: unknown, opts: ToastOpts = {}): number {
    const r = errorToast(head, e);
    return this.show(r.text, "bad", { details: r.details, ...opts });
  }

  /** Stops the clock (the pointer is on the card, or its details are open). */
  hold(id: number) {
    const t = this.#timers.get(id);
    if (t !== undefined) clearTimeout(t);
    this.#timers.delete(id);
  }

  /** Starts the clock again, a whole stay from now. */
  release(id: number) {
    this.hold(id);
    const stay = this.#stay.get(id);
    if (stay === null || stay === undefined) return;
    this.#timers.set(id, setTimeout(() => this.dismiss(id), stay));
  }

  dismiss(id: number) {
    this.items = this.items.filter((t) => t.id !== id);
    this.#forget(id);
  }

  #forget(id: number) {
    this.hold(id);
    this.#stay.delete(id);
  }
}

export const toast = new Toasts();
