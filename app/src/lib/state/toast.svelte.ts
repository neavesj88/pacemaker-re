// Transient messages: `toast.show("Saved")`. Rendered by ui/Toast.svelte in the shell.
export type ToastKind = "info" | "ok" | "bad";
export type ToastItem = { id: number; text: string; kind: ToastKind };

class Toasts {
  items = $state<ToastItem[]>([]);
  #next = 1;

  show(text: string, kind: ToastKind = "info", ms = 4000) {
    const id = this.#next++;
    this.items.push({ id, text, kind });
    setTimeout(() => this.dismiss(id), ms);
  }

  dismiss(id: number) {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toast = new Toasts();
