<script lang="ts">
  import { libraryKind, libraryLabel, sameRoot } from "$lib/state/place";
  import { session } from "$lib/state/session.svelte";

  // The screen while a library is being read and none is open yet (app start, or the first library
  // chosen). A Pacemaker's hard drive has to read every .str, which can take a while.
  const what = $derived.by(() => {
    const root = session.loadingRoot;
    if (!root) return "Looking for your library…";
    const kind = libraryKind(root);
    if (kind === "computer" && sameRoot(root, session.computer?.path ?? "")) return "Reading the library on this computer…";
    return `Reading ${libraryLabel(kind, root, session.computer?.path ?? null)}…`;
  });
</script>

<div class="loading" role="status" aria-live="polite">
  <div class="spin" aria-hidden="true"></div>
  <p>{what}</p>
  <p class="muted">A Pacemaker with a hard drive can take a minute to read.</p>
</div>

<style>
  .loading {
    margin: auto;
    text-align: center;
    color: var(--muted);
  }
  p {
    margin: 6px 0;
  }
  .loading p:first-of-type {
    color: var(--text);
  }
  .spin {
    width: 28px;
    height: 28px;
    margin: 0 auto 12px;
    border: 3px solid var(--line-hi);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: turn 0.9s linear infinite;
  }
  @keyframes turn {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation-duration: 3s;
    }
  }
</style>
