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

/** What the settings window edits (src-tauri/src/form.rs, `Form`). */
export interface Form {
  push_to_talk: PushToTalkKey;
  autostart: boolean;
  language: LanguageMode;
  script: Script;
  engines: string[];
  dictionary: string[];
  polish: boolean;
  tone: string;
  polish_apps: Record<string, boolean>;
  history: boolean;
  retention_days: number;
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

export interface AppRow {
  name: string;
  exes: string[];
  format: Format;
  code_mode: boolean;
  polish: boolean | null;
}

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
  apps: AppRow[];
  version: string;
  paths: { settings: string; models: string; history: string };
  ram_mb: number;
  onboarding_step: number | null;
}

/** Onboarding's mic check (src-tauri/src/mic.rs, `MicCheck`). */
export interface MicCheck {
  device: string;
  leading_ms: number | null;
  peak_rms: number;
  heard: boolean;
  late: boolean;
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
export interface ProviderReports {
  deepgram: Report<DeepgramReport>;
  anthropic: Report<AnthropicReport>;
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
  | { state: "available"; version: string; notes: string | null }
  | { state: "downloading"; downloaded: number; total: number | null }
  | { state: "error"; message: string; manual: boolean };

/** About's update section (src-tauri/src/updates.rs, `View`). Bolo and Bolo Nightly are
 *  separate apps, each updating from its own feed (ADR-0019). */
export interface UpdateView {
  version: string;
  product: "Bolo" | "Bolo Nightly";
  variant: "release" | "nightly";
  badge: "Beta" | "Nightly" | null;
  /** The other app, to install next to this one. */
  other: { product: string; url: string };
  status: UpdateStatus;
}

/** Which build this is (ADR-0019): a Nightly from `main`, or a release (Beta until 1.0). */
export function variant(version: string): {
  name: "Nightly" | "Beta" | "Release";
  nightly: boolean;
} {
  if (version.includes("-nightly.")) return { name: "Nightly", nightly: true };
  return {
    name: version.startsWith("0.") ? "Beta" : "Release",
    nightly: false,
  };
}

export const api = {
  settings: () => invoke<SettingsView>("settings_get"),
  save: (form: Form) => invoke<SettingsView>("settings_save", { form }),
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

/** RMS on a log scale, 0–1, for a level meter (speech sits around 0.01–0.2). */
export function meter(rms: number): number {
  return Math.max(0, Math.min(1, (Math.log10(Math.max(rms, 1e-4)) + 4) / 3.3));
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
