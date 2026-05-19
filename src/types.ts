/** Shared types between frontend and backend (must match Rust serde shapes). */

export type AppStatus = "idle" | "recording" | "transcribing" | "loading_model";

export type HotkeyMode = "push_to_talk" | "toggle";

export type OutputMethod = "paste" | "type";

export interface AppConfig {
  hotkey: string;
  mode: HotkeyMode;
  model: string;
  language: string | null;
  output_method: OutputMethod;
  input_device: string | null;
}

export interface AudioDevice {
  id: string;
  name: string;
}

export interface ModelInfo {
  name: string;
  label: string;
  size_mb: number;
  downloaded: boolean;
  path: string | null;
}

export interface DownloadProgress {
  model: string;
  progress: number;
  downloaded: number;
  total: number;
}

export type StatusChangedPayload = { status: AppStatus };
export type TranscriptionPayload = { text: string };
export type ErrorPayload = { message: string };
