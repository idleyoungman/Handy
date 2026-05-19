use std::{
    io::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SizedSample,
};

use crate::audio_toolkit::{
    audio::{AudioVisualiser, FrameResampler},
    constants,
    vad::{self, VadFrame},
    VoiceActivityDetector,
};

enum Cmd {
    Start,
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

enum AudioChunk {
    Samples(Vec<f32>),
    EndOfStream,
}

pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    vad: Option<Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
}

impl AudioRecorder {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AudioRecorder {
            device: None,
            cmd_tx: None,
            worker_handle: None,
            vad: None,
            level_cb: None,
        })
    }

    pub fn with_vad(mut self, vad: Box<dyn VoiceActivityDetector>) -> Self {
        self.vad = Some(Arc::new(Mutex::new(vad)));
        self
    }

    pub fn with_level_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(Vec<f32>) + Send + Sync + 'static,
    {
        self.level_cb = Some(Arc::new(cb));
        self
    }

    pub fn open(&mut self, device: Option<Device>) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            return Ok(());
        }

        let (sample_tx, sample_rx) = mpsc::channel::<AudioChunk>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let host = crate::audio_toolkit::get_cpal_host();
        let device = match device {
            Some(dev) => dev,
            None => host
                .default_input_device()
                .ok_or_else(|| Error::new(std::io::ErrorKind::NotFound, "No input device found"))?,
        };

        let thread_device = device.clone();
        let vad = self.vad.clone();
        let level_cb = self.level_cb.clone();

        let worker = std::thread::spawn(move || {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_for_stream = stop_flag.clone();
            let init_result = (|| -> Result<(cpal::Stream, u32), String> {
                let config = AudioRecorder::get_preferred_config(&thread_device)
                    .map_err(|e| format!("Failed to fetch preferred config: {e}"))?;

                let sample_rate = config.sample_rate().0;
                let channels = config.channels() as usize;

                tracing::info!(
                    "Using device: {:?}, sample_rate={}, channels={}, format={:?}",
                    thread_device.name(),
                    sample_rate,
                    channels,
                    config.sample_format()
                );

                let stream = match config.sample_format() {
                    cpal::SampleFormat::U8 => AudioRecorder::build_stream::<u8>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        stop_flag_for_stream,
                    ),
                    cpal::SampleFormat::I8 => AudioRecorder::build_stream::<i8>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        stop_flag_for_stream,
                    ),
                    cpal::SampleFormat::I16 => AudioRecorder::build_stream::<i16>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        stop_flag_for_stream,
                    ),
                    cpal::SampleFormat::I32 => AudioRecorder::build_stream::<i32>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        stop_flag_for_stream,
                    ),
                    cpal::SampleFormat::F32 => AudioRecorder::build_stream::<f32>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        stop_flag_for_stream,
                    ),
                    fmt => return Err(format!("Unsupported sample format: {fmt:?}")),
                }
                .map_err(|e| format!("Failed to build input stream: {e}"))?;

                stream
                    .play()
                    .map_err(|e| format!("Failed to start microphone stream: {e}"))?;

                Ok((stream, sample_rate))
            })();

            match init_result {
                Ok((stream, sample_rate)) => {
                    let _ = init_tx.send(Ok(()));
                    run_consumer(sample_rate, vad, sample_rx, cmd_rx, level_cb, stop_flag);
                    drop(stream);
                }
                Err(msg) => {
                    tracing::error!("{msg}");
                    let _ = init_tx.send(Err(msg));
                }
            }
        });

        match init_rx.recv() {
            Ok(Ok(())) => {
                self.device = Some(device);
                self.cmd_tx = Some(cmd_tx);
                self.worker_handle = Some(worker);
                Ok(())
            }
            Ok(Err(msg)) => {
                let _ = worker.join();
                let kind = if is_microphone_access_denied(&msg) {
                    std::io::ErrorKind::PermissionDenied
                } else {
                    std::io::ErrorKind::Other
                };
                Err(Box::new(Error::new(kind, msg)))
            }
            Err(e) => {
                let _ = worker.join();
                Err(Box::new(std::io::Error::other(format!(
                    "Failed to initialize microphone worker: {e}"
                ))))
            }
        }
    }

    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Start)?;
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let (resp_tx, resp_rx) = mpsc::channel();
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Stop(resp_tx))?;
        }
        Ok(resp_rx.recv()?)
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        self.device = None;
        Ok(())
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        sample_tx: mpsc::Sender<AudioChunk>,
        channels: usize,
        stop_flag: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let mut output_buffer = Vec::new();
        let mut eos_sent = false;

        device.build_input_stream(
            &config.clone().into(),
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if stop_flag.load(Ordering::Relaxed) {
                    if !eos_sent {
                        let _ = sample_tx.send(AudioChunk::EndOfStream);
                        eos_sent = true;
                    }
                    return;
                }
                eos_sent = false;

                output_buffer.clear();
                if channels == 1 {
                    output_buffer.extend(data.iter().map(|&s| s.to_sample::<f32>()));
                } else {
                    let frame_count = data.len() / channels;
                    output_buffer.reserve(frame_count);
                    for frame in data.chunks_exact(channels) {
                        let mono = frame.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>()
                            / channels as f32;
                        output_buffer.push(mono);
                    }
                }

                if sample_tx
                    .send(AudioChunk::Samples(output_buffer.clone()))
                    .is_err()
                {
                    tracing::error!("Failed to send audio samples");
                }
            },
            |err| tracing::error!("Stream error: {}", err),
            None,
        )
    }

    fn get_preferred_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        let default_config = device.default_input_config()?;
        let target_rate = default_config.sample_rate();

        let supported_configs = match device.supported_input_configs() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Could not enumerate input configs ({e}), using device default");
                return Ok(default_config);
            }
        };

        let mut best_config: Option<cpal::SupportedStreamConfigRange> = None;
        for cr in supported_configs {
            if cr.min_sample_rate() <= target_rate && cr.max_sample_rate() >= target_rate {
                let score = |fmt: cpal::SampleFormat| match fmt {
                    cpal::SampleFormat::F32 => 4,
                    cpal::SampleFormat::I16 => 3,
                    cpal::SampleFormat::I32 => 2,
                    _ => 1,
                };
                match &best_config {
                    None => best_config = Some(cr),
                    Some(cur) if score(cr.sample_format()) > score(cur.sample_format()) => {
                        best_config = Some(cr);
                    }
                    _ => {}
                }
            }
        }

        if let Some(c) = best_config {
            return Ok(c.with_sample_rate(target_rate));
        }

        tracing::warn!(
            "No supported config matched device default rate {:?}, using default",
            target_rate
        );
        Ok(default_config)
    }
}

pub fn is_microphone_access_denied(msg: &str) -> bool {
    let n = msg.to_lowercase();
    n.contains("access is denied") || n.contains("permission denied") || n.contains("0x80070005")
}

fn run_consumer(
    in_sample_rate: u32,
    vad: Option<Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>>,
    sample_rx: mpsc::Receiver<AudioChunk>,
    cmd_rx: mpsc::Receiver<Cmd>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    stop_flag: Arc<AtomicBool>,
) {
    let mut frame_resampler = FrameResampler::new(
        in_sample_rate as usize,
        constants::WHISPER_SAMPLE_RATE as usize,
        Duration::from_millis(30),
    );

    let mut processed_samples = Vec::<f32>::new();
    let mut recording = false;

    const BUCKETS: usize = 16;
    const WINDOW_SIZE: usize = 512;
    let mut visualizer = AudioVisualiser::new(in_sample_rate, WINDOW_SIZE, BUCKETS, 400.0, 4000.0);

    fn handle_frame(
        samples: &[f32],
        recording: bool,
        vad: &Option<Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>>,
        out_buf: &mut Vec<f32>,
    ) {
        if !recording {
            return;
        }
        if let Some(vad_arc) = vad {
            let mut det = vad_arc.lock().unwrap();
            match det.push_frame(samples).unwrap_or(VadFrame::Speech(samples)) {
                VadFrame::Speech(buf) => out_buf.extend_from_slice(buf),
                VadFrame::Noise => {}
            }
        } else {
            out_buf.extend_from_slice(samples);
        }
    }

    while let Ok(chunk) = sample_rx.recv() {
        let raw = match chunk {
            AudioChunk::Samples(s) => s,
            AudioChunk::EndOfStream => continue,
        };

        if let Some(buckets) = visualizer.feed(&raw) {
            if let Some(cb) = &level_cb {
                cb(buckets);
            }
        }

        frame_resampler.push(&raw, &mut |frame: &[f32]| {
            handle_frame(frame, recording, &vad, &mut processed_samples)
        });

        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start => {
                    stop_flag.store(false, Ordering::Relaxed);
                    processed_samples.clear();
                    recording = true;
                    visualizer.reset();
                    if let Some(v) = &vad {
                        v.lock().unwrap().reset();
                    }
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    stop_flag.store(true, Ordering::Relaxed);

                    loop {
                        match sample_rx.recv_timeout(Duration::from_secs(2)) {
                            Ok(AudioChunk::Samples(remaining)) => {
                                frame_resampler.push(&remaining, &mut |frame: &[f32]| {
                                    handle_frame(frame, true, &vad, &mut processed_samples)
                                });
                            }
                            Ok(AudioChunk::EndOfStream) => break,
                            Err(_) => {
                                tracing::warn!(
                                    "Timed out waiting for EndOfStream from audio callback"
                                );
                                break;
                            }
                        }
                    }

                    frame_resampler.finish(&mut |frame: &[f32]| {
                        handle_frame(frame, true, &vad, &mut processed_samples)
                    });

                    let _ = reply_tx.send(std::mem::take(&mut processed_samples));
                    stop_flag.store(false, Ordering::Relaxed);
                }
                Cmd::Shutdown => {
                    stop_flag.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
    }
}
