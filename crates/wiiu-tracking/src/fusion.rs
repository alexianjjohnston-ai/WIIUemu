//! Gyroscope fusion between camera fixes.
//!
//! Each camera fix gives a homography `H` (image → game) and therefore an absolute aim point. Between fixes the
//! phone keeps rotating; the gyroscope measures that rotation. For a camera that only rotates (translation over a
//! few frames is negligible at TV distance), the pointing direction in the *last fix's* camera frame is
//! `R · K⁻¹ · a`, where `R` is the rotation accumulated since the fix and `a` the aim image point. Projecting it
//! with `K` and mapping through the last `H` gives the predicted aim. Every new camera fix resets `R`, which
//! removes accumulated gyro drift.
//!
//! Gyro input is the angular velocity **in the camera frame** (x right, y down, z forward, rad/s). For a phone's
//! rear camera in portrait with an unrotated image, that is `(ωx, −ωy, −ωz)` of the Android/W3C device frame.

use crate::aim::AimEstimate;
use crate::linalg::{Mat3, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FusionConfig {
    /// Focal length in pixels (≈ image_width / (2·tan(hfov/2))). Errors only scale gyro-predicted motion.
    pub focal_px: f64,
    /// Principal point in pixels (normally the image centre).
    pub principal: Vec2,
    /// Aim image point (see [`crate::AimConfig::aim_image_point`]).
    pub aim_image_point: Vec2,
    /// How long the pointer may run on gyro alone after the last fix before it is reported lost.
    pub max_gyro_only_s: f64,
}

impl FusionConfig {
    pub fn for_camera(width: f64, height: f64, horizontal_fov_deg: f64) -> Self {
        Self {
            focal_px: width / (2.0 * (horizontal_fov_deg.to_radians() / 2.0).tan()),
            principal: [width / 2.0, height / 2.0],
            aim_image_point: [width / 2.0, height / 2.0],
            max_gyro_only_s: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerState {
    /// Aim point from a fresh camera fix (possibly gyro-propagated within the fix interval).
    Tracking { x: f64, y: f64 },
    /// Markers lost; aim point propagated by gyro only.
    GyroOnly { x: f64, y: f64 },
    /// No fix yet, or gyro-only time exceeded.
    Lost,
}

impl PointerState {
    /// Protocol `pointer`: `None` when lost or off the game image.
    pub fn pointer(&self) -> Option<(f64, f64)> {
        match *self {
            PointerState::Tracking { x, y } | PointerState::GyroOnly { x, y } => {
                ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)).then_some((x, y))
            }
            PointerState::Lost => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PointerFusion {
    cfg: FusionConfig,
    last_fix: Option<(Mat3, f64)>,
    /// Rotation since the last fix: maps current camera-frame vectors to last-fix camera-frame vectors.
    rot: [[f64; 3]; 3],
    now: f64,
    /// A fix older than this counts as "markers lost".
    fix_timeout_s: f64,
}

fn mat_mul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut r = [[0.0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

/// Rodrigues: rotation matrix for rotation vector `w` (axis · angle).
pub fn exp_so3(w: [f64; 3]) -> [[f64; 3]; 3] {
    let theta = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    if theta < 1e-12 {
        return [[1.0, -w[2], w[1]], [w[2], 1.0, -w[0]], [-w[1], w[0], 1.0]];
    }
    let k = [w[0] / theta, w[1] / theta, w[2] / theta];
    let (s, c) = theta.sin_cos();
    let v = 1.0 - c;
    [
        [
            c + k[0] * k[0] * v,
            k[0] * k[1] * v - k[2] * s,
            k[0] * k[2] * v + k[1] * s,
        ],
        [
            k[1] * k[0] * v + k[2] * s,
            c + k[1] * k[1] * v,
            k[1] * k[2] * v - k[0] * s,
        ],
        [
            k[2] * k[0] * v - k[1] * s,
            k[2] * k[1] * v + k[0] * s,
            c + k[2] * k[2] * v,
        ],
    ]
}

const IDENTITY3: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

impl PointerFusion {
    pub fn new(cfg: FusionConfig) -> Self {
        Self {
            cfg,
            last_fix: None,
            rot: IDENTITY3,
            now: 0.0,
            fix_timeout_s: 0.3,
        }
    }

    pub fn config(&self) -> &FusionConfig {
        &self.cfg
    }

    /// Integrates a gyro sample (camera frame, rad/s) over `dt` seconds.
    pub fn gyro(&mut self, omega: [f64; 3], dt: f64) {
        if !(dt.is_finite() && dt > 0.0 && dt < 0.5) || omega.iter().any(|w| !w.is_finite()) {
            return;
        }
        self.now += dt;
        self.rot = mat_mul(&self.rot, &exp_so3([omega[0] * dt, omega[1] * dt, omega[2] * dt]));
    }

    /// Applies a camera fix taken at the current time.
    pub fn camera_fix(&mut self, estimate: &AimEstimate) {
        self.last_fix = Some((estimate.image_to_game, self.now));
        self.rot = IDENTITY3;
    }

    /// Advances time without gyro data (e.g. sensor unavailable).
    pub fn advance(&mut self, dt: f64) {
        if dt.is_finite() && dt > 0.0 {
            self.now += dt;
        }
    }

    pub fn state(&self) -> PointerState {
        let Some((h, t_fix)) = self.last_fix else {
            return PointerState::Lost;
        };
        let age = self.now - t_fix;
        if age > self.cfg.max_gyro_only_s {
            return PointerState::Lost;
        }
        let f = self.cfg.focal_px;
        let [cx, cy] = self.cfg.principal;
        let [ax, ay] = self.cfg.aim_image_point;
        let d_now = [(ax - cx) / f, (ay - cy) / f, 1.0];
        let d: [f64; 3] = std::array::from_fn(|i| (0..3).map(|k| self.rot[i][k] * d_now[k]).sum());
        if d[2] <= 1e-6 {
            return PointerState::Lost; // pointing behind the last fix's image plane
        }
        let p = [f * d[0] / d[2] + cx, f * d[1] / d[2] + cy];
        match h.transform(p) {
            Some([x, y]) if age <= self.fix_timeout_s => PointerState::Tracking { x, y },
            Some([x, y]) => PointerState::GyroOnly { x, y },
            None => PointerState::Lost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_so3_is_a_rotation() {
        let r = exp_so3([0.3, -0.2, 0.5]);
        let rt_r = mat_mul(
            &[
                [r[0][0], r[1][0], r[2][0]],
                [r[0][1], r[1][1], r[2][1]],
                [r[0][2], r[1][2], r[2][2]],
            ],
            &r,
        );
        for (i, row) in rt_r.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                assert!((v - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn lost_without_fix_and_rejects_bad_samples() {
        let mut f = PointerFusion::new(FusionConfig::for_camera(1280.0, 720.0, 65.0));
        assert_eq!(f.state(), PointerState::Lost);
        f.gyro([f64::NAN, 0.0, 0.0], 0.01);
        f.gyro([0.0, 0.0, 0.0], -1.0);
        assert_eq!(f.now, 0.0);
    }
}
