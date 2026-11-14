<script lang="ts">
  import { libraryLabel } from "$lib/state/place";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { errorScope, type ErrorSeen } from "./bannerLogic";

  // A message stays while the view it came from is shown, goes when the user moves to another view,
  // and can be closed by hand. (Plain variable, not state: the effect must not re-run on its own write.)
  let seen: ErrorSeen = { text: "", view: "" };
  $effect(() => {
    const r = errorScope(seen, session.error, nav.view);
    seen = r.next;
    if (r.clear) session.error = "";
  });

  const label = $derived(session.library ? libraryLabel(session.library.kind, session.library.root, session.computer?.path ?? null) : "");
</script>

{#if session.computer?.warning}
  <div class="banner bad" role="alert">
    The settings for the library on this computer could not be used, so no library was opened by itself and the default
    location is shown until you save the settings again.
    <div class="detail">{session.computer.warning}</div>
  </div>
{/if}

{#if session.error}
  <div class="banner bad row" role="alert">
    <span class="text">{session.error}</span>
    <Button small variant="ghost" onclick={() => (session.error = "")} aria-label="Dismiss this message" title="Dismiss this message">Dismiss</Button>
  </div>
{/if}

{#if session.library}
  {#if session.isDetached}
    <div class="banner warn">{label} · {session.library.root} · ejected: it is safe to unplug. Nothing can be written to it any more. To keep working, open the library on this computer, dock a Pacemaker and press Rescan, or open a folder.</div>
  {:else if session.library.problems.length === 0}
    <div class="banner ok">{label} · {session.library.root} · music.db is device-compatible</div>
  {:else}
    <div class="banner bad">
      {session.isComputer ? "This library is not in the format the Pacemaker needs, so it stays read-only here:" : "This library would not load on the device:"}
      <ul>
        {#each session.library.problems as p}<li>{p}</li>{/each}
      </ul>
    </div>
  {/if}
  {#if session.switching}
    <div class="banner reading" role="status">Reading the other library… This one stays open until it is ready.</div>
  {/if}
{/if}

<style>
  .banner {
    padding: 6px 14px;
    font-size: 12px;
    border-bottom: 1px solid var(--line);
  }
  .ok {
    color: var(--muted);
  }
  .ok::before {
    content: "●  ";
    color: var(--ok);
  }
  .bad {
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .warn {
    color: var(--text);
  }
  .warn::before {
    content: "●  ";
    color: var(--muted);
  }
  .reading {
    color: var(--muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .text {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .detail {
    margin-top: 2px;
    overflow-wrap: anywhere;
    opacity: 0.85;
  }
  ul {
    margin: 4px 0 0;
  }
</style>
