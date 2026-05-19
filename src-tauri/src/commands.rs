use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    audio::{audio_rms, list_input_devices, normalize_audio, pad_audio, resample_to_16k, trim_silence, AudioDevice, Recorder},
    state::{AppConfig, AppStatus, SharedState},
    transcriber::{list_models, model_path, models_dir, ModelInfo},
};

// ---------------------------------------------------------------------------
// Event helpers
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct StatusPayload {
    status: AppStatus,
}

#[derive(Serialize, Clone)]
struct TranscriptionPayload {
    text: String,
}

#[derive(Serialize, Clone)]
struct ErrorPayload {
    message: String,
}

fn emit_status(app: &AppHandle, status: AppStatus) {
    let _ = app.emit("status-changed", StatusPayload { status });
}

fn emit_error(app: &AppHandle, msg: impl ToString) {
    let _ = app.emit(
        "talka-error",
        ErrorPayload {
            message: msg.to_string(),
        },
    );
}

fn show_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        if let Ok(Some(m)) = w.primary_monitor() {
            let size = m.size();
            let scale = m.scale_factor();
            let x = (size.width as f64 / scale) as i32 - 240;
            let y = (size.height as f64 / scale) as i32 - 80;
            let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        }
        let _ = w.show();
    }
}

fn hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

fn set_idle(app: &AppHandle, shared: &SharedState) {
    shared.lock().status = AppStatus::Idle;
    emit_status(app, AppStatus::Idle);
    hide_overlay(app);
}

// ---------------------------------------------------------------------------
// Recording control (called from lib.rs shortcut handler)
// ---------------------------------------------------------------------------

/// Begin recording. Spawns a thread that owns the whole
/// record → resample → transcribe → type pipeline.
pub fn start_recording(app: &AppHandle, shared: &SharedState) {
    {
        let mut s = shared.lock();
        if s.status != AppStatus::Idle {
            return;
        }
        s.status = AppStatus::Recording;
    }
    emit_status(app, AppStatus::Recording);
    show_overlay(app);

    let shared = shared.clone();
    let app = app.clone();

    std::thread::spawn(move || {
        let (device, config) = {
            let s = shared.lock();
            (s.config.input_device.clone(), s.config.clone())
        };

        // --- Start microphone ---
        let mut recorder = Recorder::new();
        if let Err(e) = recorder.start(device.as_deref()) {
            emit_error(&app, format!("Microphone error: {e}"));
            set_idle(&app, &shared);
            return;
        }

        // --- Wait until stop_recording sets status to Transcribing ---
        loop {
            std::thread::sleep(Duration::from_millis(20));
            if shared.lock().status != AppStatus::Recording {
                break;
            }
        }

        // If status was externally reset to Idle, discard.
        if shared.lock().status != AppStatus::Transcribing {
            set_idle(&app, &shared);
            return;
        }

        // --- Capture audio ---
        let (audio, rate) = recorder.stop();

        if audio.len() < 3_200 {
            // < 0.2 s — ignore
            set_idle(&app, &shared);
            return;
        }

        // --- Resample to 16 kHz ---
        let resampled = resample_to_16k(&audio, rate);

        // --- Silence gate ---
        if audio_rms(&resampled) < 0.002 {
            set_idle(&app, &shared);
            return;
        }

        // --- Normalize, trim silence edges, add small padding ---
        let resampled = normalize_audio(&resampled);
        let trimmed   = trim_silence(&resampled);
        if trimmed.is_empty() {
            set_idle(&app, &shared);
            return;
        }
        let resampled = pad_audio(trimmed, 100); // 100 ms silence each side

        // --- Resolve model path ---
        let data_dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        let dir = models_dir(&data_dir);
        let path = model_path(&dir, &config.model);

        if !path.exists() {
            emit_error(
                &app,
                format!(
                    "Model '{}' is not downloaded. Open Settings and download it first.",
                    config.model
                ),
            );
            set_idle(&app, &shared);
            return;
        }

        // --- Transcribe (reuse persistent loaded model across recordings) ---
        // Only show "loading model" on the first recording; after that the model
        // stays in memory so we go straight to Transcribing.
        if !crate::transcriber::shared().lock().is_loaded_for(&path) {
            emit_status(&app, AppStatus::LoadingModel);
        }
        emit_status(&app, AppStatus::Transcribing);

        let transcription_result = {
            let mut transcriber = crate::transcriber::shared().lock();
            if let Err(e) = transcriber.load(&path) {
                emit_error(&app, format!("Failed to load model: {e}"));
                set_idle(&app, &shared);
                return;
            }
            let lang = config.language.as_deref();
            transcriber.transcribe(&resampled, lang)
        };

        match transcription_result {
            Ok(text) if !text.is_empty() => {
                let _ = app.emit(
                    "transcription-complete",
                    TranscriptionPayload { text: text.clone() },
                );
                if let Err(e) = crate::output::output_text(&text, &config.output_method) {
                    emit_error(&app, format!("Could not output text: {e}"));
                }
            }
            Ok(_) => {} // Silence / nothing recognised
            Err(e) => emit_error(&app, format!("Transcription error: {e}")),
        }

        set_idle(&app, &shared);
    });
}

/// Flip status from Recording → Transcribing so the recording thread
/// finishes capturing and begins processing.
pub fn stop_recording(app: &AppHandle, shared: &SharedState) {
    {
        let mut state = shared.lock();
        if state.status != AppStatus::Recording {
            return;
        }
        state.status = AppStatus::Transcribing;
    }
    emit_status(app, AppStatus::Transcribing);
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_config(state: tauri::State<SharedState>) -> AppConfig {
    state.lock().config.clone()
}

#[tauri::command]
pub async fn set_config(
    config: AppConfig,
    state: tauri::State<'_, SharedState>,
    app: AppHandle,
) -> Result<(), String> {
    let cfg_clone = config.clone();
    state.lock().config = config;
    crate::save_config(&app, &cfg_clone);
    crate::register_shortcut(&app, state.inner().clone()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_audio_devices() -> Vec<AudioDevice> {
    list_input_devices()
}

#[tauri::command]
pub fn get_models(app: AppHandle) -> Vec<ModelInfo> {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    list_models(&data_dir)
}

#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub model: String,
    pub progress: u8,
    pub downloaded: u64,
    pub total: u64,
}

/// Shared download logic — callable from both the Tauri command and internal setup.
pub async fn do_download(model_name: String, app: &AppHandle) -> Result<(), String> {
    use crate::transcriber::MODELS;
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let url = MODELS
        .iter()
        .find(|(n, ..)| *n == model_name)
        .map(|(.., url)| *url)
        .ok_or_else(|| format!("Unknown model: {model_name}"))?;

    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let dir = models_dir(&data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;

    let dest = model_path(&dir, &model_name);
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let progress = if total > 0 {
            (downloaded * 100 / total) as u8
        } else {
            0
        };
        let _ = app.emit(
            "download-progress",
            DownloadProgress {
                model: model_name.clone(),
                progress,
                downloaded,
                total,
            },
        );
    }

    // Emit 100% complete
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            model: model_name,
            progress: 100,
            downloaded,
            total,
        },
    );

    Ok(())
}

#[tauri::command]
pub async fn download_model(model_name: String, app: AppHandle) -> Result<(), String> {
    do_download(model_name, &app).await
}

#[tauri::command]
pub fn open_models_dir(app: AppHandle) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let dir = models_dir(&data_dir);
    let _ = std::fs::create_dir_all(&dir);
    open::that(dir).map_err(|e| e.to_string())
}
