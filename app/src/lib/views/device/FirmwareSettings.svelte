<script lang="ts">
  // Settings > Device > Included firmware: whether the Firmware card offers the firmware that ships with the app.
  import HelpLink from "../help/HelpLink.svelte";
  import { firmwareOffer } from "./firmware.svelte";
</script>

<label class="check">
  <input type="checkbox" checked={firmwareOffer.offer} onchange={(e) => firmwareOffer.setOffer(e.currentTarget.checked)} />
  Offer the included firmware <HelpLink id="install-firmware" />
</label>
<p class="muted note">Shows an update on the Firmware card when the app's firmware number is higher than the Pacemaker's.</p>

<style>
  .check {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .note {
    margin: 6px 0 0;
  }
</style>
