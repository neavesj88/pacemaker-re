<script lang="ts">
  import { mixList, type MixSummary } from "$lib/api/mixes";
  import { session } from "$lib/state/session.svelte";
  import CollectionTable from "$lib/ui/CollectionTable.svelte";

  // Read-only list of the mixes on the device, from the mixes area's own command. The mix
  // editor replaces this.
  let rows = $state<MixSummary[]>([]);

  $effect(() => {
    const lib = session.library; // re-read after a reload
    if (!lib) return;
    mixList(lib.root).then((m) => (rows = m)).catch((e) => (session.error = String(e)));
  });
</script>

<CollectionTable {rows} noun="mixes" />
