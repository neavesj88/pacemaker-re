<script lang="ts">
  // Settings > Library > Pacemaker connected: whether the popup is shown, and how often the app looks
  // for a connected drive. Looking only lists the drives; it never opens one.
  import { connect } from "$lib/state/connect.svelte";
  import { CONNECT_SECS_MAX } from "$lib/state/connectLogic";
  import HelpLink from "../help/HelpLink.svelte";

  function saveSecs(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    input.value = String(connect.setSecs(input.valueAsNumber)); // show what was kept
  }
</script>

<div class="box">
  <label class="check">
    <input type="checkbox" checked={connect.ask} onchange={(e) => connect.setAsk(e.currentTarget.checked)} />
    Ask to open a Pacemaker when one is connected <HelpLink id="libraries" anchor="pacemaker-connected" />
  </label>
  <div class="row">
    <label for="connect-secs">Check for a connected Pacemaker every (s)</label>
    <input id="connect-secs" type="number" min="0" max={CONNECT_SECS_MAX} step="1" value={connect.secs} onchange={saveSecs} />
  </div>
  <p class="muted note">0 = off. It only looks for the drive.</p>
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row input {
    width: 70px;
  }
  .check {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .note {
    margin: 0;
  }
</style>
