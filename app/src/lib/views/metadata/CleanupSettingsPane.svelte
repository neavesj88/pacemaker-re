<script lang="ts">
  // The clean-up recipes for the Settings view. The Tidy page saves them as they change; this
  // page has no Tidy view behind it, so it does the saving itself (without the preview).
  import { untrack } from "svelte";
  import CleanupRecipes from "./CleanupRecipes.svelte";
  import { cleanup } from "./cleanupState.svelte";

  $effect(() => {
    void cleanup.signature;
    untrack(() => cleanup.scheduleSave());
  });
</script>

<CleanupRecipes />
