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
  /** Bumped by every openHelp(id): the reader then scrolls to the anchor (or the top) even when the article and anchor are the same as before. */
  jump = $state(0);
  /** The `jump` the reader last acted on, so coming back to Help later restores the scroll instead of repeating the jump. */
  appliedJump = 0;
  /** Where the reader was scrolled to, kept for when Help is left and entered again. Not reactive. */
  scrollTop = 0;
  /** Bumped to move the focus to the search box (the view is already open, or opens just now). */
  focusTick = $state(0);
}

export const help = new Help();

/**
 * Keeps `returnTo` at the view shown just before Help, whichever way Help was entered (F1, a "?" link or
 * the sidebar). Started once by the shell (init.ts); the returned function stops it.
 */
export function trackReturnView(): () => void {
  return $effect.root(() => {
    $effect(() => {
      if (nav.view !== "help") help.returnTo = nav.view;
    });
  });
}

/** Opens Help, at an article when `id` is given (and at one of its headings with `anchor`). */
export function openHelp(id?: string, opts: { anchor?: string | null; focusSearch?: boolean } = {}) {
  if (nav.view !== "help") help.returnTo = nav.view;
  if (id !== undefined) {
    help.articleId = id;
    help.anchor = opts.anchor ?? null;
    help.jump++;
  }
  if (opts.focusSearch || id === undefined) help.focusTick++;
  nav.go("help");
}
