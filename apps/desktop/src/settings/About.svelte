<script lang="ts">
  import { ExternalLink, FolderOpen, RefreshCw, LoaderCircle, CircleCheck, ArrowUpCircle, CircleAlert } from "@lucide/svelte";
  import { api, megabytes, type SettingsView, type UpdateView } from "../lib/api";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  // The apps' own icons (src-tauri/icons, icons-nightly), so About looks like the taskbar.
  import releaseIcon from "../../src-tauri/icons/128x128.png?no-inline";
  import nightlyIcon from "../../src-tauri/icons-nightly/128x128.png?no-inline";

  // The self-updater (ADR-0019): the state lives in Rust and arrives as `update` events,
  // which the settings shell keeps in `updates`.
  let { view, updates = $bindable() }: { view: SettingsView; updates: UpdateView | null } = $props();

  const status = $derived(updates?.status ?? { state: "idle" as const });
  const busy = $derived(["checking", "downloading", "installing"].includes(status.state));
  const percent = $derived(
    status.state === "downloading" && status.total ? Math.round((status.downloaded / status.total) * 100) : null,
  );
  const product = $derived(updates?.product ?? "Bolo");
</script>

<div class="flex items-center gap-4 rounded-xl border border-border/80 bg-card px-5 py-5 shadow-xs/5 dark:border-border">
  <img src={updates?.variant === "nightly" ? nightlyIcon : releaseIcon} width="48" height="48" alt="" class="rounded-xl" />
  <div class="flex min-w-0 flex-1 flex-col gap-1">
    <div class="flex items-center gap-2">
      <span class="text-base font-semibold tracking-[-0.01em]">{product}</span>
      {#if updates?.badge}<Badge variant={updates.badge === "Nightly" ? "nightly" : "secondary"}>{updates.badge}</Badge>{/if}
    </div>
    <span class="text-xs text-muted-foreground">
      Version <span class="mono text-foreground">{updates?.version ?? view.version}</span> · Open-source dictation for English
      and Hinglish · MIT or Apache-2.0
    </span>
  </div>
  <Button variant="outline" size="sm" onclick={() => api.openLink("https://github.com/Nirav1810/bolo")}>
    Source <ExternalLink aria-hidden="true" />
  </Button>
</div>

<SettingsSection title="Updates">
  <SettingsRow
    title={status.state === "available"
      ? `Version ${status.version} is available`
      : status.state === "downloading"
        ? "Downloading the update…"
        : status.state === "installing"
          ? "Installing…"
          : status.state === "checking"
            ? "Checking for updates…"
            : status.state === "up-to-date"
              ? `${product} is up to date`
              : status.state === "error" && status.manual
                ? "Couldn’t check for updates"
                : "Automatic updates"}
  >
    {#snippet leading()}
      <div class="grid size-8 place-items-center rounded-lg bg-accent">
        {#if status.state === "available"}
          <ArrowUpCircle class="size-4 text-primary-text" aria-hidden="true" />
        {:else if busy}
          <LoaderCircle class="size-4 animate-spin text-muted-foreground" aria-hidden="true" />
        {:else if status.state === "error" && status.manual}
          <CircleAlert class="size-4 text-destructive-foreground" aria-hidden="true" />
        {:else}
          <CircleCheck class="size-4 text-success-foreground" aria-hidden="true" />
        {/if}
      </div>
    {/snippet}
    {#snippet description()}
      {#if status.state === "available"}
        Bolo restarts to finish. Your settings and history stay.
      {:else if status.state === "downloading"}
        {megabytes(status.downloaded)}{status.total ? ` of ${megabytes(status.total)}` : ""}
      {:else if status.state === "installing"}
        Bolo will close and open again in a few seconds.
      {:else if status.state === "error" && status.manual}
        {status.message}
      {:else}
        Bolo checks when it starts and every 6 hours.
      {/if}
    {/snippet}
    {#snippet control()}
      {#if status.state === "available"}
        <Button variant="primary" onclick={() => api.installUpdate()}>Update now</Button>
      {:else}
        <Button variant="outline" disabled={busy} onclick={() => api.checkUpdate()}>
          <RefreshCw class={status.state === "checking" ? "animate-spin" : ""} aria-hidden="true" />
          Check now
        </Button>
      {/if}
    {/snippet}
    {#if status.state === "downloading"}
      <div
        class="progress ml-11"
        role="progressbar"
        aria-label="Update download"
        aria-valuenow={percent ?? undefined}
        aria-valuemin="0"
        aria-valuemax="100"
      >
        <div style:width="{percent ?? 100}%"></div>
      </div>
    {/if}
  </SettingsRow>
  {#if status.state === "available" && status.notes}
    <div class="px-4 py-3">
      <p class="mb-1 text-xs font-medium text-muted-foreground">What’s new</p>
      <p class="max-h-40 overflow-y-auto text-[13px] whitespace-pre-wrap">{status.notes}</p>
    </div>
  {/if}
  {#if updates}
    <SettingsRow
      title={updates.variant === "nightly" ? "Bolo Nightly gets a new build with every change" : "Try what’s next with Bolo Nightly"}
      description={updates.variant === "nightly"
        ? "Newest features, less tested. Install Bolo next to it for the release: they share your settings, models and history, and only the one you started last listens for the key."
        : "A separate app with a violet icon, built from every change. It installs next to Bolo and shares your settings, models and history. Only the one you started last listens for the key."}
    >
      {#snippet control()}
        <Button variant="ghost" size="sm" onclick={() => updates && api.openLink(updates.other.url)}>
          Get {updates.other.product}
          <ExternalLink aria-hidden="true" />
        </Button>
      {/snippet}
    </SettingsRow>
  {/if}
</SettingsSection>

<SettingsSection title="Files">
  {#each [["settings", "Settings file", view.paths.settings], ["models", "Models", view.paths.models], ["logs", "Logs", "For a bug report."]] as const as [which, title, detail] (which)}
    <SettingsRow {title}>
      {#snippet description()}
        <span class={which === "logs" ? "" : "mono text-2xs break-all"}>{detail}</span>
      {/snippet}
      {#snippet control()}
        <Button variant="ghost" size="sm" onclick={() => api.openFolder(which)}>
          <FolderOpen aria-hidden="true" />
          Open
        </Button>
      {/snippet}
    </SettingsRow>
  {/each}
</SettingsSection>
