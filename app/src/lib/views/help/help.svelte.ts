// What the Help view shows: the search text, the open article and where "Back" goes. A view opens
// an article with `openHelp("article-id")` (the "?" links do), F1 opens Help wherever you are.
import { nav } from "$lib/state/nav.svelte";

class Help {
  query = $state("");
  /** The article in the reader, or null for the start page. */
  articleId = $state<string | null>(null);
  /** A heading of the open article to scroll to (its slug), consumed by the reader. */
  anchor = $state<string | null>(null);
  /** The view Back returns to: the one Help was opened from. */
  returnTo = $state("tracks");
  /** Bumped to move the focus to the search box (the view is already open, or opens just now). */
  focusTick = $state(0);
}

export const help = new Help();

/** Opens Help, at an article when `id` is given (and at one of its headings with `anchor`). */
export function openHelp(id?: string, opts: { anchor?: string | null; focusSearch?: boolean } = {}) {
  if (nav.view !== "help") help.returnTo = nav.view;
  if (id !== undefined) {
    help.articleId = id;
    help.anchor = opts.anchor ?? null;
  }
  if (opts.focusSearch || id === undefined) help.focusTick++;
  nav.go("help");
}
