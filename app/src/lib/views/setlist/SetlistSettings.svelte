<script lang="ts">
  // The setlist page's print options for the Settings view: the same parts the setlist dialog shows,
  // on the same state. The heading, event, venue and notes belong to each setlist, not here.
  import SetlistOptions from "./SetlistOptions.svelte";
  import { setlist } from "./setlist.svelte";

  // remember the print options
  $effect(() => {
    JSON.stringify($state.snapshot(setlist.options));
    setlist.persist();
  });
</script>

<section>
  <h4>Times</h4>
  <SetlistOptions part="time" />
</section>
<section>
  <h4>Columns</h4>
  <SetlistOptions part="columns" />
</section>
<section>
  <h4>Page</h4>
  <SetlistOptions part="page" />
</section>

<style>
  section {
    margin-top: 10px;
  }
  h4 {
    margin: 0 0 4px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
</style>
