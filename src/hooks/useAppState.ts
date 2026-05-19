import { useEffect, useState, useCallback } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type {
  AppStatus,
  AppConfig,
  AudioDevice,
  ModelInfo,
  DownloadProgress,
  StatusChangedPayload,
  ErrorPayload,
} from "../types";

export function useAppState() {
  const [status, setStatus] = useState<AppStatus>("idle");
  const [lastText, setLastText] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    listen<StatusChangedPayload>("status-changed", (e) => {
      setStatus(e.payload.status);
    }).then((fn) => unlisteners.push(fn));

    listen<{ text: string }>("transcription-complete", (e) => {
      setLastText(e.payload.text);
    }).then((fn) => unlisteners.push(fn));

    listen<ErrorPayload>("talka-error", (e) => {
      setError(e.payload.message);
      setTimeout(() => setError(null), 4000);
    }).then((fn) => unlisteners.push(fn));

    return () => unlisteners.forEach((fn) => fn());
  }, []);

  return { status, lastText, error };
}

export function useConfig() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [downloadProgress, setDownloadProgress] =
    useState<DownloadProgress | null>(null);
  const [saving, setSaving] = useState(false);

  const reload = useCallback(async () => {
    const [cfg, devs, mdls] = await Promise.all([
      invoke<AppConfig>("get_config"),
      invoke<AudioDevice[]>("get_audio_devices"),
      invoke<ModelInfo[]>("get_models"),
    ]);
    setConfig(cfg);
    setDevices(devs);
    setModels(mdls);
  }, []);

  useEffect(() => {
    reload();

    const unlisteners: UnlistenFn[] = [];

    listen<DownloadProgress>("download-progress", (e) => {
      setDownloadProgress(e.payload);
      if (e.payload.progress >= 100) {
        setDownloadProgress(null);
        reload();
      }
    }).then((fn) => unlisteners.push(fn));

    return () => unlisteners.forEach((fn) => fn());
  }, [reload]);

  const saveConfig = useCallback(
    async (updated: AppConfig) => {
      setSaving(true);
      try {
        await invoke("set_config", { config: updated });
        setConfig(updated);
      } finally {
        setSaving(false);
      }
    },
    [],
  );

  const downloadModel = useCallback(async (modelName: string) => {
    await invoke("download_model", { modelName });
  }, []);

  return {
    config,
    devices,
    models,
    downloadProgress,
    saving,
    saveConfig,
    downloadModel,
    reload,
  };
}
