// The payloads the Rust app controller emits (src-tauri/src/controller.rs, `AppEvent`).
// The webviews only render these; dictation logic stays in Rust.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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
  | { kind: "polished"; output: Output }
  | { kind: "notice"; message: string }
  | { kind: "error"; message: string };

export function onAppEvent(handler: (e: AppEvent) => void): Promise<UnlistenFn> {
  return listen<AppEvent>(EVENT, (e) => handler(e.payload));
}
