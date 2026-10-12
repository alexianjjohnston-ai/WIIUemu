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

/// How the gyro-predicted motion maps onto camera-measured motion, learned online (see [`GyroCheck`]).
/// `map · gyro_delta ≈ camera_delta` for pointer deltas `[dx, dy]` between consecutive fixes.
pub type GyroMap = [[f64; 2]; 2];

const IDENTITY_MAP: GyroMap = [[1.0, 0.0], [0.0, 1.0]];
/// Consecutive fixes further apart than this aren't used to check the gyro (too much can change).
const CHECK_MAX_GAP_US: u64 = 250_000;
/// Forgetting factor per fix pair, so the check adapts if the phone or its sensors behave differently.
const CHECK_FORGET: f64 = 0.97;
/// Minimum accumulated gyro-predicted motion (squared, screen units) before the map is updated.
const CHECK_MIN_MOTION: f64 = 2e-3;

/// Online least-squares check of gyro predictions against camera fixes.
///
/// Phones differ in sensor axes, units and timing, and none of that can be verified without each phone. Instead of
/// trusting the theory, each pair of consecutive fixes gives one sample: what the gyro alone predicted (starting
/// from the first fix) versus what the camera then measured. A 2×2 map fitted to these samples corrects swapped
/// axes, flipped signs and wrong scales. If the gyro explains the camera motion poorly, the map becomes zero and
/// the pointer simply holds between camera fixes.
#[derive(Debug, Clone)]
struct GyroCheck {
    /// Σ camera_delta · gyro_deltaᵀ, Σ gyro_delta · gyro_deltaᵀ, Σ |camera_delta|².
    scg: [[f64; 2]; 2],
    sgg: [[f64; 2]; 2],
    scc: f64,
    pairs: f64,
    map: GyroMap,
}

impl GyroCheck {
    fn new() -> Self {
        Self {
            scg: [[0.0; 2]; 2],
            sgg: [[0.0; 2]; 2],
            scc: 0.0,
            pairs: 0.0,
            map: IDENTITY_MAP,
        }
    }

    fn add(&mut self, cam: [f64; 2], gyro: [f64; 2]) {
        for i in 0..2 {
            for j in 0..2 {
                self.scg[i][j] = CHECK_FORGET * self.scg[i][j] + cam[i] * gyro[j];
                self.sgg[i][j] = CHECK_FORGET * self.sgg[i][j] + gyro[i] * gyro[j];
            }
        }
        self.scc = CHECK_FORGET * self.scc + cam[0] * cam[0] + cam[1] * cam[1];
        self.pairs = CHECK_FORGET * self.pairs + 1.0;
        self.update();
    }

    fn update(&mut self) {
        let g = self.sgg;
        let trace = g[0][0] + g[1][1];
        if self.pairs < 8.0 || trace < CHECK_MIN_MOTION {
            return; // not enough evidence yet: keep the current map
        }
        // Least squares with a weak prior towards the identity: an axis that hasn't moved yet keeps the
        // theoretical mapping instead of collapsing to zero.
        let prior = 0.5 * CHECK_MIN_MOTION;
        let (a, b, c, d) = (g[0][0] + prior, g[0][1], g[1][0], g[1][1] + prior);
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return;
        }
        let inv = [[d / det, -b / det], [-c / det, a / det]];
        let scg = [
            [self.scg[0][0] + prior, self.scg[0][1]],
            [self.scg[1][0], self.scg[1][1] + prior],
        ];
        let m: GyroMap =
            std::array::from_fn(|i| std::array::from_fn(|j| scg[i][0] * inv[0][j] + scg[i][1] * inv[1][j]));
        // Fraction of camera motion the map explains: Σ|c|² − residual = tr(M Σgc).
        let explained: f64 = (0..2)
            .map(|i| (0..2).map(|j| m[i][j] * self.scg[i][j]).sum::<f64>())
            .sum();
        let r2 = if self.scc > 0.0 { explained / self.scc } else { 0.0 };
        self.map = if r2 < 0.3 {
            [[0.0; 2]; 2] // the gyro doesn't predict what the camera sees: don't let it move the pointer
        } else {
            m.map(|row| row.map(|v| v.clamp(-100.0, 100.0)))
        };
    }
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
    /// Fusion right after the last fix, before gyro replay: the start of the next gyro-vs-camera check.
    check_start: Option<(u64, PointerFusion, [f64; 2])>,
    check: GyroCheck,
}

impl Tracker {
    /// `long_side_fov_deg`: camera field of view across the image's **long** side (65° is typical for a phone's
    /// main camera in 16:9 video). Only scales gyro-predicted motion, and the online gyro check corrects it anyway.
    pub fn new(layout: MarkerLayout, width: u16, height: u16, long_side_fov_deg: f64) -> Self {
        let (w, h) = (f64::from(width), f64::from(height));
        let half = (long_side_fov_deg.to_radians() / 2.0).tan();
        let width_fov_deg = 2.0 * (half * w / w.max(h)).atan().to_degrees();
        let mut fusion_cfg = FusionConfig::for_camera(w, h, width_fov_deg);
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
            check_start: None,
            check: GyroCheck::new(),
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
        self.check_start = None;
    }

    fn integrate_into(fusion: &mut PointerFusion, from_us: u64, s: &DeviceGyro) {
        let dt_us = s.t_us.saturating_sub(from_us);
        if dt_us > 0 && dt_us <= MAX_GYRO_STEP_US {
            fusion.gyro(device_to_camera(s.rate), dt_us as f64 / 1e6);
        }
    }

    fn integrate(&mut self, from_us: u64, s: &DeviceGyro) {
        Self::integrate_into(&mut self.fusion, from_us, s);
    }

    /// Pure-gyro prediction from the previous fix to `capture_us`, compared with the new fix.
    fn check_gyro(&mut self, capture_us: u64, est: &AimEstimate) {
        let Some((t0, mut f, p0)) = self.check_start.take() else {
            return;
        };
        if capture_us.saturating_sub(t0) > CHECK_MAX_GAP_US {
            return;
        }
        let mut from = t0;
        let mut any = false;
        for s in self.gyro.iter().filter(|s| s.t_us > t0 && s.t_us <= capture_us) {
            Self::integrate_into(&mut f, from, s);
            from = s.t_us;
            any = true;
        }
        if !any {
            return; // no gyro data in between (sensor off): nothing to learn
        }
        if let Some(p) = raw_pointer(f.state()) {
            self.check
                .add([est.x - p0[0], est.y - p0[1]], [p[0] - p0[0], p[1] - p0[1]]);
        }
    }

    /// The learned gyro → camera map (identity until enough motion has been seen; zero = gyro not trusted).
    pub fn gyro_map(&self) -> GyroMap {
        self.check.map
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
        self.check_gyro(capture_us, &est);
        self.fusion.camera_fix(&est);
        self.check_start = Some((capture_us, self.fusion.clone(), [est.x, est.y]));
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

    /// Current propagated pointer state (staleness is the caller's job). Gyro-predicted motion since the last fix
    /// goes through the learned [`GyroMap`].
    pub fn state(&self) -> PointerState {
        let raw = self.fusion.state();
        let (Some(p), Some((_, _, fix))) = (raw_pointer(raw), self.check_start.as_ref()) else {
            return raw;
        };
        let m = self.check.map;
        let d = [p[0] - fix[0], p[1] - fix[1]];
        let x = fix[0] + m[0][0] * d[0] + m[0][1] * d[1];
        let y = fix[1] + m[1][0] * d[0] + m[1][1] * d[1];
        match raw {
            PointerState::Tracking { .. } => PointerState::Tracking { x, y },
            PointerState::GyroOnly { .. } => PointerState::GyroOnly { x, y },
            PointerState::Lost => PointerState::Lost,
        }
    }

    pub fn last_tag_count(&self) -> usize {
        self.last_tags
    }
}

fn raw_pointer(s: PointerState) -> Option<[f64; 2]> {
    match s {
        PointerState::Tracking { x, y } | PointerState::GyroOnly { x, y } => Some([x, y]),
        PointerState::Lost => None,
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
