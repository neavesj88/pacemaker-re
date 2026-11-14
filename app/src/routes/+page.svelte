<script lang="ts">
  // The shell: header, sidebar, the active view and the player slot. No feature logic here;
  // each area lives under $lib/views/<area>. See app/ARCHITECTURE.md.
  import { onMount } from "svelte";
  import Banners from "$lib/shell/Banners.svelte";
  import Header from "$lib/shell/Header.svelte";
  import LeaveDialog from "$lib/shell/LeaveDialog.svelte";
  import Loading from "$lib/shell/Loading.svelte";
  import Sidebar from "$lib/shell/Sidebar.svelte";
  import Welcome from "$lib/shell/Welcome.svelte";
  import Player from "$lib/player/Player.svelte";
  import { overlays } from "$lib/shell/overlays";
  import { startAll } from "$lib/shell/startup";
  import { nav } from "$lib/state/nav.svelte";
  import { appearance } from "$lib/state/appearance.svelte";
  import { SCALE_DEFAULT, scaleKey } from "$lib/state/appearanceLogic";
  import { toast } from "$lib/state/toast.svelte";
  import { session } from "$lib/state/session.svelte";
  import Toast from "$lib/ui/Toast.svelte";
  import JobStrip from "$lib/shell/JobStrip.svelte";
  import { viewById } from "$lib/views/registry";

  onMount(() => {
    void session.start();
    return startAll();
  });

  // Ctrl + / Ctrl - / Ctrl 0 change the interface size (Settings > Appearance) wherever the focus is
  let sizeToast = 0;
  function onkeydown(e: KeyboardEvent) {
    const act = scaleKey(e);
    if (!act) return;
    e.preventDefault();
    if (act === "reset") appearance.reset();
    else appearance.step(act === "bigger" ? 1 : -1);
    // the whole window changes size, so say what it is now and how to get back; one message, replaced
    toast.dismiss(sizeToast);
    sizeToast = toast.show(`Interface size ${appearance.scale}%. Ctrl+0 sets it back to ${SCALE_DEFAULT}%.`, "info", 2500);
  }

  let Active = $derived(viewById(nav.view).component);
</script>

<svelte:window {onkeydown} />

<div class="app">
  <Header />
  <Banners />
  {#if session.library}
    <JobStrip />
    <div class="body">
      <Sidebar />
      <main><Active /></main>
    </div>
    <Player />
  {:else if session.loading}
    <Loading />
  {:else}
    <Welcome />
  {/if}
</div>

{#each overlays as Overlay}<Overlay />{/each}
<LeaveDialog />
<Toast />

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  main {
    flex: 1;
    overflow: auto;
  }
</style>
