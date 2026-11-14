// Which view is showing, plus a one-shot parameter bag for deep links. Shared; views switch with
// `nav.go(id)` or `nav.go(id, { trackId: 7 })`. Each area reads only its own keys from
// `nav.params` (name them after the area, e.g. `grid.trackId`) and clears what it consumed.
import type { ViewId } from "$lib/views/ids";

class Nav {
  view = $state<ViewId>("tracks");
  params = $state<Record<string, unknown>>({});
  /** Set by a view that can hold unsaved edits (the Cases store): called when something asks to show
   *  a different view, and it runs `proceed` at once or after asking the user. Not called for a
   *  deep link into the view already showing. */
  beforeLeave: ((proceed: () => void) => void) | null = null;

  go(view: ViewId, params: Record<string, unknown> = {}) {
    const show = () => {
      this.params = params;
      this.view = view;
    };
    if (view !== this.view && this.beforeLeave) this.beforeLeave(show);
    else show();
  }

  /** Reads and removes one param, so a deep link is acted on once. */
  take<T>(key: string): T | undefined {
    const v = this.params[key] as T | undefined;
    if (key in this.params) {
      const { [key]: _gone, ...rest } = this.params;
      this.params = rest;
    }
    return v;
  }
}

export const nav = new Nav();
