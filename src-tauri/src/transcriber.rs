use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub label: String,
    pub size_mb: u32,
    pub downloaded: bool,
    pub path: Option<String>,
}

pub const MODELS: &[(&str, &str, u32, &str)] = &[
    (
        "ggml-tiny",
        "Tiny (75 MB) — fastest",
        75,
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
    ),
    (
        "ggml-base",
        "Base (142 MB) — recommended",
        142,
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
    ),
    (
        "ggml-small",
        "Small (483 MB) — better accuracy",
        483,
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
    ),
    (
        "ggml-medium",
        "Medium (1.5 GB) — high accuracy",
        1500,
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
    ),
];

pub fn models_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models")
}

pub fn model_path(models_dir: &Path, name: &str) -> PathBuf {
    models_dir.join(format!("{name}.bin"))
}

pub fn list_models(app_data_dir: &Path) -> Vec<ModelInfo> {
    let dir = models_dir(app_data_dir);
    MODELS
        .iter()
        .map(|(name, label, size_mb, _)| {
            let path = model_path(&dir, name);
            let downloaded = path.exists();
            ModelInfo {
                name: name.to_string(),
                label: label.to_string(),
                size_mb: *size_mb,
                downloaded,
                path: if downloaded {
                    Some(path.to_string_lossy().into_owned())
                } else {
                    None
                },
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shared singleton — persists the loaded model across recordings
// ---------------------------------------------------------------------------

// Safety: WhisperContext wraps a raw C pointer. Access is serialised by the
// Mutex, and only one recording runs at a time (guarded by AppStatus).
unsafe impl Send for Transcriber {}

static SHARED: LazyLock<Mutex<Transcriber>> =
    LazyLock::new(|| Mutex::new(Transcriber::new()));

pub fn shared() -> &'static Mutex<Transcriber> {
    &SHARED
}

// ---------------------------------------------------------------------------
// Transcriber
// ---------------------------------------------------------------------------

pub struct Transcriber {
    context: Option<WhisperContext>,
    loaded_model: Option<String>,
}

impl Transcriber {
    pub fn new() -> Self {
        Self {
            context: None,
            loaded_model: None,
        }
    }

    /// Load (or reload) a model from disk. Skips loading if already loaded.
    pub fn load(&mut self, model_path: &Path) -> Result<()> {
        let key = model_path.to_string_lossy().into_owned();
        if self.loaded_model.as_deref() == Some(&key) {
            return Ok(());
        }
        tracing::info!("Loading Whisper model: {}", key);
        let ctx = WhisperContext::new_with_params(
            &key,
            WhisperContextParameters::default(),
        )
        .context("Failed to load Whisper model — ensure the model file exists")?;

        self.context = Some(ctx);
        self.loaded_model = Some(key);
        Ok(())
    }

    /// Returns true when this exact model file is already loaded in memory.
    pub fn is_loaded_for(&self, path: &Path) -> bool {
        self.loaded_model.as_deref() == Some(path.to_string_lossy().as_ref())
    }

    /// Transcribe 16 kHz mono f32 samples.  Returns trimmed text.
    pub fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String> {
        let ctx = self.context.as_ref().context("Model not loaded")?;
        let mut state = ctx.create_state().context("Failed to create Whisper state")?;

        let threads = std::thread::available_parallelism()
            .map(|n| (n.get() as i32).min(8))
            .unwrap_or(4);

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language);
        params.set_n_threads(threads);
        // Hard token cap — prevents infinite repetition loops.
        params.set_max_tokens(200);
        params.set_temperature(0.0);
        // Re-enable Whisper's built-in fallback: if output looks bad (high
        // entropy / repetitive), it retries with progressively higher temperature.
        params.set_temperature_inc(0.2);
        params.set_entropy_thold(2.8);
        params.set_logprob_thold(-1.0);
        params.set_translate(false);
        params.set_no_context(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Speed: treat every utterance as one segment (no multi-segment overhead)
        // and cap the encoder context to ~15 s (768 frames) instead of 30 s.
        // For typical dictation clips this halves encoder compute time.
        params.set_single_segment(true);
        params.set_audio_ctx(768);

        state
            .full(params, audio)
            .context("Whisper transcription failed")?;

        let n = state.full_n_segments();
        let text: String = (0..n)
            .filter_map(|i| state.get_segment(i))
            .filter_map(|seg| seg.to_str_lossy().ok().map(|s| s.into_owned()))
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();

        // Guard against repetition loops that slip through the token cap.
        if is_repetitive(&text) {
            return Ok(String::new());
        }

        Ok(text)
    }
}

/// Returns true when any word or short phrase repeats 5+ times consecutively —
/// a reliable sign of a Whisper hallucination loop.
fn is_repetitive(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 10 {
        return false;
    }
    // Sliding window: check if any ngram (1-3 words) repeats ≥5 times in a row.
    for n in 1usize..=3 {
        let mut run = 1usize;
        for i in n..words.len() {
            if words[i - n..i] == words[i..i + n.min(words.len() - i)] {
                run += 1;
                if run >= 5 {
                    return true;
                }
            } else {
                run = 1;
            }
        }
    }
    false
}
