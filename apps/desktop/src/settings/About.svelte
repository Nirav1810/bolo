<script lang="ts">
  import { api, megabytes, type SettingsView, type UpdateView } from "../lib/api";
  import { onAppEvent } from "../lib/events";
  // The apps' own icons (src-tauri/icons, icons-nightly), so About looks like the taskbar.
  import releaseIcon from "../../src-tauri/icons/128x128.png?no-inline";
  import nightlyIcon from "../../src-tauri/icons-nightly/128x128.png?no-inline";

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

  const status = $derived(update?.status ?? { state: "idle" as const });
  const busy = $derived(["checking", "downloading", "installing"].includes(status.state));
  const percent = $derived(
    status.state === "downloading" && status.total ? Math.round((status.downloaded / status.total) * 100) : null,
  );
</script>

<div class="card min-h-22">
  <img src={update?.variant === "nightly" ? nightlyIcon : releaseIcon} width="40" height="40" alt="" />
  <div class="card-text">
    <span class="font-semibold">
      {update?.product ?? "Bolo"} {update?.version ?? view.version}
      {#if update?.badge}<span class="tag" class:nightly={update.badge === "Nightly"}>{update.badge}</span>{/if}
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
        <span>{update?.product ?? "Bolo"} is up to date</span>
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
  {#if update}
    <div class="card">
      <div class="card-text">
        {#if update.variant === "nightly"}
          <span>Bolo Nightly gets a new build with every change</span>
          <span class="caption">
            Newest features, less tested. For the release, install Bolo next to it: they share your settings,
            models and history, and only the one you started last listens for the key.
          </span>
        {:else}
          <span>Try what’s next with Bolo Nightly</span>
          <span class="caption">
            A separate app with a violet icon, built from every change. It installs next to Bolo and shares your
            settings, models and history. Only the one you started last listens for the key.
          </span>
        {/if}
      </div>
      <button
        type="button"
        class="link whitespace-nowrap"
        onclick={() => update && api.openLink(update.other.url)}
        >Get {update.other.product}</button
      >
    </div>
  {/if}
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
