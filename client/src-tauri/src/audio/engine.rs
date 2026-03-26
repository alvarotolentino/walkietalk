use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::transport::ws::WsWriteTx;
use walkietalk_shared::audio::AudioFrame;

// ── Constants ────────────────────────────────────────────────────────────────

const FRAME_SAMPLES: usize = 320;
const TARGET_SAMPLE_RATE: u32 = 16_000;
const OPUS_BITRATE: i32 = 16_000;
const MAX_ENCODED_BYTES: usize = 256;
const LEVEL_EMIT_INTERVAL: Duration = Duration::from_millis(67); // ~15 Hz
const JITTER_BUFFER_FRAMES: usize = 3;

// ── SendStream wrapper ──────────────────────────────────────────────────────

struct SendStream(cpal::Stream);

// SAFETY: We only target desktop (WASAPI/CoreAudio/ALSA) where
// cpal::Stream is internally thread-safe.
unsafe impl Send for SendStream {}

// ── JitterBuffer ────────────────────────────────────────────────────────────

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
        if let Some(last) = self.last_seq {
            if seq <= last {
                return;
            }
        }
        self.last_seq = Some(seq);
        self.frames.push_back(pcm);
        while self.frames.len() > JITTER_BUFFER_FRAMES + 2 {
            self.frames.pop_front();
        }
    }

    fn pop(&mut self) -> Option<Vec<f32>> {
        self.frames.pop_front()
    }

    fn reset(&mut self) {
        self.frames.clear();
        self.last_seq = None;
    }
}

// ── AudioReceiver ───────────────────────────────────────────────────────────

/// Thread-safe audio frame receiver shared between the transport read loop
/// (which pushes incoming frames) and the CPAL output callback (which pops
/// decoded PCM). All locking uses `std::sync::Mutex` (microsecond-level),
/// completely avoiding the tokio Mutex contention that caused frame drops.
pub struct AudioReceiver {
    decoder: StdMutex<audiopus::coder::Decoder>,
    buffer: StdMutex<JitterBuffer>,
    active: AtomicBool,
}

impl AudioReceiver {
    pub fn new() -> Self {
        let decoder =
            audiopus::coder::Decoder::new(audiopus::SampleRate::Hz16000, audiopus::Channels::Mono)
                .expect("Failed to create Opus decoder");

        Self {
            decoder: StdMutex::new(decoder),
            buffer: StdMutex::new(JitterBuffer::new()),
            active: AtomicBool::new(false),
        }
    }

    /// Decode a raw AudioFrame binary and push decoded PCM into the jitter
    /// buffer. Called from the transport read loop — uses only std::sync::Mutex
    /// (never blocks on WASAPI init).
    pub fn push_frame(&self, data: &[u8]) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }

        let frame = match AudioFrame::decode(data) {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut dec = match self.decoder.lock() {
            Ok(d) => d,
            Err(_) => return,
        };

        let mut pcm = vec![0i16; FRAME_SAMPLES];
        let decoded = if frame.payload.is_empty() {
            // PLC (Packet Loss Concealment)
            let output: audiopus::MutSignals<'_, i16> = match (&mut pcm).try_into() {
                Ok(o) => o,
                Err(_) => return,
            };
            match dec.decode(None, output, false) {
                Ok(n) => n,
                Err(_) => return,
            }
        } else {
            let packet: audiopus::packet::Packet<'_> = match (&frame.payload[..]).try_into() {
                Ok(p) => p,
                Err(_) => return,
            };
            let output: audiopus::MutSignals<'_, i16> = match (&mut pcm).try_into() {
                Ok(o) => o,
                Err(_) => return,
            };
            match dec.decode(Some(packet), output, false) {
                Ok(n) => n,
                Err(_) => return,
            }
        };

        // Release decoder lock before locking buffer.
        drop(dec);

        if decoded > 0 {
            let pcm_f32: Vec<f32> = pcm[..decoded]
                .iter()
                .map(|&s| s as f32 / i16::MAX as f32)
                .collect();
            if let Ok(mut buf) = self.buffer.lock() {
                buf.push(frame.sequence_num, pcm_f32);
            }
        }
    }

    /// Activate: reset decoder + clear buffer + start accepting frames.
    fn activate(&self) {
        if let Ok(mut dec) = self.decoder.lock() {
            if let Ok(new_dec) = audiopus::coder::Decoder::new(
                audiopus::SampleRate::Hz16000,
                audiopus::Channels::Mono,
            ) {
                *dec = new_dec;
            }
        }
        if let Ok(mut buf) = self.buffer.lock() {
            buf.reset();
        }
        self.active.store(true, Ordering::SeqCst);
    }

    /// Deactivate: stop accepting frames + clear buffer.
    fn deactivate(&self) {
        self.active.store(false, Ordering::SeqCst);
        if let Ok(mut buf) = self.buffer.lock() {
            buf.reset();
        }
    }
}

// ── AudioEngine ─────────────────────────────────────────────────────────────

/// Owns both CPAL input + output streams for the entire room session.
///
/// Streams are created once on room join and dropped once on room leave.
/// `activate_capture` / `deactivate_capture` only pause/unpause — no
/// WASAPI session churn. The `AudioReceiver` (shared with `AppState`) handles
/// all decode + buffering via `std::sync::Mutex`, eliminating the tokio Mutex
/// race condition that dropped audio frames.
pub struct AudioEngine {
    // Input side
    input_stream: SendStream,
    /// PCM receiver slot: taken by encode thread, returned when it exits.
    pcm_rx: Arc<StdMutex<Option<mpsc::UnboundedReceiver<Vec<f32>>>>>,
    encode_thread: Option<JoinHandle<()>>,
    encode_stop: Arc<AtomicBool>,

    // Output side
    output_stream: SendStream,
    /// Shared receiver (also stored in AppState for the transport read loop).
    receiver: Arc<AudioReceiver>,

    // Config
    native_input_rate: u32,
    native_input_channels: u16,
    app: AppHandle,
}

impl AudioEngine {
    /// Create a new AudioEngine — opens both CPAL input and output streams
    /// (both start **paused**).
    pub fn new(app: AppHandle, receiver: Arc<AudioReceiver>) -> Result<Self, String> {
        let host = cpal::default_host();

        // ── Input device ─────────────────────────────────────────────────
        let in_device = host
            .default_input_device()
            .ok_or("No audio input device found")?;
        let in_cfg = in_device
            .default_input_config()
            .map_err(|e| format!("Input config: {e}"))?;
        let native_input_rate = in_cfg.sample_rate().0;
        let native_input_channels = in_cfg.channels();
        tracing::info!(
            "AudioEngine input: {:?} {}Hz {}ch",
            in_device.name().unwrap_or_default(),
            native_input_rate,
            native_input_channels,
        );

        let in_config = cpal::StreamConfig {
            channels: native_input_channels,
            sample_rate: cpal::SampleRate(native_input_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let (pcm_tx, pcm_rx) = mpsc::unbounded_channel::<Vec<f32>>();

        let input_stream = in_device
            .build_input_stream(
                &in_config,
                move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                    let _ = pcm_tx.send(data.to_vec());
                },
                |err| tracing::error!("CPAL input error: {err}"),
                None,
            )
            .map_err(|e| format!("Build input stream: {e}"))?;

        input_stream.pause().ok();

        // ── Output device ────────────────────────────────────────────────
        let out_device = host
            .default_output_device()
            .ok_or("No audio output device found")?;
        let out_cfg = out_device
            .default_output_config()
            .map_err(|e| format!("Output config: {e}"))?;
        let native_output_rate = out_cfg.sample_rate().0;
        let native_output_channels = out_cfg.channels();
        tracing::info!(
            "AudioEngine output: {:?} {}Hz {}ch",
            out_device.name().unwrap_or_default(),
            native_output_rate,
            native_output_channels,
        );

        let out_config = cpal::StreamConfig {
            channels: native_output_channels,
            sample_rate: cpal::SampleRate(native_output_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let buf_reader = receiver.clone();
        let app_out = app.clone();
        let need_resample = native_output_rate != TARGET_SAMPLE_RATE;
        let upsample_ratio = native_output_rate as f64 / TARGET_SAMPLE_RATE as f64;
        let ch = native_output_channels as usize;

        let mut resample_buf: Vec<f32> = Vec::new();
        let mut resample_pos: f64 = 0.0;
        let mut residual: Vec<f32> = Vec::new();
        let mut last_level_emit = Instant::now();

        let output_stream = out_device
            .build_output_stream(
                &out_config,
                move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    let output_mono_samples = data.len() / ch;
                    let mut mono_out: Vec<f32> = Vec::with_capacity(output_mono_samples);

                    if !residual.is_empty() {
                        let take = output_mono_samples.min(residual.len());
                        mono_out.extend_from_slice(&residual[..take]);
                        residual.drain(..take);
                    }

                    while mono_out.len() < output_mono_samples {
                        let frame = buf_reader
                            .buffer
                            .lock()
                            .unwrap()
                            .pop()
                            .unwrap_or_else(|| vec![0.0; FRAME_SAMPLES]);

                        if last_level_emit.elapsed() >= LEVEL_EMIT_INTERVAL {
                            last_level_emit = Instant::now();
                            let rms = compute_rms(&frame);
                            let _ = app_out.emit(
                                "audio_level",
                                serde_json::json!({
                                    "direction": "recv",
                                    "level": rms,
                                }),
                            );
                        }

                        if need_resample {
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

                    if mono_out.len() > output_mono_samples {
                        residual.extend_from_slice(&mono_out[output_mono_samples..]);
                        mono_out.truncate(output_mono_samples);
                    }

                    for (i, sample) in mono_out.iter().enumerate() {
                        for c in 0..ch {
                            let idx = i * ch + c;
                            if idx < data.len() {
                                data[idx] = *sample;
                            }
                        }
                    }
                },
                |err| tracing::error!("CPAL output error: {err}"),
                None,
            )
            .map_err(|e| format!("Build output stream: {e}"))?;

        output_stream.pause().ok();

        tracing::info!("AudioEngine created (both streams paused)");

        Ok(Self {
            input_stream: SendStream(input_stream),
            output_stream: SendStream(output_stream),
            pcm_rx: Arc::new(StdMutex::new(Some(pcm_rx))),
            encode_thread: None,
            encode_stop: Arc::new(AtomicBool::new(true)),
            receiver,
            native_input_rate,
            native_input_channels,
            app,
        })
    }

    // ── Capture ──────────────────────────────────────────────────────────

    /// Unpause the input stream and start encoding → WebSocket.
    pub fn activate_capture(
        &mut self,
        room_id_u64: u64,
        speaker_id_u32: u32,
        write_tx: WsWriteTx,
    ) -> Result<(), String> {
        self.deactivate_capture();

        let pcm_rx = self
            .pcm_rx
            .lock()
            .unwrap()
            .take()
            .ok_or("PCM receiver unavailable")?;

        self.encode_stop.store(false, Ordering::SeqCst);

        let stop = self.encode_stop.clone();
        let pcm_rx_slot = self.pcm_rx.clone();
        let app = self.app.clone();
        let native_sr = self.native_input_rate;
        let native_ch = self.native_input_channels;
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|_| "No tokio runtime for audio capture")?;

        let handle = std::thread::Builder::new()
            .name("audio-encode".into())
            .spawn(move || {
                let mut pcm_rx = pcm_rx;
                // Drain stale PCM from when the stream was paused.
                while pcm_rx.try_recv().is_ok() {}

                encode_loop(
                    &mut pcm_rx,
                    &stop,
                    &app,
                    room_id_u64,
                    speaker_id_u32,
                    &write_tx,
                    native_sr,
                    native_ch,
                    &rt,
                );

                // Return receiver to slot for next capture session.
                if let Ok(mut slot) = pcm_rx_slot.lock() {
                    *slot = Some(pcm_rx);
                }
            })
            .map_err(|e| format!("Spawn encode thread: {e}"))?;

        self.encode_thread = Some(handle);

        self.input_stream
            .0
            .play()
            .map_err(|e| format!("Unpause input: {e}"))?;

        tracing::debug!("Capture activated");
        Ok(())
    }

    /// Pause the input stream and join the encode thread.
    pub fn deactivate_capture(&mut self) {
        if !self.encode_stop.swap(true, Ordering::SeqCst) {
            // Was active (false → true).
            self.input_stream.0.pause().ok();
            if let Some(handle) = self.encode_thread.take() {
                let _ = handle.join();
            }
            tracing::debug!("Capture deactivated");
        }
    }

    // ── Playback ─────────────────────────────────────────────────────────

    /// Activate playback: reset decoder + jitter buffer, unpause output.
    pub fn activate_playback(&self) -> Result<(), String> {
        self.receiver.activate();
        self.output_stream
            .0
            .play()
            .map_err(|e| format!("Unpause output: {e}"))?;
        tracing::debug!("Playback activated");
        Ok(())
    }

    /// Deactivate playback: pause output, clear buffer.
    pub fn deactivate_playback(&self) {
        self.receiver.deactivate();
        self.output_stream.0.pause().ok();
        tracing::debug!("Playback deactivated");
    }

    /// Shut down the engine: deactivate everything, drop streams cleanly.
    pub fn shutdown(mut self) {
        tracing::info!("AudioEngine shutting down");
        self.deactivate_capture();
        self.deactivate_playback();
        // Give WASAPI time to release endpoints cleanly.
        std::thread::sleep(Duration::from_millis(50));
        tracing::info!("AudioEngine shutdown complete");
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.encode_stop.store(true, Ordering::SeqCst);
        self.input_stream.0.pause().ok();
        self.output_stream.0.pause().ok();
        if let Some(handle) = self.encode_thread.take() {
            let _ = handle.join();
        }
    }
}

// ── Encode loop ─────────────────────────────────────────────────────────────

fn encode_loop(
    pcm_rx: &mut mpsc::UnboundedReceiver<Vec<f32>>,
    stop: &AtomicBool,
    app: &AppHandle,
    room_id_u64: u64,
    speaker_id_u32: u32,
    write_tx: &WsWriteTx,
    native_sample_rate: u32,
    native_channels: u16,
    rt: &tokio::runtime::Handle,
) {
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
    let mut pcm_buffer: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES * 2);
    let mut encode_buf = [0u8; MAX_ENCODED_BYTES];
    let mut last_level_emit = Instant::now();

    let need_downmix = native_channels > 1;
    let need_resample = native_sample_rate != TARGET_SAMPLE_RATE;
    let resample_ratio = TARGET_SAMPLE_RATE as f64 / native_sample_rate as f64;

    let mut resample_pos: f64 = 0.0;
    let mut prev_sample: f32 = 0.0;

    while !stop.load(Ordering::Relaxed) {
        match pcm_rx.try_recv() {
            Ok(samples) => {
                let mono: Vec<f32> = if need_downmix {
                    let ch = native_channels as usize;
                    samples
                        .chunks_exact(ch)
                        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                        .collect()
                } else {
                    samples
                };

                if need_resample {
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

        while pcm_buffer.len() >= FRAME_SAMPLES {
            let frame_pcm: Vec<f32> = pcm_buffer.drain(..FRAME_SAMPLES).collect();

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

            let pcm_i16: Vec<i16> = frame_pcm
                .iter()
                .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .collect();

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

fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}
