<script lang="ts">
  import { api, megabytes, type SettingsView, type UpdateChannel, type UpdateView } from "../lib/api";
  import { onAppEvent } from "../lib/events";
  import Segmented from "../lib/Segmented.svelte";

  let { view }: { view: SettingsView } = $props();

  // The self-updater (ADR-0019): the state lives in Rust and arrives as `update` events.
  let update = $state<UpdateView | null>(null);

  $effect(() => {
    api.updateView().then((v) => (update = v));
    const off = onAppEvent((e) => {
      if (e.kind === "update" && update) update = { ...update, status: e.status };
    });
    return () => {
      off.then((f) => f());
    };
  });

  async function setChannel(channel: string) {
    update = await api.setChannel(channel as UpdateChannel);
  }

  const status = $derived(update?.status ?? { state: "idle" as const });
  const busy = $derived(["checking", "downloading", "installing"].includes(status.state));
  const percent = $derived(
    status.state === "downloading" && status.total ? Math.round((status.downloaded / status.total) * 100) : null,
  );
</script>

<div class="card min-h-22">
  <svg width="40" height="40" viewBox="0 0 40 40" fill="none" aria-hidden="true">
    <rect width="40" height="40" rx="10" fill="var(--bolo-accent)" />
    <rect x="16" y="9" width="8" height="14" rx="4" fill="var(--bolo-on-accent)" />
    <path d="M12 20a8 8 0 0016 0M20 28v4" stroke="var(--bolo-on-accent)" stroke-width="2" stroke-linecap="round" />
  </svg>
  <div class="card-text">
    <span class="font-semibold">
      Bolo {update?.version ?? view.version}
      {#if update?.badge}<span class="tag" class:accent={update.badge === "Nightly"}>{update.badge}</span>{/if}
    </span>
    <span class="caption">Open-source dictation for English and Hinglish. MIT or Apache-2.0.</span>
  </div>
  <button type="button" class="link" onclick={() => api.openLink("https://github.com/Nirav1810/bolo")}>Source</button>
</div>

<h2 class="section">Updates</h2>
<div class="group">
  <div class="card">
    <div class="card-text">
      {#if status.state === "available"}
        <span>Version {status.version} is available</span>
        <span class="caption">Bolo restarts to finish. Your settings and history stay.</span>
      {:else if status.state === "downloading"}
        <span>Downloading the update…</span>
        <div
          class="progress mt-1.5"
          role="progressbar"
          aria-label="Update download"
          aria-valuenow={percent ?? undefined}
          aria-valuemin="0"
          aria-valuemax="100"
        >
          <div style:width="{percent ?? 100}%"></div>
        </div>
        <span class="caption">
          {megabytes(status.downloaded)}{status.total ? ` of ${megabytes(status.total)}` : ""}
        </span>
      {:else if status.state === "installing"}
        <span>Installing…</span>
        <span class="caption">Bolo will close and open again in a few seconds.</span>
      {:else if status.state === "checking"}
        <span>Checking for updates…</span>
        <span class="caption">&nbsp;</span>
      {:else if status.state === "up-to-date"}
        <span>Bolo is up to date</span>
        <span class="caption">Checks again every 6 hours.</span>
      {:else if status.state === "error" && status.manual}
        <span>Couldn’t check for updates</span>
        <span class="caption">{status.message}</span>
      {:else}
        <span>Updates</span>
        <span class="caption">Bolo checks when it starts and every 6 hours.</span>
      {/if}
    </div>
    {#if status.state === "available"}
      <button type="button" class="btn primary" onclick={() => api.installUpdate()}>Update now</button>
    {:else}
      <button type="button" class="btn" disabled={busy} onclick={() => api.checkUpdate()}>Check for updates</button>
    {/if}
  </div>
  {#if status.state === "available" && status.notes}
    <div class="card compact">
      <div class="card-text">
        <span class="caption">What’s new</span>
        <span class="notes">{status.notes}</span>
      </div>
    </div>
  {/if}
  <div class="card">
    <div class="card-text">
      <span>Channel</span>
      <span class="caption">
        {#if update?.channel === "nightly"}
          A new build from every change. Newest features, less tested.
        {:else}
          Releases. Bolo is in beta until 1.0.
        {/if}
      </span>
    </div>
    {#if update}
      <Segmented
        label="Update channel"
        value={update.channel}
        options={[
          ["beta", "Beta"],
          ["nightly", "Nightly"],
        ]}
        onchange={setChannel}
      />
    {/if}
  </div>
</div>

<h2 class="section">Files</h2>
<div class="group">
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
</div>

<style>
  .notes {
    white-space: pre-wrap;
    max-height: 10rem;
    overflow-y: auto;
  }
</style>
