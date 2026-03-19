use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter};

use walkietalk_shared::audio::AudioFrame;

/// Samples per frame at 16 kHz mono and 20 ms frame duration.
const FRAME_SAMPLES: usize = 320;
/// Target sample rate for Opus decoding.
const TARGET_SAMPLE_RATE: u32 = 16_000;
/// Jitter buffer target: 3 frames = 60 ms.
const JITTER_BUFFER_FRAMES: usize = 3;
/// Minimum interval between audio_level events emitted to the WebView.
const LEVEL_EMIT_INTERVAL: Duration = Duration::from_millis(67); // ~15 Hz

/// Wrapper to make `cpal::Stream` usable across threads.
///
/// See capture.rs for the safety rationale.
struct SendStream(cpal::Stream);

// SAFETY: Desktop-only (WASAPI/CoreAudio/ALSA). Stream is internally
// thread-safe; we only call pause() once on stop.
unsafe impl Send for SendStream {}

/// Handle for the playback pipeline. Drop to stop.
pub struct PlaybackHandle {
    stream: SendStream,
    stop: Arc<AtomicBool>,
    /// Thread-safe queue for incoming decoded PCM frames.
    buffer: Arc<Mutex<JitterBuffer>>,
}

/// Simple jitter buffer: a ring of decoded PCM frames.
struct JitterBuffer {
    frames: VecDeque<Vec<f32>>,
    last_seq: Option<u32>,
}

impl JitterBuffer {
    fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(JITTER_BUFFER_FRAMES + 2),
            last_seq: None,
        }
    }

    fn push(&mut self, seq: u32, pcm: Vec<f32>) {
        // Simple sequence-based ordering: drop late frames
        if let Some(last) = self.last_seq {
            if seq <= last {
                return;
            }
        }
        self.last_seq = Some(seq);
        self.frames.push_back(pcm);
        // Cap buffer size
        while self.frames.len() > JITTER_BUFFER_FRAMES + 2 {
            self.frames.pop_front();
        }
    }

    fn pop(&mut self) -> Option<Vec<f32>> {
        self.frames.pop_front()
    }

    fn len(&self) -> usize {
        self.frames.len()
    }
}

impl PlaybackHandle {
    /// Feed an incoming AudioFrame into the jitter buffer.
    pub fn push_frame(&self, frame: &AudioFrame) -> Result<(), String> {
        // Decode Opus payload to PCM.
        let mut decoder = DECODER.lock().map_err(|e| e.to_string())?;
        let decoder = decoder.get_or_insert_with(|| {
            audiopus::coder::Decoder::new(
                audiopus::SampleRate::Hz16000,
                audiopus::Channels::Mono,
            )
            .expect("Failed to create Opus decoder")
        });

        let mut pcm = vec![0i16; FRAME_SAMPLES];
        let decoded_samples = if frame.payload.is_empty() {
            // PLC (Packet Loss Concealment): pass None to decode
            let output: audiopus::MutSignals<'_, i16> = (&mut pcm)
                .try_into()
                .map_err(|e: audiopus::Error| format!("MutSignals: {e}"))?;
            decoder
                .decode(None, output, false)
                .map_err(|e| format!("Opus PLC error: {e}"))?
        } else {
            let packet: audiopus::packet::Packet<'_> = (&frame.payload[..])
                .try_into()
                .map_err(|e: audiopus::Error| format!("Packet: {e}"))?;
            let output: audiopus::MutSignals<'_, i16> = (&mut pcm)
                .try_into()
                .map_err(|e: audiopus::Error| format!("MutSignals: {e}"))?;
            decoder
                .decode(Some(packet), output, false)
                .map_err(|e| format!("Opus decode error: {e}"))?
        };

        // Convert i16 → f32
        let pcm_f32: Vec<f32> = pcm[..decoded_samples]
            .iter()
            .map(|&s| s as f32 / i16::MAX as f32)
            .collect();

        let mut buf = self.buffer.lock().map_err(|e| e.to_string())?;
        buf.push(frame.sequence_num, pcm_f32);
        Ok(())
    }

    /// Stop the playback stream and reset the Opus decoder.
    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        self.stream.0.pause().ok();
        // Reset decoder so PLC state from this session doesn't leak into the next.
        if let Ok(mut dec) = DECODER.lock() {
            *dec = None;
        }
    }
}

// Thread-local Opus decoder (reused across frames for PLC state).
static DECODER: Mutex<Option<audiopus::coder::Decoder>> = Mutex::new(None);

/// Start a playback output stream. Returns a handle to push decoded frames and stop.
pub fn start_playback(app: AppHandle) -> Result<PlaybackHandle, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No audio output device found".to_string())?;

    // Query the device's default output config.
    let default_cfg = device
        .default_output_config()
        .map_err(|e| format!("Failed to get default output config: {e}"))?;

    let native_sample_rate = default_cfg.sample_rate().0;
    let native_channels = default_cfg.channels();
    tracing::info!(
        "Audio output device: {:?}, native config: {}Hz {}ch",
        device.name().unwrap_or_default(),
        native_sample_rate,
        native_channels
    );

    let config = cpal::StreamConfig {
        channels: native_channels,
        sample_rate: cpal::SampleRate(native_sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let buffer = Arc::new(Mutex::new(JitterBuffer::new()));

    let buf_reader = buffer.clone();
    let app_clone = app.clone();
    let mut last_level_emit = Instant::now();

    // Pre-compute resampling parameters.
    let need_resample = native_sample_rate != TARGET_SAMPLE_RATE;
    let upsample_ratio = native_sample_rate as f64 / TARGET_SAMPLE_RATE as f64;
    let ch = native_channels as usize;

    // Resampler state (lives in the output callback closure).
    let mut resample_buf: Vec<f32> = Vec::new();
    let mut resample_pos: f64 = 0.0;
    let mut residual: Vec<f32> = Vec::new();

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                // Number of mono samples we need to fill the output buffer.
                let output_mono_samples = data.len() / ch;
                let mut mono_out: Vec<f32> = Vec::with_capacity(output_mono_samples);

                // Drain residual from previous callback first.
                if !residual.is_empty() {
                    let take = output_mono_samples.min(residual.len());
                    mono_out.extend_from_slice(&residual[..take]);
                    residual.drain(..take);
                }

                // Pull frames from jitter buffer until we have enough.
                while mono_out.len() < output_mono_samples {
                    let frame = {
                        let mut buf = buf_reader.lock().unwrap();
                        buf.pop()
                    };
                    let frame = frame.unwrap_or_else(|| vec![0.0; FRAME_SAMPLES]);

                    // Emit audio level (throttled)
                    if last_level_emit.elapsed() >= LEVEL_EMIT_INTERVAL {
                        last_level_emit = Instant::now();
                        let rms = compute_rms(&frame);
                        let _ = app_clone.emit(
                            "audio_level",
                            serde_json::json!({
                                "direction": "recv",
                                "level": rms,
                            }),
                        );
                    }

                    if need_resample {
                        // Upsample from 16 kHz to native rate using linear interp.
                        resample_buf.clear();
                        let in_len = frame.len();
                        while resample_pos < in_len as f64 {
                            let idx = resample_pos as usize;
                            let frac = resample_pos - idx as f64;
                            let s0 = frame[idx.min(in_len - 1)];
                            let s1 = frame[(idx + 1).min(in_len - 1)];
                            resample_buf.push(s0 + (s1 - s0) * frac as f32);
                            resample_pos += 1.0 / upsample_ratio;
                        }
                        resample_pos -= in_len as f64;
                        mono_out.extend_from_slice(&resample_buf);
                    } else {
                        mono_out.extend_from_slice(&frame);
                    }
                }

                // If we generated more than needed, store residual.
                if mono_out.len() > output_mono_samples {
                    residual.extend_from_slice(&mono_out[output_mono_samples..]);
                    mono_out.truncate(output_mono_samples);
                }

                // Write to output: duplicate mono to all channels.
                for (i, sample) in mono_out.iter().enumerate() {
                    for c in 0..ch {
                        let idx = i * ch + c;
                        if idx < data.len() {
                            data[idx] = *sample;
                        }
                    }
                }
            },
            move |err| {
                tracing::error!("CPAL output error: {err}");
            },
            None,
        )
        .map_err(|e| format!("Failed to build output stream: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("Failed to start output stream: {e}"))?;

    let stream = SendStream(stream);

    Ok(PlaybackHandle {
        stream,
        stop,
        buffer,
    })
}

/// Compute RMS (root mean square) of a PCM buffer, normalized to 0.0–1.0.
fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}
