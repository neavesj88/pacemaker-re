<script lang="ts">
  // The shell: header, sidebar, the active view and the player slot. No feature logic here;
  // each area lives under $lib/views/<area>. See app/ARCHITECTURE.md.
  import { onMount } from "svelte";
  import Banners from "$lib/shell/Banners.svelte";
  import Header from "$lib/shell/Header.svelte";
  import Sidebar from "$lib/shell/Sidebar.svelte";
  import Welcome from "$lib/shell/Welcome.svelte";
  import Player from "$lib/player/Player.svelte";
  import { overlays } from "$lib/shell/overlays";
  import { startAll } from "$lib/shell/startup";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import Toast from "$lib/ui/Toast.svelte";
  import JobStrip from "$lib/shell/JobStrip.svelte";
  import { viewById } from "$lib/views/registry";

  onMount(() => {
    void session.start();
    return startAll();
  });

  let Active = $derived(viewById(nav.view).component);
</script>

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
  {:else if !session.loading}
    <Welcome />
  {/if}
</div>

{#each overlays as Overlay}<Overlay />{/each}
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
