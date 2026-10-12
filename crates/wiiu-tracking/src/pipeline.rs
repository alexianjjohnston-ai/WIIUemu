//! Per-phone tracking pipeline: tag observations + gyro samples → pointer.
//!
//! Camera frames arrive late (capture → network → detection), while gyro samples stream continuously. On each
//! fix the tracker anchors the fusion at the frame's **capture time** and replays the buffered gyro samples taken
//! after it, so the pointer reflects the phone's current orientation, not the one at capture.

use crate::aim::{estimate_aim, AimConfig, AimError, AimEstimate, TagObservation};
use crate::fusion::{FusionConfig, PointerFusion, PointerState};
use crate::markers::MarkerLayout;
use std::collections::VecDeque;

/// A gyro sample in the W3C **device** frame (x right, y up, z out of the screen), rad/s, phone clock µs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceGyro {
    pub t_us: u64,
    pub rate: [f64; 3],
}

/// Rear camera in portrait with an unrotated image: camera x = device x, camera y = −device y,
/// camera z (forward, out of the back) = −device z.
pub fn device_to_camera(rate: [f64; 3]) -> [f64; 3] {
    [rate[0], -rate[1], -rate[2]]
}

/// Gyro history kept for replay; covers the worst expected capture → fix delay.
const GYRO_HISTORY_US: u64 = 1_000_000;
/// Hard cap on buffered samples (≈ 1 s at 1 kHz), so tightly spaced timestamps can't grow the history or the
/// replay cost without bound.
const GYRO_HISTORY_MAX: usize = 1024;
/// Largest gap integrated as one step; longer gaps mean missing data, not motion.
const MAX_GYRO_STEP_US: u64 = 100_000;

#[derive(Debug, Clone)]
pub struct Tracker {
    layout: MarkerLayout,
    width: u16,
    height: u16,
    aim: AimConfig,
    fusion: PointerFusion,
    gyro: VecDeque<DeviceGyro>,
    /// Phone time up to which gyro has been integrated into `fusion`.
    integrated_to_us: Option<u64>,
    last_fix_capture_us: Option<u64>,
    last_tags: usize,
}

impl Tracker {
    /// `hfov_deg`: camera horizontal field of view; only scales gyro-predicted motion (65° is typical).
    pub fn new(layout: MarkerLayout, width: u16, height: u16, hfov_deg: f64) -> Self {
        let (w, h) = (f64::from(width), f64::from(height));
        let mut fusion_cfg = FusionConfig::for_camera(w, h, hfov_deg);
        // Staleness is decided by the caller with a wall clock; the fusion only propagates.
        fusion_cfg.max_gyro_only_s = f64::INFINITY;
        Self {
            layout,
            width,
            height,
            aim: AimConfig::centred(w, h),
            fusion: PointerFusion::new(fusion_cfg),
            gyro: VecDeque::new(),
            integrated_to_us: None,
            last_fix_capture_us: None,
            last_tags: 0,
        }
    }

    pub fn image_size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Image point that represents the pointing direction (from per-device calibration).
    /// Resets tracking; the next camera fix re-anchors.
    pub fn set_aim_image_point(&mut self, p: [f64; 2]) {
        self.aim.aim_image_point = p;
        let mut cfg = *self.fusion.config();
        cfg.aim_image_point = p;
        self.fusion = PointerFusion::new(cfg);
        self.integrated_to_us = None;
        self.last_fix_capture_us = None;
    }

    fn integrate(&mut self, from_us: u64, s: &DeviceGyro) {
        let dt_us = s.t_us.saturating_sub(from_us);
        if dt_us > 0 && dt_us <= MAX_GYRO_STEP_US {
            self.fusion.gyro(device_to_camera(s.rate), dt_us as f64 / 1e6);
        }
    }

    /// Adds gyro samples (strictly increasing `t_us`) and propagates the pointer.
    pub fn on_gyro(&mut self, samples: &[DeviceGyro]) {
        for s in samples {
            if self.gyro.back().is_some_and(|b| s.t_us <= b.t_us) {
                continue;
            }
            if let Some(from) = self.integrated_to_us {
                if s.t_us > from {
                    self.integrate(from, s);
                    self.integrated_to_us = Some(s.t_us);
                }
            }
            self.gyro.push_back(*s);
        }
        if let Some(newest) = self.gyro.back().map(|s| s.t_us) {
            while self.gyro.len() > GYRO_HISTORY_MAX
                || self.gyro.front().is_some_and(|s| s.t_us + GYRO_HISTORY_US < newest)
            {
                self.gyro.pop_front();
            }
        }
    }

    /// Applies tag observations from a frame captured at `capture_us` (phone clock).
    /// Frames older than the last fix are ignored (returns the error the estimate would have).
    pub fn on_observations(
        &mut self,
        capture_us: u64,
        observations: &[TagObservation],
    ) -> Result<AimEstimate, AimError> {
        self.last_tags = observations.len();
        let est = estimate_aim(observations, &self.layout, &self.aim)?;
        if self.last_fix_capture_us.is_some_and(|t| capture_us <= t) {
            return Ok(est); // out-of-order frame: don't rewind the fusion
        }
        self.last_fix_capture_us = Some(capture_us);
        self.fusion.camera_fix(&est);
        // Replay gyro recorded after the capture.
        let mut from = capture_us;
        let replay: Vec<DeviceGyro> = self.gyro.iter().filter(|s| s.t_us > capture_us).copied().collect();
        for s in &replay {
            self.integrate(from, s);
            from = s.t_us;
        }
        self.integrated_to_us = Some(from);
        Ok(est)
    }

    /// Current propagated pointer state (staleness is the caller's job).
    pub fn state(&self) -> PointerState {
        self.fusion.state()
    }

    pub fn last_tag_count(&self) -> usize {
        self.last_tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markers::BorderConfig;

    #[test]
    fn device_to_camera_axes() {
        assert_eq!(device_to_camera([1.0, 2.0, 3.0]), [1.0, -2.0, -3.0]);
    }

    #[test]
    fn lost_until_first_fix_and_rejects_without_tags() {
        let mut t = Tracker::new(MarkerLayout::border(&BorderConfig::default()), 640, 360, 65.0);
        assert_eq!(t.state(), PointerState::Lost);
        t.on_gyro(&[DeviceGyro {
            t_us: 10,
            rate: [0.1, 0.0, 0.0],
        }]);
        assert_eq!(t.state(), PointerState::Lost);
        assert!(t.on_observations(20, &[]).is_err());
        assert_eq!(t.state(), PointerState::Lost);
    }

    #[test]
    fn gyro_history_is_bounded() {
        let mut t = Tracker::new(MarkerLayout::border(&BorderConfig::default()), 640, 360, 65.0);
        let samples: Vec<DeviceGyro> = (0..1000)
            .map(|i| DeviceGyro {
                t_us: i * 5_000,
                rate: [0.0; 3],
            })
            .collect();
        t.on_gyro(&samples);
        assert!(t.gyro.len() <= 202, "{}", t.gyro.len());
    }

    #[test]
    fn gyro_history_is_bounded_by_count_for_dense_timestamps() {
        let mut t = Tracker::new(MarkerLayout::border(&BorderConfig::default()), 640, 360, 65.0);
        let samples: Vec<DeviceGyro> = (1..=10_000)
            .map(|i| DeviceGyro {
                t_us: i,
                rate: [0.0; 3],
            })
            .collect();
        t.on_gyro(&samples);
        assert_eq!(t.gyro.len(), GYRO_HISTORY_MAX);
    }
}
