//! Per-phone camera tracking worker (protocol v2).
//!
//! Runs on its own OS thread because the AprilTag detector is not `Send`, and so JPEG decoding and detection
//! never block the async runtime. The WebSocket task feeds it through a small bounded channel. When frames
//! arrive faster than they can be processed, only the newest is kept (no queue builds up). Gyro samples are
//! never dropped unless the channel is full.

use crate::session::SessionManager;
use std::collections::VecDeque;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wiiu_controller::protocol::{GyroSample, PointerXy, ServerMessage, TrackingState};
use wiiu_tracking::detect::{decode_jpeg_gray, TagDetector};
use wiiu_tracking::pipeline::{DeviceGyro, Tracker};
use wiiu_tracking::{BorderConfig, MarkerLayout, PointerState};

/// Without a camera fix for this long the pointer is gyro-only; after `LOST_AFTER` it is reported lost.
const GYRO_ONLY_AFTER: Duration = Duration::from_millis(300);
const LOST_AFTER: Duration = Duration::from_secs(2);
const STATUS_INTERVAL: Duration = Duration::from_millis(100);
const CHANNEL_CAPACITY: usize = 64;
/// Typical phone main-camera horizontal FOV; only scales gyro-predicted motion between fixes.
const DEFAULT_HFOV_DEG: f64 = 65.0;

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
}

impl Worker {
    fn new(sessions: Arc<SessionManager>, conn_id: u64, slot: usize, status: StatusSender) -> Self {
        let layout = MarkerLayout::border(&BorderConfig::default());
        let detector = match TagDetector::new() {
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
        if self.tracker.on_observations(capture_us, &observations).is_ok() {
            let now = Instant::now();
            self.last_fix = Some(now);
            self.fixes.push_back(now);
        }
    }

    fn state(&self, now: Instant) -> (TrackingState, Option<(f32, f32)>) {
        let age = self.last_fix.map(|t| now.saturating_duration_since(t));
        let pointer = match self.tracker.state() {
            PointerState::Lost => None,
            s => s.pointer().map(|(x, y)| (x as f32, y as f32)),
        };
        match age {
            None => (TrackingState::Lost, None),
            Some(a) if a >= LOST_AFTER => (TrackingState::Lost, None),
            Some(a) if a >= GYRO_ONLY_AFTER => (TrackingState::GyroOnly, pointer),
            Some(_) => (TrackingState::Tracking, pointer),
        }
    }

    fn publish(&mut self) {
        let now = Instant::now();
        while self
            .fixes
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) > Duration::from_secs(1))
        {
            self.fixes.pop_front();
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
        }
    }
}
