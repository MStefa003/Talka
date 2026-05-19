use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

/// List all available audio input devices on the system.
pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devs) => devs
            .filter_map(|d| {
                let name = d.name().ok()?;
                Some(AudioDevice {
                    id: name.clone(),
                    name,
                })
            })
            .collect(),
        Err(_) => vec![],
    }
}

// ---------------------------------------------------------------------------
// Recorder
// ---------------------------------------------------------------------------

pub struct Recorder {
    /// Accumulated float samples (any channel count, device native rate).
    samples: Arc<Mutex<Vec<f32>>>,
    /// Sample rate from the device config.
    sample_rate: Arc<Mutex<u32>>,
    /// The live stream — kept alive as long as recording is active.
    stream: Option<cpal::Stream>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Arc::new(Mutex::new(16_000)),
            stream: None,
        }
    }

    /// Start capturing audio from `device_name` (or system default if `None`).
    pub fn start(&mut self, device_name: Option<&str>) -> Result<()> {
        let host = cpal::default_host();

        let device = if let Some(name) = device_name {
            host.input_devices()?
                .find(|d| d.name().ok().as_deref() == Some(name))
                .context("Audio device not found")?
        } else {
            host.default_input_device()
                .context("No default audio input device")?
        };

        let supported = device.default_input_config()?;
        let rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let fmt = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        *self.sample_rate.lock() = rate;
        self.samples.lock().clear();

        let samples = self.samples.clone();

        let stream = match fmt {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, channels, samples)?,
            SampleFormat::I16 => build_stream::<i16>(&device, &config, channels, samples)?,
            SampleFormat::U16 => build_stream::<u16>(&device, &config, channels, samples)?,
            _ => anyhow::bail!("Unsupported sample format: {:?}", fmt),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    /// Stop recording and return (samples, sample_rate).
    pub fn stop(&mut self) -> (Vec<f32>, u32) {
        // Drop the stream to stop recording.
        self.stream.take();

        let rate = *self.sample_rate.lock();
        let data = std::mem::take(&mut *self.samples.lock());
        (data, rate)
    }

}

// ---------------------------------------------------------------------------
// Audio quality helpers
// ---------------------------------------------------------------------------

/// Root mean square of the signal (0.0 = silence, ~1.0 = full scale).
pub fn audio_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Normalize audio to a fixed target RMS so quiet mics still transcribe well.
/// Applies a maximum gain cap of 20× to avoid amplifying pure noise.
pub fn normalize_audio(samples: &[f32]) -> Vec<f32> {
    const TARGET_RMS: f32 = 0.08;
    const MAX_GAIN: f32 = 20.0;
    let rms = audio_rms(samples);
    if rms < 1e-7 {
        return samples.to_vec(); // Dead silence — don't amplify
    }
    let gain = (TARGET_RMS / rms).min(MAX_GAIN);
    samples.iter().map(|s| (s * gain).clamp(-1.0, 1.0)).collect()
}

/// Trim leading and trailing silence from 16 kHz audio.
/// Uses 10 ms RMS windows; keeps one window of margin before and after speech.
pub fn trim_silence(samples: &[f32]) -> &[f32] {
    const THRESHOLD: f32 = 0.004; // active-speech floor after normalization
    const WINDOW: usize = 160;    // 10 ms at 16 kHz
    const STEP: usize = 80;       // 5 ms step (50 % overlap)

    if samples.len() < WINDOW * 2 {
        return samples;
    }

    // Scan forward — find first window above threshold
    let mut start = 0usize;
    let mut found = false;
    let mut i = 0usize;
    while i + WINDOW <= samples.len() {
        if audio_rms(&samples[i..i + WINDOW]) > THRESHOLD {
            start = i.saturating_sub(WINDOW); // one window of margin before
            found = true;
            break;
        }
        i += STEP;
    }
    if !found {
        return &samples[0..0]; // Pure silence
    }

    // Scan backward — find last window above threshold
    let mut end = samples.len();
    let total = samples.len();
    let mut j = total.saturating_sub(WINDOW);
    loop {
        let w_end = (j + WINDOW).min(total);
        if audio_rms(&samples[j..w_end]) > THRESHOLD {
            end = (j + WINDOW * 2).min(total); // one window of margin after
            break;
        }
        if j < STEP {
            break;
        }
        j -= STEP;
    }

    if start < end { &samples[start..end] } else { samples }
}

/// Pad audio with `pad_ms` milliseconds of silence on each side.
/// A small leading silence dramatically improves Whisper decoder accuracy.
pub fn pad_audio(samples: &[f32], pad_ms: usize) -> Vec<f32> {
    let pad = pad_ms * 16; // samples at 16 kHz (rate/1000)
    let mut out = vec![0.0f32; pad];
    out.extend_from_slice(samples);
    out.extend(std::iter::repeat(0.0f32).take(pad));
    out
}

// ---------------------------------------------------------------------------
// Generic stream builder for any cpal sample type
// ---------------------------------------------------------------------------

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    out: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + ToF32,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut buf = out.lock();
            // Mix down to mono
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|frame| {
                    let sum: f32 = frame.iter().map(|s| s.to_f32()).sum();
                    sum / channels as f32
                })
                .collect();
            buf.extend_from_slice(&mono);
        },
        |err| tracing::error!("Audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}

// ---------------------------------------------------------------------------
// Convert any cpal sample type to f32
// ---------------------------------------------------------------------------

pub trait ToF32 {
    fn to_f32(&self) -> f32;
}

impl ToF32 for f32 {
    fn to_f32(&self) -> f32 {
        *self
    }
}
impl ToF32 for i16 {
    fn to_f32(&self) -> f32 {
        *self as f32 / 32_768.0
    }
}
impl ToF32 for u16 {
    fn to_f32(&self) -> f32 {
        (*self as f32 / 32_768.0) - 1.0
    }
}

// ---------------------------------------------------------------------------
// Resample to 16 kHz mono (Whisper requirement)
// ---------------------------------------------------------------------------

pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == 16_000 {
        return samples.to_vec();
    }

    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
        WindowFunction,
    };

    let ratio = 16_000.0 / from_rate as f64;
    let params = SincInterpolationParameters {
        sinc_len: 64,
        f_cutoff: 0.925,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 16,
        window: WindowFunction::BlackmanHarris2,
    };

    let chunk = samples.len();
    let mut resampler = match SincFixedIn::<f32>::new(ratio, 2.0, params, chunk, 1) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Resampler init failed: {e}");
            return samples.to_vec();
        }
    };

    let waves_in = vec![samples.to_vec()];
    match resampler.process(&waves_in, None) {
        Ok(out) => out.into_iter().next().unwrap_or_default(),
        Err(e) => {
            tracing::error!("Resampling failed: {e}");
            samples.to_vec()
        }
    }
}
