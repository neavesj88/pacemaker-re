<script lang="ts">
  // The "Pacemaker connected" popup (picked up by shell/overlays.ts): small, bottom right, any view,
  // never modal. Opening the Pacemaker needs the click on "Open it"; the check behind it only lists
  // the connected drives (state/connect.svelte.ts).
  import { connect } from "$lib/state/connect.svelte";
  import { driveText } from "$lib/state/connectLogic";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";

  const drive = $derived(connect.shown);
  // reset for each drive that is asked about
  let never = $state(false);
  $effect(() => {
    void drive;
    never = false;
  });
</script>

{#if drive}
  <div class="pop" role="status" aria-label="Pacemaker connected">
    <svg class="icon" viewBox="0 0 24 24" width="28" height="28" aria-hidden="true">
      <rect x="3" y="7" width="18" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.6" />
      <circle cx="17" cy="12" r="1.2" fill="currentColor" />
      <path d="M6.5 12h6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
    </svg>
    <div class="text">
      <b>Pacemaker connected</b>
      <span class="muted">{driveText(drive)}</span>
    </div>
    <div class="buttons">
      <Button small disabled={connect.busy} onclick={() => connect.notNow(drive, never)}>Not now</Button>
      <Button small variant="primary" disabled={connect.busy || session.loading} onclick={() => void connect.open(drive, never)}>Open it</Button>
    </div>
    <label class="never"><input type="checkbox" bind:checked={never} /> Don't ask again</label>
  </div>
{/if}

<style>
  .pop {
    position: fixed;
    right: 14px;
    /* above the player bar and one message of the stack (ui/Toast.svelte sits right on the bar) */
    bottom: calc(var(--player-h, 48px) + 60px);
    z-index: 49;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 6px 12px;
    width: max-content;
    max-width: min(380px, calc(100vw - 28px));
    padding: 10px 12px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .icon {
    color: var(--accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .buttons {
    display: flex;
    gap: 6px;
  }
  .never {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
</style>
