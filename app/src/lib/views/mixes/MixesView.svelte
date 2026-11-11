<script lang="ts">
  // Mixes: the list of mixes on the device, and the editor for the one that is open. This file
  // only switches between them and receives deep links; the work is in the components beside it.
  //
  // Deep link: the tracks view opens this view with `nav.go("mixes", { addTracks: [trackIds] })`.
  // The tracks are added to the open mix, or kept until the user opens or creates one.
  import { onDestroy } from "svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { mixes } from "$lib/state/mixes.svelte";
  import { addTracks } from "./actions";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import MixEditor from "./MixEditor.svelte";
  import MixList from "./MixList.svelte";
  import NameDialog from "./NameDialog.svelte";
  import UnsavedDialog from "./UnsavedDialog.svelte";
  import { preview } from "./preview.svelte";
  import { mv } from "./view.svelte";

  // leaving the view silences the preview; the open mix stays open
  onDestroy(() => preview.pause());

  // accept the key with or without the area prefix
  const KEYS = ["addTracks", "mixes.addTracks"];

  $effect(() => {
    for (const key of KEYS) {
      const ids = nav.params[key];
      if (Array.isArray(ids)) {
        nav.take(key);
        const clean = ids.filter((x): x is number => typeof x === "number");
        if (clean.length > 0) mv.pending = [...mv.pending, ...clean];
      }
    }
  });

  // once a mix is open, the waiting tracks go in at its end
  $effect(() => {
    if (mixes.doc && mv.pending.length > 0 && !mixes.busy) {
      const ids = mv.pending;
      mv.pending = [];
      void addTracks(ids);
    }
  });
</script>

<div class="mixes">
  {#if mixes.doc}
    <MixEditor />
  {:else}
    <MixList />
  {/if}
</div>

<ConfirmDialog />
<NameDialog />
<UnsavedDialog />

<style>
  .mixes {
    /* the mockup's magenta ramp for tool blocks, and its warning yellow */
    --l2a: #e070ff;
    --l2b: #ff50ff;
    --l2c: #ff50d0;
    --l2d: #ff48a8;
    --l2e: #f060ff;
    --l2f: #ff50e8;
    --warn: #ffd23b;
    height: 100%;
    min-height: 0;
    min-width: 0;
  }
</style>
