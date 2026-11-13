<script lang="ts">
  import {
    CalendarClock,
    CircleAlert,
    CircleArrowUp,
    CircleCheck,
    ExternalLink,
    FolderClock,
    FolderCog,
    FolderDown,
    LoaderCircle,
    RefreshCw,
    ScrollText,
  } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { api, keyLabel, megabytes, sentence, variant, type Form, type SettingsView, type UpdateView } from "../lib/api";
  import AppMark from "../lib/ui/AppMark.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Card from "../lib/ui/Card.svelte";
  import IconTile from "../lib/ui/IconTile.svelte";
  import Progress from "../lib/ui/Progress.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";

  // The self-updater (ADR-0019): the state lives in Rust and arrives as `update` events,
  // which the settings shell keeps in `updates`.
  let {
    view,
    update,
    updates = $bindable(),
  }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void>; updates: UpdateView | null } = $props();

  const status = $derived(updates?.status ?? { state: "idle" as const });
  const busy = $derived(["checking", "downloading", "installing"].includes(status.state));
  const percent = $derived(
    status.state === "downloading" && status.total ? Math.round((status.downloaded / status.total) * 100) : null,
  );
  const version = $derived(updates?.version ?? view.version);
  /** This installed app's name, known from its version before the updater has answered. */
  const product = $derived(updates?.product ?? variant(view.version).product);
  const failed = $derived(status.state === "error" && status.manual);
  // One app, two channels (ADR-0019). Picking the other channel reinstalls Bolo from its
  // feed, so the name, icon and features change together once it restarts.
  const nightlyBuild = $derived(updates ? updates.variant === "nightly" : variant(view.version).nightly);
  const switching = $derived((view.form.channel === "nightly") !== nightlyBuild);
  const target = $derived(view.form.channel === "nightly" ? "Bolo Nightly" : "Bolo Beta");
  const switched = $derived(status.state === "updated" && variant(status.from).nightly !== nightlyBuild);

  type Tone = "neutral" | "primary" | "success" | "warning" | "error";
  /** The status row's tile: its icon and tone follow the updater's state. */
  const tile = $derived.by((): { icon: Component<{ class?: string }>; tone: Tone; spin: boolean } => {
    if (status.state === "available") return { icon: CircleArrowUp, tone: "primary", spin: false };
    if (status.state === "downloading") return { icon: LoaderCircle, tone: "primary", spin: true };
    if (busy) return { icon: LoaderCircle, tone: "neutral", spin: true };
    if (failed) return { icon: CircleAlert, tone: "error", spin: false };
    if (status.state === "up-to-date" || status.state === "updated") return { icon: CircleCheck, tone: "success", spin: false };
    return { icon: RefreshCw, tone: "neutral", spin: false };
  });

  const title = $derived(
    status.state === "available"
      ? status.switch
        ? `${target} ${status.version} is ready`
        : `Version ${status.version} is available`
      : status.state === "downloading"
        ? switching
          ? `Switching to ${target}…`
          : "Downloading the update…"
        : status.state === "installing"
          ? "Installing…"
          : status.state === "checking"
            ? "Checking for updates…"
            : status.state === "up-to-date"
              ? `${product} is up to date`
              : status.state === "updated"
                ? switched
                  ? `Switched to ${product}`
                  : `Updated to version ${version}`
                : failed
                  ? "Couldn’t check for updates"
                  : "No update check yet",
  );

  const description = $derived(
    (status.state === "available" && status.switch) || (status.state === "downloading" && switching)
      ? "Bolo reinstalls from that channel and restarts. Your settings, models and history stay."
      : status.state === "available" || status.state === "downloading"
        ? `${product} restarts to finish. Your settings and history stay.`
        : status.state === "installing"
          ? `${product} restarts in a few seconds.`
          : status.state === "updated"
            ? `${product} restarted after ${switched ? "switching" : "updating"} from ${status.from}, and is ready: hold ${keyLabel(view.form.push_to_talk)} to dictate.`
            : failed
              ? undefined
              : view.form.auto_update_check
                ? `${product} checks for updates when it starts and once a day.`
                : "Automatic update checks are off. Check now to look for an update.",
  );

  const channelNote = $derived(
    (switching
      ? `Switching to ${target}: Bolo reinstalls from that channel and restarts in a few seconds.`
      : nightlyBuild
        ? "Nightly gets a build of every change, with a violet icon: newest features, less tested. Beta gets the releases."
        : "Beta gets the releases. Nightly gets a build of every change, with a violet icon: newest features, less tested.") +
      " Switching keeps your settings, models and history.",
  );

  const files = $derived([
    { which: "settings", title: "Settings file", detail: view.paths.settings, icon: FolderCog },
    { which: "models", title: "Models", detail: view.paths.models, icon: FolderDown },
    { which: "logs", title: "Logs", detail: "For a bug report.", icon: FolderClock },
  ] as const);
</script>

<Card>
  <div class="flex flex-col gap-2 px-4 py-3.5">
    <div class="flex items-center justify-between gap-3">
      <AppMark {version} size="lg" />
      <div class="flex shrink-0 items-center gap-1.5">
        <Button variant="ghost" size="sm" onclick={() => api.openLink("https://github.com/Nirav1810/bolo/releases")}>
          Release notes<ExternalLink aria-hidden="true" />
        </Button>
        <Button variant="ghost" size="sm" onclick={() => api.openLink("https://github.com/Nirav1810/bolo")}>
          Source<ExternalLink aria-hidden="true" />
        </Button>
      </div>
    </div>
    <p class="text-xs text-muted-foreground tabular-nums">
      Version {version} · Open-source dictation for English and Hinglish · MIT or Apache-2.0
    </p>
  </div>
</Card>

<SettingsSection title="Updates">
  <SettingsRow {title} {description} status={failed && status.state === "error" ? { tone: "error", text: sentence(status.message) } : null}>
    {#snippet leading()}<IconTile icon={tile.icon} tone={tile.tone} spin={tile.spin} />{/snippet}
    {#snippet control()}
      {#if status.state === "available"}
        <Button variant="primary" size="sm" onclick={() => api.installUpdate()}>{status.switch ? "Switch now" : "Update now"}</Button>
      {:else}
        <Button variant="outline" size="sm" loading={status.state === "checking"} disabled={busy} onclick={() => api.checkUpdate()}>
          <RefreshCw aria-hidden="true" />Check now
        </Button>
      {/if}
    {/snippet}
    {#if status.state === "downloading"}
      <Progress
        label="Downloading the update"
        value={percent}
        caption={status.total ? `${megabytes(status.downloaded)} of ${megabytes(status.total)}` : megabytes(status.downloaded)}
      />
    {/if}
  </SettingsRow>
  {#if status.state === "available" && status.notes}
    <SettingsRow title="What’s new" align="start">
      {#snippet leading()}<IconTile icon={ScrollText} />{/snippet}
      {#snippet description()}<span class="block max-h-40 overflow-y-auto whitespace-pre-wrap">{status.notes}</span>{/snippet}
    </SettingsRow>
  {/if}
  <SettingsRow
    title="Check for updates automatically"
    labelFor="auto-update-check"
    description="It never installs without your click."
  >
    {#snippet leading()}<IconTile icon={CalendarClock} />{/snippet}
    {#snippet control()}
      <Switch id="auto-update-check" checked={view.form.auto_update_check} onchange={(on) => update((f) => (f.auto_update_check = on))} />
    {/snippet}
  </SettingsRow>
</SettingsSection>

<SettingsSection title="Channel">
  <SettingsRow title="Update channel" description={channelNote}>
    {#snippet control()}
      <Segmented
        label="Update channel"
        value={view.form.channel}
        options={[
          ["beta", "Beta"],
          ["nightly", "Nightly"],
        ]}
        onchange={(v) => update((f) => (f.channel = v as Form["channel"]))}
      />
    {/snippet}
  </SettingsRow>
</SettingsSection>

<SettingsSection title="Files">
  {#each files as f (f.which)}
    <SettingsRow title={f.title}>
      {#snippet leading()}<IconTile icon={f.icon} />{/snippet}
      {#snippet description()}
        <span class={f.which === "logs" ? "" : "font-mono text-xs break-all"}>{f.detail}</span>
      {/snippet}
      {#snippet control()}
        <Button variant="outline" size="sm" aria-label="Open the {f.which} folder" onclick={() => api.openFolder(f.which)}>Open</Button>
      {/snippet}
    </SettingsRow>
  {/each}
</SettingsSection>
