<script lang="ts">
  import { cases } from "$lib/state/cases.svelte";
  import CaseDetail from "./CaseDetail.svelte";
  import CaseList from "./CaseList.svelte";
  import SmartCaseEditor from "./SmartCaseEditor.svelte";
</script>

{#if cases.smartOpen !== null}
  {#key cases.smartOpen}<SmartCaseEditor />{/key}
{:else if cases.open}
  <CaseDetail />
{:else}
  <CaseList />
{/if}
