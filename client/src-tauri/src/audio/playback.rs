use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter};

use walkietalk_shared::audio::AudioFrame;

/// Samples per frame at 16 kHz mono and 20 ms frame duration.
const FRAME_SAMPLES: usize = 320;
/// Sample rate in Hz.
const SAMPLE_RATE: u32 = 16_000;
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

    /// Stop the playback stream.
    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        self.stream.0.pause().ok();
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

    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let buffer = Arc::new(Mutex::new(JitterBuffer::new()));

    let buf_reader = buffer.clone();
    let app_clone = app.clone();
    let mut last_level_emit = Instant::now();

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                let mut remaining = data.len();
                let mut offset = 0;

                while remaining > 0 {
                    let frame = {
                        let mut buf = buf_reader.lock().unwrap();
                        if buf.len() > 0 {
                            buf.pop().unwrap_or_else(|| vec![0.0; FRAME_SAMPLES])
                        } else {
                            vec![0.0; FRAME_SAMPLES]
                        }
                    };

                    let copy_len = remaining.min(frame.len());
                    data[offset..offset + copy_len].copy_from_slice(&frame[..copy_len]);
                    offset += copy_len;
                    remaining -= copy_len;

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
