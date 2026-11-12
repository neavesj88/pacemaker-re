<script lang="ts">
  // Always mounted while the app runs: watches for the moments a smart case's Pacemaker copy can
  // have fallen behind its rules (a library is opened, a track or case is edited, a smart case is
  // saved) and asks the refresh store to do what the setting says. Also hosts its dialog.
  import { untrack } from "svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import { smartRefresh } from "$lib/state/smartRefresh.svelte";
  import { caseExport } from "$lib/state/caseExport.svelte";
  import ExportDialog from "./ExportDialog.svelte";
  import HoldersDialog from "./HoldersDialog.svelte";
  import SmartRefreshDialog from "./SmartRefreshDialog.svelte";

  // the export dialog and the case menus read the remembered export folder and format
  $effect(() => {
    void caseExport.ensure();
  });

  // Edits arrive in bursts (a reload, then the case list, then the smart list): one check after they settle.
  const SETTLE_MS = 700;

  $effect(() => {
    const lib = session.library;
    if (!lib) return;
    // reading these is what re-runs the effect: tracks (ratings, tags, plays, added files), the
    // case list (membership rules), the smart cases themselves
    void lib.tracks;
    void cases.infos;
    void cases.smart;
    const timer = setTimeout(() => untrack(() => smartRefresh.check()), SETTLE_MS);
    return () => clearTimeout(timer);
  });
</script>

<SmartRefreshDialog />
<ExportDialog />
<HoldersDialog />
