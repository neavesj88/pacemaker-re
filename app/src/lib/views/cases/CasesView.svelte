<script lang="ts">
  // Deep link: the History view opens a played case with `nav.go("cases", { "cases.caseId": id })`.
  import { cases } from "$lib/state/cases.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import CaseDetail from "./CaseDetail.svelte";
  import CaseList from "./CaseList.svelte";
  import SmartCaseEditor from "./SmartCaseEditor.svelte";

  $effect(() => {
    const want = nav.params["cases.caseId"];
    if (typeof want !== "number" || !session.library) return;
    const c = session.library.cases.find((x) => x.id === want);
    // taken once; a case that is no longer in the library just shows the list
    nav.take("cases.caseId");
    if (c) void cases.openCase(c);
  });
</script>

{#if cases.smartOpen !== null}
  {#key cases.smartOpen}<SmartCaseEditor />{/key}
{:else if cases.open}
  <CaseDetail />
{:else}
  <CaseList />
{/if}
