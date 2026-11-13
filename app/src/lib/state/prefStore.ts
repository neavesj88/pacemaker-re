// Per-viewer preferences kept in the browser's storage (column choices, last-used dialog options,
// remembered thresholds). One place for the two rules every one of them follows: reading never
// throws and always yields something usable, and a failed write is not an error, because the
// choice still applies to this run and only the memory of it is lost (a private window, blocked
// or full storage).
//
// Settings that other code reads outside the browser (smart-case rules, the backend) are not
// kept here: they live in the app's config folder through the backend (`src-tauri/src/store.rs`).

/**
 * Reads the JSON saved under `key` and returns what `normalise` makes of it. `normalise` gets
 * `null` when nothing is saved and must return the defaults then; anything that throws (storage
 * unavailable, text that is not JSON, a `normalise` that rejects the value) gives `fallback()`.
 */
export function loadPref<T>(key: string, normalise: (raw: unknown) => T, fallback: () => T): T {
  try {
    const text = localStorage.getItem(key);
    return normalise(text ? JSON.parse(text) : null);
  } catch {
    return fallback();
  }
}

/** Saves `value` as JSON under `key`. False when the browser would not keep it. */
export function savePref(key: string, value: unknown): boolean {
  try {
    localStorage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}
