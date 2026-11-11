// The payloads the Rust app controller emits (src-tauri/src/controller.rs, `AppEvent`).
// The webviews only render these; dictation logic stays in Rust.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { UpdateStatus } from "./api";

export const EVENT = "bolo://event";

export type DictationState = "Idle" | "Listening" | "Processing" | "Done";

export interface Timings {
  stt_ms: number;
  rules_ms: number;
  llm_ms: number;
  format_ms: number;
  total_ms: number;
}

export interface Output {
  plain: string;
  markdown: string;
  html: string | null;
  timings: Timings;
  engine_id: string;
  language: "auto" | "english" | "hinglish";
}

export type AppEvent =
  | { kind: "state"; state: DictationState }
  | { kind: "paused"; paused: boolean }
  | { kind: "level"; rms: number }
  | { kind: "partial"; text: string }
  | { kind: "final"; output: Output }
  | { kind: "polished"; output: Output; swapped: boolean }
  | { kind: "notice"; message: string; hint: string | null }
  | { kind: "error"; message: string }
  | { kind: "pill"; view: PillView }
  | { kind: "update"; status: UpdateStatus };

/// What the overlay pill shows (src-tauri/src/pill.rs, `View`). The state machine and its
/// timers live in Rust; the pill webview only draws.
export type PillView =
  | { state: "hidden" | "transcribing" | "inserted" | "polishing" | "polished" | "cancelled" }
  | { state: "idle"; hover: boolean; key: string }
  | { state: "listening"; hands_free: boolean }
  | { state: "notice"; message: string; hint: string | null }
  | { state: "error"; message: string };

export function onAppEvent(handler: (e: AppEvent) => void): Promise<UnlistenFn> {
  return listen<AppEvent>(EVENT, (e) => handler(e.payload));
}
