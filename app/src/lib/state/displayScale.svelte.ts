// The pixel ratio of the screen the window is on, kept current. Canvases that draw into an
// off-screen layer read it from here (inside their effect) so dragging the window to a screen
// with another scale, or changing the Windows display scale, rebuilds the layer at the right
// density even while nothing else about the view changes.

class DisplayScale {
  ratio = $state(typeof window === "undefined" ? 1 : window.devicePixelRatio || 1);
  #query: MediaQueryList | null = null;
  #onChange = () => {
    this.ratio = window.devicePixelRatio || 1;
    this.#watch();
  };

  constructor() {
    if (typeof window !== "undefined" && typeof window.matchMedia === "function") this.#watch();
  }

  // A resolution query only reports leaving the value it was made for, so each change is
  // followed by a new query for the new value.
  #watch() {
    this.#query?.removeEventListener("change", this.#onChange);
    this.#query = window.matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
    this.#query.addEventListener("change", this.#onChange);
  }
}

export const displayScale = new DisplayScale();
