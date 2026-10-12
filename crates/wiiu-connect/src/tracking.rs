//! Per-phone camera tracking worker (protocol v2).
//!
//! Runs on its own OS thread because the AprilTag detector is not `Send`, and so JPEG decoding and detection
//! never block the async runtime. The WebSocket task feeds it through a small bounded channel. When frames
//! arrive faster than they can be processed, only the newest is kept (no queue builds up). Gyro samples are
//! never dropped unless the channel is full.

use crate::session::{SessionManager, TrackingDiag};
use std::collections::VecDeque;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wiiu_controller::protocol::{GyroSample, PointerXy, ServerMessage, TrackingState};
use wiiu_tracking::detect::{decode_jpeg_gray, TagDetector};
use wiiu_tracking::filter::{OneEuro2, OneEuroConfig};
use wiiu_tracking::pipeline::{DeviceGyro, Tracker};
use wiiu_tracking::{BorderConfig, MarkerLayout, PointerState};

/// Without a camera fix for this long the pointer is gyro-only; after `LOST_AFTER` it is reported lost.
const GYRO_ONLY_AFTER: Duration = Duration::from_millis(300);
const LOST_AFTER: Duration = Duration::from_secs(2);
const STATUS_INTERVAL: Duration = Duration::from_millis(100);
const CHANNEL_CAPACITY: usize = 64;
/// Typical phone main-camera FOV across the image's long side (16:9 video); only scales gyro-predicted motion,
/// and the tracker's online gyro check corrects it.
const DEFAULT_HFOV_DEG: f64 = 65.0;
/// Detection settings, chosen with `crates/wiiu-tracking/tests/realistic.rs`: at 540×960 with 1.5× decimation the
/// whole border is found from 0.8 to 2.5 m in ~17 ms on 4 threads (2.0× decimation lost every tag beyond ~1 m).
const QUAD_DECIMATE: f32 = 1.5;
const MAX_DETECTOR_THREADS: usize = 4;

/// Latest tracking status for the phone. A watch channel keeps only the newest value, so a phone that stops
/// reading can't make status messages pile up in memory.
pub type StatusSender = tokio::sync::watch::Sender<Option<ServerMessage>>;

pub enum Work {
    Frame {
        width: u16,
        height: u16,
        capture_us: u64,
        jpeg: Vec<u8>,
    },
    Gyro(Vec<GyroSample>),
}

/// Handle owned by the WebSocket task. Dropping it stops the worker thread.
pub struct TrackingHandle {
    tx: SyncSender<Work>,
}

impl TrackingHandle {
    /// Non-blocking; returns `false` if the work was dropped (worker busy and queue full).
    pub fn submit(&self, work: Work) -> bool {
        match self.tx.try_send(work) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

pub fn spawn(
    sessions: Arc<SessionManager>,
    conn_id: u64,
    slot: usize,
    status: StatusSender,
) -> std::io::Result<TrackingHandle> {
    let (tx, rx) = sync_channel(CHANNEL_CAPACITY);
    std::thread::Builder::new()
        .name(format!("tracking-slot{slot}"))
        .spawn(move || Worker::new(sessions, conn_id, slot, status).run(rx))?;
    Ok(TrackingHandle { tx })
}

struct Worker {
    sessions: Arc<SessionManager>,
    conn_id: u64,
    slot: usize,
    status: StatusSender,
    layout: MarkerLayout,
    detector: Option<TagDetector>,
    tracker: Tracker,
    last_fix: Option<Instant>,
    fixes: VecDeque<Instant>,
    last_status: Instant,
    smoothing: OneEuro2,
    epoch: Instant,
    frames: VecDeque<Instant>,
    gyro_samples: VecDeque<(Instant, usize)>,
    detect_ms: f32,
    frame_size: (u16, u16),
}

impl Worker {
    fn new(sessions: Arc<SessionManager>, conn_id: u64, slot: usize, status: StatusSender) -> Self {
        let layout = MarkerLayout::border(&BorderConfig::default());
        let threads = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).clamp(1, MAX_DETECTOR_THREADS))
            .unwrap_or(1);
        let detector = match TagDetector::with_options(QUAD_DECIMATE, threads as u8) {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::error!("slot {slot}: AprilTag detector unavailable: {e}");
                None
            }
        };
        Self {
            sessions,
            conn_id,
            slot,
            status,
            tracker: Tracker::new(layout.clone(), 640, 360, DEFAULT_HFOV_DEG),
            layout,
            detector,
            last_fix: None,
            fixes: VecDeque::new(),
            last_status: Instant::now(),
            smoothing: OneEuro2::new(OneEuroConfig::default()),
            epoch: Instant::now(),
            frames: VecDeque::new(),
            gyro_samples: VecDeque::new(),
            detect_ms: 0.0,
            frame_size: (0, 0),
        }
    }

    fn run(mut self, rx: Receiver<Work>) {
        loop {
            let first = match rx.recv_timeout(STATUS_INTERVAL) {
                Ok(w) => Some(w),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            // Drain everything queued: apply all gyro in order, keep only the newest frame.
            let mut gyro = Vec::new();
            let mut frame = None;
            for work in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                match work {
                    Work::Gyro(samples) => gyro.extend(samples),
                    f @ Work::Frame { .. } => frame = Some(f),
                }
            }
            if !gyro.is_empty() {
                self.gyro_samples.push_back((Instant::now(), gyro.len()));
                let samples: Vec<DeviceGyro> = gyro
                    .iter()
                    .map(|g| DeviceGyro {
                        t_us: g.t_us,
                        rate: g.rate,
                    })
                    .collect();
                self.tracker.on_gyro(&samples);
            }
            if let Some(Work::Frame {
                width,
                height,
                capture_us,
                jpeg,
            }) = frame
            {
                self.process_frame(width, height, capture_us, &jpeg);
            }
            self.publish();
        }
    }

    fn process_frame(&mut self, width: u16, height: u16, capture_us: u64, jpeg: &[u8]) {
        let Some(detector) = self.detector.as_mut() else {
            return;
        };
        let started = Instant::now();
        self.frames.push_back(started);
        self.frame_size = (width, height);
        if self.tracker.image_size() != (width, height) {
            self.tracker = Tracker::new(self.layout.clone(), width, height, DEFAULT_HFOV_DEG);
        }
        let gray = match decode_jpeg_gray(jpeg, (width, height)) {
            Ok(g) => g,
            Err(e) => {
                tracing::debug!("slot {}: dropping camera frame: {e}", self.slot);
                return;
            }
        };
        let observations = detector.detect(&gray, usize::from(width), usize::from(height));
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        self.detect_ms = if self.detect_ms == 0.0 {
            ms
        } else {
            0.9 * self.detect_ms + 0.1 * ms
        };
        if self.tracker.on_observations(capture_us, &observations).is_ok() {
            let now = Instant::now();
            self.last_fix = Some(now);
            self.fixes.push_back(now);
        }
    }

    fn state(&mut self, now: Instant) -> (TrackingState, Option<(f32, f32)>) {
        let age = self.last_fix.map(|t| now.saturating_duration_since(t));
        let raw = match self.tracker.state() {
            PointerState::Tracking { x, y } | PointerState::GyroOnly { x, y } => Some([x, y]),
            PointerState::Lost => None,
        };
        // Smooth before the on-screen check, so the filter doesn't restart at every screen edge crossing.
        let smoothed = match raw {
            Some(p) => Some(self.smoothing.filter(now.duration_since(self.epoch).as_secs_f64(), p)),
            None => {
                self.smoothing.reset();
                None
            }
        };
        let pointer = smoothed
            .filter(|[x, y]| (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y))
            .map(|[x, y]| (x as f32, y as f32));
        match age {
            None => (TrackingState::Lost, None),
            Some(a) if a >= LOST_AFTER => (TrackingState::Lost, None),
            Some(a) if a >= GYRO_ONLY_AFTER => (TrackingState::GyroOnly, pointer),
            Some(_) => (TrackingState::Tracking, pointer),
        }
    }

    fn publish(&mut self) {
        let now = Instant::now();
        let second_ago = |t: &Instant| now.saturating_duration_since(*t) > Duration::from_secs(1);
        while self.fixes.front().is_some_and(second_ago) {
            self.fixes.pop_front();
        }
        while self.frames.front().is_some_and(second_ago) {
            self.frames.pop_front();
        }
        while self.gyro_samples.front().is_some_and(|(t, _)| second_ago(t)) {
            self.gyro_samples.pop_front();
        }
        let (state, pointer) = self.state(now);
        // The session decides whether this pointer applies (only while camera frames keep arriving).
        self.sessions.set_tracked_pointer(self.conn_id, self.slot, pointer, now);
        if now.saturating_duration_since(self.last_status) >= STATUS_INTERVAL {
            self.last_status = now;
            self.status.send_replace(Some(ServerMessage::Tracking {
                state,
                pointer: pointer.map(|(x, y)| PointerXy { x, y }),
                tags: self.tracker.last_tag_count() as u32,
                fixes_per_s: self.fixes.len() as f32,
            }));
            let m = self.tracker.gyro_map();
            self.sessions.set_tracking_diag(
                self.conn_id,
                self.slot,
                TrackingDiag {
                    frame: [self.frame_size.0, self.frame_size.1],
                    frames_per_s: self.frames.len() as f32,
                    fixes_per_s: self.fixes.len() as f32,
                    tags: self.tracker.last_tag_count() as u32,
                    detect_ms: self.detect_ms,
                    gyro_per_s: self.gyro_samples.iter().map(|(_, n)| *n).sum::<usize>() as f32,
                    gyro_map: m.map(|row| row.map(|v| v as f32)),
                },
            );
        }
    }
}
