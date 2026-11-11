// Where a popover opened from a toolbar button goes: under the button, its right edge on the
// button's, kept inside the window. (The toolbar wraps, so a button can sit anywhere along the
// row; a popover simply hanging off one side would run off the screen.)
export type PopoverPlace = { left: number; top: number; width: number; maxHeight: number };

export function popoverPlace(anchor: HTMLElement, wanted: number): PopoverPlace {
  const r = anchor.getBoundingClientRect();
  const width = Math.min(wanted, window.innerWidth - 16);
  const left = Math.max(8, Math.min(r.right - width, window.innerWidth - width - 8));
  const top = r.bottom + 4;
  return { left, top, width, maxHeight: Math.max(160, window.innerHeight - top - 12) };
}
