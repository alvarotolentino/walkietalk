use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::transport::ws::WsWriteTx;
use walkietalk_shared::audio::AudioFrame;

/// Samples per frame at 16 kHz mono and 20 ms frame duration.
const FRAME_SAMPLES: usize = 320;
/// Target sample rate for Opus encoding.
const TARGET_SAMPLE_RATE: u32 = 16_000;
/// Opus bitrate in bits per second (VOIP 16 kbps).
const OPUS_BITRATE: i32 = 16_000;
/// Max encoded frame size in bytes (Opus guarantees under this for 20ms at 16kbps).
const MAX_ENCODED_BYTES: usize = 256;
/// Minimum interval between audio_level events emitted to the WebView.
const LEVEL_EMIT_INTERVAL: Duration = Duration::from_millis(67); // ~15 Hz

/// Wrapper to make `cpal::Stream` usable across threads.
///
/// CPAL streams on all desktop backends (WASAPI, CoreAudio, ALSA) are
/// internally thread-safe — pause/play/drop can be called from any thread.
/// The crate simply doesn't declare `Send` because some niche backends
/// are not. We only target desktop here.
struct SendStream(cpal::Stream);

// SAFETY: We only target desktop (WASAPI/CoreAudio/ALSA) where
// cpal::Stream is internally thread-safe. The Stream is never accessed
// concurrently — we only call pause() once on stop.
unsafe impl Send for SendStream {}

/// Handle returned when capture starts. Drop it to stop.
pub struct CaptureHandle {
    stream: SendStream,
    stop: Arc<AtomicBool>,
}

impl CaptureHandle {
    /// Signal the encoding loop to stop, then pause the CPAL stream.
    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        self.stream.0.pause().ok();
    }
}

/// Start mic capture → Opus encode → WS binary send.
///
/// Returns a `CaptureHandle` that must be kept alive for the duration of the
/// transmission. Dropping or calling `stop()` ends capture.
///
/// `room_id_u64` and `speaker_id_u32` are pre-computed numeric IDs for the
/// AudioFrame header. `write_tx` is the WebSocket write channel.
pub fn start_capture(
    app: AppHandle,
    room_id_u64: u64,
    speaker_id_u32: u32,
    write_tx: WsWriteTx,
) -> Result<CaptureHandle, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No audio input device found".to_string())?;

    // Query the device's default input config instead of hardcoding 16 kHz.
    // Most devices (especially on Windows/WASAPI) support 44100 or 48000 Hz
    // but not 16000 Hz directly. We'll resample in the encode loop.
    let default_cfg = device
        .default_input_config()
        .map_err(|e| format!("Failed to get default input config: {e}"))?;

    let native_sample_rate = default_cfg.sample_rate().0;
    let native_channels = default_cfg.channels();
    tracing::info!(
        "Audio input device: {:?}, native config: {}Hz {}ch",
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
    let stop_flag = stop.clone();

    // Channel for raw PCM samples from CPAL callback → encoding thread.
    let (pcm_tx, pcm_rx) = mpsc::unbounded_channel::<Vec<f32>>();

    // Build the CPAL input stream (callback runs on audio thread).
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                // Send a copy to the encoding thread. If the channel is full,
                // the data is dropped (acceptable for real-time audio).
                let _ = pcm_tx.send(data.to_vec());
            },
            move |err| {
                tracing::error!("CPAL input error: {err}");
            },
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("Failed to start input stream: {e}"))?;

    let stream = SendStream(stream);

    // Capture the tokio runtime handle before spawning — the new OS thread
    // won't inherit it automatically.
    let rt_handle = tokio::runtime::Handle::try_current()
        .map_err(|_| "No tokio runtime available to start audio capture".to_string())?;

    // Spawn the encoding task on a dedicated thread (Opus encoder is not Send
    // across await points and we need real-time guarantees).
    let stop_enc = stop.clone();
    std::thread::Builder::new()
        .name("audio-encode".into())
        .spawn(move || {
            encode_loop(
                pcm_rx,
                stop_enc,
                app,
                room_id_u64,
                speaker_id_u32,
                write_tx,
                native_sample_rate,
                native_channels,
                rt_handle,
            );
        })
        .map_err(|e| format!("Failed to spawn encode thread: {e}"))?;

    Ok(CaptureHandle {
        stream,
        stop: stop_flag,
    })
}

/// Encoding loop: buffers samples → downmix → resample → Opus encode → WS binary.
fn encode_loop(
    mut pcm_rx: mpsc::UnboundedReceiver<Vec<f32>>,
    stop: Arc<AtomicBool>,
    app: AppHandle,
    room_id_u64: u64,
    speaker_id_u32: u32,
    write_tx: WsWriteTx,
    native_sample_rate: u32,
    native_channels: u16,
    rt: tokio::runtime::Handle,
) {
    // Create Opus encoder: 16 kHz, mono, VOIP application.
    let encoder = match audiopus::coder::Encoder::new(
        audiopus::SampleRate::Hz16000,
        audiopus::Channels::Mono,
        audiopus::Application::Voip,
    ) {
        Ok(mut enc) => {
            if let Err(e) = enc.set_bitrate(audiopus::Bitrate::BitsPerSecond(OPUS_BITRATE)) {
                tracing::warn!("Failed to set Opus bitrate: {e}");
            }
            enc
        }
        Err(e) => {
            tracing::error!("Failed to create Opus encoder: {e}");
            return;
        }
    };

    let sequence = AtomicU32::new(0);
    // Buffer in mono 16 kHz samples (after downmix + resample).
    let mut pcm_buffer: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES * 2);
    let mut encode_buf = [0u8; MAX_ENCODED_BYTES];
    let mut last_level_emit = Instant::now();

    let need_downmix = native_channels > 1;
    let need_resample = native_sample_rate != TARGET_SAMPLE_RATE;
    let resample_ratio = TARGET_SAMPLE_RATE as f64 / native_sample_rate as f64;

    // Fractional resample accumulator for linear interpolation.
    let mut resample_pos: f64 = 0.0;
    // Previous mono sample for interpolation across chunk boundaries.
    let mut prev_sample: f32 = 0.0;

    while !stop.load(Ordering::Relaxed) {
        // Block on receiving PCM data with a short timeout.
        match pcm_rx.try_recv() {
            Ok(samples) => {
                // Step 1: Downmix to mono if multi-channel.
                let mono: Vec<f32> = if need_downmix {
                    let ch = native_channels as usize;
                    samples
                        .chunks_exact(ch)
                        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                        .collect()
                } else {
                    samples
                };

                // Step 2: Resample from native rate to 16 kHz if needed.
                if need_resample {
                    // Linear interpolation resampler.
                    let input = &mono;
                    let in_len = input.len();
                    if in_len == 0 {
                        continue;
                    }
                    while resample_pos < in_len as f64 {
                        let idx = resample_pos as usize;
                        let frac = resample_pos - idx as f64;
                        let s0 = if idx == 0 && resample_pos < 1.0 {
                            prev_sample
                        } else if idx < in_len {
                            input[idx]
                        } else {
                            break;
                        };
                        let s1 = if idx + 1 < in_len {
                            input[idx + 1]
                        } else if idx < in_len {
                            input[idx]
                        } else {
                            break;
                        };
                        let interpolated = s0 + (s1 - s0) * frac as f32;
                        pcm_buffer.push(interpolated);
                        resample_pos += 1.0 / resample_ratio;
                    }
                    resample_pos -= in_len as f64;
                    if let Some(&last) = mono.last() {
                        prev_sample = last;
                    }
                } else {
                    pcm_buffer.extend_from_slice(&mono);
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            Err(mpsc::error::TryRecvError::Disconnected) => break,
        }

        // Process all complete frames in the buffer.
        while pcm_buffer.len() >= FRAME_SAMPLES {
            let frame_pcm: Vec<f32> = pcm_buffer.drain(..FRAME_SAMPLES).collect();

            // Compute RMS and emit audio_level event (throttled to ~15 Hz).
            let rms = compute_rms(&frame_pcm);
            if last_level_emit.elapsed() >= LEVEL_EMIT_INTERVAL {
                last_level_emit = Instant::now();
                let _ = app.emit(
                    "audio_level",
                    serde_json::json!({
                        "direction": "send",
                        "level": rms,
                    }),
                );
            }

            // Convert f32 [-1.0, 1.0] to i16 for Opus.
            let pcm_i16: Vec<i16> = frame_pcm
                .iter()
                .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .collect();

            // Opus encode.
            let encoded_len = match encoder.encode(&pcm_i16, &mut encode_buf) {
                Ok(len) => len,
                Err(e) => {
                    tracing::warn!("Opus encode error: {e}");
                    continue;
                }
            };

            let seq = sequence.fetch_add(1, Ordering::Relaxed);
            let frame = AudioFrame {
                room_id: room_id_u64,
                speaker_id: speaker_id_u32,
                sequence_num: seq,
                flags: 0,
                payload: encode_buf[..encoded_len].to_vec(),
            };

            let bytes = frame.encode();
            let tx = write_tx.clone();
            rt.spawn(async move {
                if let Err(e) = tx.send(WsMessage::Binary(bytes.into())).await {
                    tracing::debug!("Failed to send audio frame: {e}");
                }
            });
        }
    }

    // Send end-of-transmission frame.
    let seq = sequence.fetch_add(1, Ordering::Relaxed);
    let eot = AudioFrame {
        room_id: room_id_u64,
        speaker_id: speaker_id_u32,
        sequence_num: seq,
        flags: walkietalk_shared::audio::FLAG_END_OF_TRANSMISSION,
        payload: Vec::new(),
    };
    let bytes = eot.encode();
    let tx = write_tx.clone();
    rt.spawn(async move {
        let _ = tx.send(WsMessage::Binary(bytes.into())).await;
    });

    tracing::debug!("Audio capture encode loop ended");
}

/// Compute RMS (root mean square) of a PCM buffer, normalized to 0.0–1.0.
fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}
