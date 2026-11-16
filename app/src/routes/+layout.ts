// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

// Reads the saved interface size and applies it now, before the first view is built.
import "$lib/state/appearance.svelte";

// Outside the Tauri shell (browser preview during UI work) answer commands with a synthetic
// library; inside Tauri this branch never runs. The mock is only built in for `npm run dev` or a
// build made with VITE_DEVMOCK=1 (for `vite preview`): a release build leaves it out, so the
// installed app does not carry 200 KB it never loads.
if ((import.meta.env.DEV || import.meta.env.VITE_DEVMOCK) && typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window)) {
  const { installDevMock } = await import("$lib/devmock/index");
  installDevMock();
}
