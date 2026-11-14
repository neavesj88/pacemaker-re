// Transient messages: `toast.show("Saved")`. Rendered by ui/Toast.svelte in the shell. A message
// about a failure stays long enough to read (a setting, see appearanceLogic.toastMs) and can be closed.
import { appearance } from "./appearance.svelte";
import { MAX_TOASTS, toastMs } from "./appearanceLogic";

export type ToastKind = "info" | "ok" | "bad";
export type ToastItem = { id: number; text: string; kind: ToastKind };

class Toasts {
  items = $state<ToastItem[]>([]);
  #next = 1;

  show(text: string, kind: ToastKind = "info", ms = 4000): number {
    const id = this.#next++;
    this.items.push({ id, text, kind });
    if (this.items.length > MAX_TOASTS) this.items = this.items.slice(-MAX_TOASTS);
    const stay = toastMs(kind, ms, appearance.errorSecs);
    if (stay !== null) setTimeout(() => this.dismiss(id), stay);
    return id;
  }

  dismiss(id: number) {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toast = new Toasts();
