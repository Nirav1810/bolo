<script lang="ts">
  import { api, type SettingsView } from "../lib/api";

  let { view }: { view: SettingsView } = $props();
</script>

<div class="card min-h-22">
  <svg width="40" height="40" viewBox="0 0 40 40" fill="none" aria-hidden="true">
    <rect width="40" height="40" rx="10" fill="var(--bolo-accent)" />
    <rect x="16" y="9" width="8" height="14" rx="4" fill="var(--bolo-on-accent)" />
    <path d="M12 20a8 8 0 0016 0M20 28v4" stroke="var(--bolo-on-accent)" stroke-width="2" stroke-linecap="round" />
  </svg>
  <div class="card-text">
    <span class="font-semibold">Bolo {view.version}</span>
    <span class="caption">Open-source dictation for English and Hinglish. MIT or Apache-2.0.</span>
  </div>
  <button type="button" class="link" onclick={() => api.openLink("https://github.com/Nirav1810/bolo")}>Source</button>
</div>
<div class="group mt-[3px]">
  <div class="card compact">
    <div class="card-text"><span>Settings file</span><span class="caption mono">{view.paths.settings}</span></div>
    <button type="button" class="btn" onclick={() => api.openFolder("settings")}>Open folder</button>
  </div>
  <div class="card compact">
    <div class="card-text"><span>Models</span><span class="caption mono">{view.paths.models}</span></div>
    <button type="button" class="btn" onclick={() => api.openFolder("models")}>Open folder</button>
  </div>
  <div class="card compact">
    <div class="card-text"><span>Logs</span><span class="caption">For a bug report.</span></div>
    <button type="button" class="btn" onclick={() => api.openFolder("logs")}>Open folder</button>
  </div>
  <div class="card compact">
    <div class="card-text"><span>Updates</span><span class="caption">Check the releases page. Bolo doesn’t update itself yet.</span></div>
    <button type="button" class="link" onclick={() => api.openLink("https://github.com/Nirav1810/bolo/releases")}>Releases</button>
  </div>
</div>
