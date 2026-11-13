// A Svelte action for a hand-built dialog: when it appears, focus goes to the first element marked
// `data-autofocus` (put it on the safe choice), else to the dialog itself, as `ui/Modal.svelte`
// does. Use it as `<div class="dlg" role="dialog" tabindex="-1" use:focusFirst>`.
export function focusFirst(node: HTMLElement) {
  (node.querySelector<HTMLElement>("[data-autofocus]") ?? node).focus();
}
