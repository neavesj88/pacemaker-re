<script lang="ts">
  // Writes the remembered view (search, filters, sort of the open library) to the app-side views
  // file as it changes. Always mounted (see Overlays.svelte) so a change made while another view
  // is showing, such as the search box used in Cases, is remembered too. Renders nothing.
  import { onMount } from "svelte";
  import { tracksState } from "./tracksState.svelte";

  // the reads inside noteChange are what this effect depends on
  $effect(() => {
    tracksState.noteChange();
  });

  // leaving the app or the page writes a change that is still waiting
  onMount(() => {
    const flush = () => void tracksState.flushViews();
    window.addEventListener("pagehide", flush);
    return () => window.removeEventListener("pagehide", flush);
  });
</script>
