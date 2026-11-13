// The settings window's calls into Rust (src-tauri/src/commands.rs) and their types.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type PushToTalkKey =
  | "caps-lock"
  | "right-ctrl"
  | "right-alt"
  | "f13"
  | "f14"
  | "f15"
  | "f16"
  | "f17"
  | "f18"
  | "f19"
  | "f20"
  | "f21"
  | "f22"
  | "f23"
  | "f24";
export type LanguageMode = "auto" | "english" | "hinglish";
export type Script = "roman" | "devanagari";
export type Format = "plain" | "markdown" | "html";

/** A spoken trigger and the text it expands to (bolo_core::Snippet). */
export interface TextSnippet {
  trigger: string;
  text: string;
}

/** What the settings window edits (src-tauri/src/form.rs, `Form`). */
export interface Form {
  push_to_talk: PushToTalkKey;
  autostart: boolean;
  language: LanguageMode;
  script: Script;
  engines: string[];
  dictionary: string[];
  /** `[[snippets]]`: say `trigger`, get `text` (verbatim). */
  snippets: TextSnippet[];
  polish: boolean;
  tone: string;
  history: boolean;
  retention_days: number;
  /** `[audio] input_device`: the microphone by name; `null` for the Windows default. */
  input_device: string | null;
  /** Check for updates at launch and once a day (`[updates] auto_check`). */
  auto_update_check: boolean;
  /** The update channel Bolo follows (`[updates] channel`; unpicked, the build's own).
   *  Picking the other one switches Bolo to that channel's build (ADR-0019). */
  channel: "beta" | "nightly";
}

export interface ModelView {
  id: string;
  name: string;
  spec: string;
  languages: string[];
  download_bytes: number;
  disk_bytes: number | null;
  installed: boolean;
  recommended: boolean;
  hinglish: boolean;
}

/** Where an app's field comes from: the user's row, Bolo's app rules, or the default. */
export type Source = "user" | "bundled" | "default";
export interface Field<T> {
  value: T;
  source: Source;
}
export interface AppFields {
  name: Field<string>;
  polish: Field<boolean>;
  tone: Field<string>;
  format: Field<Format>;
}

/** One row of the Apps tab (src-tauri/src/rules.rs, `AppView`; ADR-0020). */
export interface AppView extends AppFields {
  /** The exe the user's row is stored under. */
  exe: string;
  /** Every exe the row covers. */
  exes: string[];
  /** The fields without the user's row: what clearing one goes back to. */
  inherited: AppFields;
  code_mode: boolean;
  /** Bolo's app rules know it; resetting keeps it listed. */
  bundled: boolean;
  /** The user set at least one field. */
  custom: boolean;
}

/** One field of an app's row; `null` clears it (src-tauri/src/form.rs, `AppChange`). */
export type AppChange =
  | { field: "name"; value: string | null }
  | { field: "polish"; value: boolean | null }
  | { field: "tone"; value: string | null }
  | { field: "format"; value: Format | null };

export interface CloudEngine {
  spec: string;
  name: string;
  provider: string;
}

export interface SettingsView {
  form: Form;
  keys: Record<string, string | null>;
  keystore: boolean;
  key_name: string;
  models: ModelView[];
  downloading: string[];
  cloud: CloudEngine[];
  apps: AppView[];
  /** The last app in front that wasn't Bolo, when it has no row yet. */
  recent_app: string | null;
  version: string;
  paths: { settings: string; models: string; history: string };
  ram_mb: number;
  onboarding_step: number | null;
}

/** Onboarding's mic check (src-tauri/src/mic.rs, `MicCheck`). */
export interface MicCheck {
  device: string;
  /** The chosen microphone isn't plugged in, so `device` is the Windows default. */
  fell_back: boolean;
  leading_ms: number | null;
  peak_rms: number;
  heard: boolean;
  late: boolean;
}

/** The microphones plugged in now (src-tauri/src/commands.rs, `Microphones`). */
export interface Microphones {
  devices: string[];
  /** The Windows default's name. */
  default: string | null;
}

export type Tested =
  | { result: "ok" }
  | { result: "refused" }
  | { result: "untestable" }
  | { result: "failed"; message: string };

export interface HistoryItem {
  id: number;
  at_ms: number;
  app: string;
  language: string;
  engine: string;
  final_text: string;
  polished_text: string | null;
  swap: string | null;
}

/** One provider's totals (bolo-store `ProviderUsage`). Costs are estimates at list price. */
export interface ProviderUsage {
  provider: string;
  requests: number;
  audio_secs: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  cost_usd: number;
}

/** The providers' own usage reports (src-tauri/src/billing.rs). Days are UTC days. */
export type Report<T> =
  | { state: "no-key" }
  | { state: "ok"; report: T }
  | { state: "refused" | "failed"; message: string };
export interface Periods<T> {
  today: T;
  last_30_days: T;
  all_time: T;
}
export interface DeepgramReport {
  project: string;
  periods: Periods<{ hours: number; requests: number; usd: number | null }>;
  balance: number | null;
  balance_units: string | null;
}
export interface AnthropicReport {
  key_name: string | null;
  periods: Periods<{ input_tokens: number; cache_read_tokens: number; cache_write_tokens: number; output_tokens: number }>;
  account_usd: Periods<number>;
}
/** Which key a report read with: the Engines and keys one, or a separate admin key. */
export type KeySource = "none" | "engine" | "usage";

export interface ProviderReports {
  deepgram: Report<DeepgramReport>;
  anthropic: Report<AnthropicReport>;
  deepgram_key: KeySource;
  anthropic_key: KeySource;
  fetched_ms: number;
}

/** The usage meter (src-tauri/src/commands.rs, `UsageView`). */
export interface UsageView {
  today: ProviderUsage[];
  last_30_days: ProviderUsage[];
  all_time: ProviderUsage[];
  since_ms: number | null;
}

export interface DownloadEvent {
  id: string;
  phase:
    "downloading" | "verifying" | "unpacking" | "done" | "failed" | "cancelled";
  downloaded: number;
  total: number;
  error: string | null;
}

/** The self-updater's state (src-tauri/src/updates.rs, `Status`). */
export type UpdateStatus =
  | { state: "idle" | "checking" | "up-to-date" | "installing" }
  | { state: "updated"; from: string }
  | { state: "available"; version: string; notes: string | null; switch: boolean }
  | { state: "downloading"; downloaded: number; total: number | null }
  | { state: "error"; message: string; manual: boolean };

/** About's update section (src-tauri/src/updates.rs, `View`). One app with two channels
 *  (ADR-0019): the build that runs names it, and the followed channel says where it
 *  updates from. */
export interface UpdateView {
  version: string;
  product: "Bolo" | "Bolo Nightly";
  /** The running build's channel. */
  variant: "release" | "nightly";
  badge: "Beta" | "Nightly" | null;
  /** The channel Bolo follows. */
  channel: "release" | "nightly";
  status: UpdateStatus;
}

/** Which build this is (ADR-0019): a Nightly from `main`, or a release (Beta until 1.0). */
export function variant(version: string): {
  name: "Nightly" | "Beta" | "Release";
  nightly: boolean;
  /** The installed app's own name. Feature prose says plain "Bolo". */
  product: "Bolo" | "Bolo Nightly";
} {
  if (version.includes("-nightly.")) return { name: "Nightly", nightly: true, product: "Bolo Nightly" };
  return {
    name: version.startsWith("0.") ? "Beta" : "Release",
    nightly: false,
    product: "Bolo",
  };
}

export const api = {
  settings: () => invoke<SettingsView>("settings_get"),
  save: (form: Form) => invoke<SettingsView>("settings_save", { form }),
  appSet: (exe: string, change: AppChange) =>
    invoke<SettingsView>("app_set", { exe, change }),
  appReset: (exe: string) => invoke<SettingsView>("app_reset", { exe }),
  appAdd: (exe: string) => invoke<SettingsView>("app_add", { exe }),
  saveKey: (provider: string, key: string) =>
    invoke<string | null>("key_save", { provider, key }),
  testKey: (provider: string, key?: string) =>
    invoke<Tested>("key_test", { provider, key: key ?? null }),
  capture: (capturing: boolean) => invoke<void>("ptt_capture", { capturing }),
  history: (query: string) =>
    invoke<HistoryItem[]>("history_search", { query }),
  deleteRow: (id: number) => invoke<void>("history_delete", { id }),
  clearHistory: () => invoke<void>("history_clear"),
  usage: (todayStartMs: number) =>
    invoke<UsageView>("usage_summary", { todayStartMs }),
  resetUsage: () => invoke<void>("usage_reset"),
  usageReports: (refresh: boolean) =>
    invoke<ProviderReports>("usage_reports", { refresh }),
  copy: (text: string) => invoke<void>("copy_text", { text }),
  download: (id: string) => invoke<void>("model_download", { id }),
  cancelDownload: (id: string) => invoke<void>("model_cancel", { id }),
  removeModel: (id: string) => invoke<void>("model_remove", { id }),
  openFolder: (which: "settings" | "models" | "logs") =>
    invoke<void>("open_folder", { which }),
  openLink: (url: string) => invoke<void>("open_link", { url }),
  micCheck: () => invoke<MicCheck>("mic_check"),
  microphones: () => invoke<Microphones>("microphones"),
  onboardingSave: (step: number | null) =>
    invoke<void>("onboarding_save", { step }),
  updateView: () => invoke<UpdateView>("update_view"),
  checkUpdate: () => invoke<void>("update_check"),
  installUpdate: () => invoke<void>("update_install"),
};

/** Mic levels (RMS) while the mic check listens. */
export function onMicLevel(
  handler: (rms: number) => void,
): Promise<UnlistenFn> {
  return listen<number>("bolo://mic", (e) => handler(e.payload));
}

export function onDownload(
  handler: (e: DownloadEvent) => void,
): Promise<UnlistenFn> {
  return listen<DownloadEvent>("bolo://download", (e) => handler(e.payload));
}

/** The push-to-talk key a key press names, from `KeyboardEvent.code`. */
export function keyFromCode(code: string): PushToTalkKey | null {
  if (code === "CapsLock") return "caps-lock";
  if (code === "ControlRight") return "right-ctrl";
  if (code === "AltRight") return "right-alt";
  const f = /^F(1[3-9]|2[0-4])$/.exec(code);
  return f ? (`f${f[1]}` as PushToTalkKey) : null;
}

export function keyLabel(key: PushToTalkKey): string {
  switch (key) {
    case "caps-lock":
      return "Caps Lock";
    case "right-ctrl":
      return "Right Ctrl";
    case "right-alt":
      return "Right Alt";
    default:
      return key.toUpperCase();
  }
}

export function megabytes(bytes: number): string {
  const mb = bytes / 1e6;
  return mb >= 1000 ? `${(mb / 1000).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

/** An engine spec's name: "local:nemotron-stream" → the model's name, "deepgram" → "Deepgram". */
export function engineName(spec: string, view: SettingsView): string {
  const model = view.models.find((m) => m.spec === spec);
  if (model) return model.name;
  const kind = spec.split(":")[0];
  const cloud = view.cloud.find((c) => c.spec === spec || c.spec === kind);
  if (cloud) return cloud.name;
  return spec.startsWith("local:") ? spec.slice(6) : spec;
}

export const PROVIDER_NAMES: Record<string, string> = {
  anthropic: "Anthropic",
  deepgram: "Deepgram",
  groq: "Groq",
  sarvam: "Sarvam",
};

// The names of the choices, written once so no page spells them its own way.
export const TONE_NAMES: Record<string, string> = { neutral: "Neutral", casual: "Casual", formal: "Formal" };
export const LANGUAGE_NAMES: Record<string, string> = { auto: "Auto", english: "English", hinglish: "Hinglish" };
export const FORMAT_NAMES: Record<Format, string> = { plain: "Plain text", markdown: "Markdown", html: "Rich text" };

/** "1 word", "3 words". */
export function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/** A message from Rust as a sentence: a capital first letter and a final period. */
export function sentence(message: string): string {
  const text = message.trim();
  if (!text) return "";
  const capped = text[0].toUpperCase() + text.slice(1);
  return /[.!?…]$/.test(capped) ? capped : `${capped}.`;
}

/** A download's progress, 0–100, or `null` once the bytes are in and it is being checked. */
export function downloadPercent(d: DownloadEvent): number | null {
  if (d.phase !== "downloading") return null;
  return d.total ? Math.round((d.downloaded / d.total) * 100) : 0;
}

/** The line under a download's bar: "120 MB of 480 MB", then what it is doing after. */
export function downloadCaption(d: DownloadEvent, fallbackTotal = 0): string {
  if (d.phase === "verifying") return "Checking the download…";
  if (d.phase !== "downloading") return "Unpacking…";
  return `${megabytes(d.downloaded)} of ${megabytes(d.total || fallbackTotal)}`;
}

/**
 * Checks a key with its provider, then saves it under `provider`. A key the provider
 * refuses is not saved. One that couldn't be checked (no network) is saved, with a
 * `note` saying so. `name` is the provider's name for the messages.
 */
export async function saveCheckedKey(
  provider: string,
  key: string,
  name: string = PROVIDER_NAMES[provider] ?? provider,
): Promise<{ saved: true; hint: string | null; note: string | null } | { saved: false; error: string }> {
  let note: string | null = null;
  try {
    const tested = await api.testKey(provider, key);
    if (tested.result === "refused") return { saved: false, error: `${name} refused this key.` };
    if (tested.result === "failed") note = `Saved, but it couldn’t be checked: ${sentence(tested.message)}`;
  } catch (e) {
    note = `Saved, but it couldn’t be checked: ${sentence(String(e))}`;
  }
  try {
    return { saved: true, hint: await api.saveKey(provider, key), note };
  } catch (e) {
    return { saved: false, error: sentence(String(e)) };
  }
}
