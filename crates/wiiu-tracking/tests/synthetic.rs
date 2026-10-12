//! Deterministic pointing tests with a synthetic pinhole camera.
//!
//! World units: the game image spans x ∈ [0, aspect], y ∈ [0, 1] (y down) on the plane z = 0; the player stands
//! at negative z looking toward +z. Ground truth is the intersection of the pointing ray with the screen plane.

use wiiu_tracking::linalg::Vec2;
use wiiu_tracking::*;

const W: f64 = 1280.0;
const H: f64 = 720.0;
const F: f64 = 1000.0; // focal length in px (~65° horizontal FOV)
const ASPECT: f64 = 16.0 / 9.0;

type V3 = [f64; 3];

fn rot(yaw: f64, pitch: f64, roll: f64) -> [[f64; 3]; 3] {
    // camera → world, R = Ry(yaw) · Rx(pitch) · Rz(roll)
    let (cy, sy) = (yaw.cos(), yaw.sin());
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (cr, sr) = (roll.cos(), roll.sin());
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rx = [[1.0, 0.0, 0.0], [0.0, cp, -sp], [0.0, sp, cp]];
    let rz = [[cr, -sr, 0.0], [sr, cr, 0.0], [0.0, 0.0, 1.0]];
    mul(&mul(&ry, &rx), &rz)
}

fn mul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

struct Camera {
    pos: V3,
    r: [[f64; 3]; 3],
}

impl Camera {
    fn project(&self, game: Vec2) -> Option<Vec2> {
        let w = [game[0] * ASPECT - self.pos[0], game[1] - self.pos[1], -self.pos[2]];
        // camera coords = Rᵀ · (X - C)
        let c: V3 = std::array::from_fn(|i| (0..3).map(|k| self.r[k][i] * w[k]).sum());
        (c[2] > 1e-6).then(|| [F * c[0] / c[2] + W / 2.0, F * c[1] / c[2] + H / 2.0])
    }

    /// Ground-truth game-normalised point hit by the ray through image point `p`.
    fn hit(&self, p: Vec2) -> Option<Vec2> {
        let d_cam = [(p[0] - W / 2.0) / F, (p[1] - H / 2.0) / F, 1.0];
        let d: V3 = std::array::from_fn(|i| (0..3).map(|k| self.r[i][k] * d_cam[k]).sum());
        if d[2] <= 1e-9 {
            return None;
        }
        let t = -self.pos[2] / d[2];
        Some([(self.pos[0] + t * d[0]) / ASPECT, self.pos[1] + t * d[1]])
    }

    fn observe(&self, layout: &MarkerLayout, noise: &mut impl FnMut() -> f64) -> Vec<TagObservation> {
        layout
            .markers
            .iter()
            .filter_map(|m| {
                let mut corners = [[0.0; 2]; 4];
                for (c, g) in corners.iter_mut().zip(m.corners) {
                    let p = self.project(g)?;
                    if !(0.0..W).contains(&p[0]) || !(0.0..H).contains(&p[1]) {
                        return None; // tag (partly) outside the camera frame
                    }
                    *c = [p[0] + noise(), p[1] + noise()];
                }
                Some(TagObservation { id: m.id, corners })
            })
            .collect()
    }
}

/// Deterministic pseudo-random numbers (no external crates).
struct Lcg(u64);
impl Lcg {
    fn uniform(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.uniform()
    }
    fn gauss(&mut self, sigma: f64) -> f64 {
        let (u1, u2) = (self.uniform().max(1e-12), self.uniform());
        sigma * (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

fn layout() -> MarkerLayout {
    MarkerLayout::border(&BorderConfig::default())
}

fn cfg() -> AimConfig {
    AimConfig::centred(W, H)
}

fn random_camera(rng: &mut Lcg) -> Camera {
    let dist = rng.range(2.0, 4.0); // 2–4 screen heights away
    let pos = [ASPECT / 2.0 + rng.range(-0.6, 0.6), 0.5 + rng.range(-0.3, 0.3), -dist];
    // Point roughly at a random spot on the screen, then add roll.
    let target = [rng.range(0.1, 0.9) * ASPECT, rng.range(0.1, 0.9)];
    let d = [target[0] - pos[0], target[1] - pos[1], -pos[2]];
    let yaw = d[0].atan2(d[2]);
    let pitch = -(d[1]).atan2((d[0] * d[0] + d[2] * d[2]).sqrt());
    Camera {
        pos,
        r: rot(yaw, pitch, rng.range(-0.5, 0.5)),
    }
}

#[test]
fn centred_camera_aims_at_centre() {
    let cam = Camera {
        pos: [ASPECT / 2.0, 0.5, -2.5],
        r: rot(0.0, 0.0, 0.0),
    };
    let obs = cam.observe(&layout(), &mut || 0.0);
    assert_eq!(obs.len(), layout().markers.len(), "whole border visible");
    let est = estimate_aim(&obs, &layout(), &cfg()).unwrap();
    assert!((est.x - 0.5).abs() < 1e-9 && (est.y - 0.5).abs() < 1e-9, "{est:?}");
    assert!(est.rms_px < 1e-6);
}

#[test]
fn noise_free_random_poses_are_exact() {
    let mut rng = Lcg(42);
    let mut checked = 0;
    for _ in 0..300 {
        let cam = random_camera(&mut rng);
        let obs = cam.observe(&layout(), &mut || 0.0);
        let Ok(est) = estimate_aim(&obs, &layout(), &cfg()) else {
            continue;
        };
        let truth = cam.hit([W / 2.0, H / 2.0]).unwrap();
        assert!(
            (est.x - truth[0]).abs() < 1e-6 && (est.y - truth[1]).abs() < 1e-6,
            "est {est:?} truth {truth:?}"
        );
        checked += 1;
    }
    assert!(checked > 200, "only {checked} poses had enough visible tags");
}

#[test]
fn half_pixel_noise_stays_within_one_percent() {
    let mut rng = Lcg(7);
    let mut noise_rng = Lcg(99);
    let mut errors = Vec::new();
    for _ in 0..300 {
        let cam = random_camera(&mut rng);
        let obs = cam.observe(&layout(), &mut || noise_rng.gauss(0.5));
        let Ok(est) = estimate_aim(&obs, &layout(), &cfg()) else {
            continue;
        };
        let truth = cam.hit([W / 2.0, H / 2.0]).unwrap();
        // Error in screen widths (x scaled by aspect so both axes use the same unit).
        let err = ((est.x - truth[0]) * ASPECT).hypot(est.y - truth[1]) / ASPECT;
        errors.push(err);
    }
    errors.sort_by(f64::total_cmp);
    let median = errors[errors.len() / 2];
    let p95 = errors[errors.len() * 95 / 100];
    eprintln!(
        "0.5 px noise: {} poses, median {median:.5}, p95 {p95:.5} screen widths",
        errors.len()
    );
    assert!(errors.len() > 200);
    assert!(median < 0.003, "median error {median}");
    assert!(p95 < 0.01, "95th percentile error {p95}");
}

#[test]
fn roll_does_not_move_the_aim_point() {
    let base = Camera {
        pos: [0.3, 0.2, -3.0],
        r: rot(0.15, -0.05, 0.0),
    };
    let a = estimate_aim(&base.observe(&layout(), &mut || 0.0), &layout(), &cfg()).unwrap();
    for roll in [-0.6, -0.2, 0.3, 0.7] {
        let rolled = Camera {
            pos: base.pos,
            r: rot(0.15, -0.05, roll),
        };
        let b = estimate_aim(&rolled.observe(&layout(), &mut || 0.0), &layout(), &cfg()).unwrap();
        assert!(
            (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6,
            "roll {roll}: {a:?} vs {b:?}"
        );
    }
}

#[test]
fn calibration_corrects_an_offset_pointing_axis() {
    // The phone "points" along a direction 8° left and 5° up of the camera axis.
    let true_aim_px = [W / 2.0 - F * 0.14, H / 2.0 - F * 0.087];
    let mut rng = Lcg(5);

    // Calibrate: user aims at the centre of the screen.
    let target = [0.5, 0.5];
    let calib_cam = {
        // Orient so that the true pointing ray hits the target.
        let pos = [ASPECT / 2.0, 0.5, -2.5];
        let mut best = (
            f64::MAX,
            Camera {
                pos,
                r: rot(0.0, 0.0, 0.0),
            },
        );
        for yi in -40..=40 {
            for pi in -40..=40 {
                let cam = Camera {
                    pos,
                    r: rot(yi as f64 * 0.005, pi as f64 * 0.005, 0.0),
                };
                let h = cam.hit(true_aim_px).unwrap();
                let e = (h[0] - target[0]).hypot(h[1] - target[1]);
                if e < best.0 {
                    best = (e, cam);
                }
            }
        }
        best.1
    };
    let est = estimate_aim(&calib_cam.observe(&layout(), &mut || 0.0), &layout(), &cfg()).unwrap();
    let aim_px = calibrate_aim_point(&est, target).unwrap();
    let calibrated = AimConfig {
        aim_image_point: aim_px,
        ..cfg()
    };

    // After calibration, random poses report where the true pointing ray hits.
    let mut checked = 0;
    for _ in 0..100 {
        let cam = random_camera(&mut rng);
        let Ok(est) = estimate_aim(&cam.observe(&layout(), &mut || 0.0), &layout(), &calibrated) else {
            continue;
        };
        let Some(truth) = cam.hit(true_aim_px) else { continue };
        // Residual comes only from the grid search in the calibration pose (≤ ~0.3% of the screen).
        assert!((est.x - truth[0]).abs() < 0.01 && (est.y - truth[1]).abs() < 0.01);
        checked += 1;
    }
    assert!(checked > 50);
}

#[test]
fn rejects_too_few_or_inconsistent_tags() {
    let cam = Camera {
        pos: [ASPECT / 2.0, 0.5, -2.5],
        r: rot(0.0, 0.0, 0.0),
    };
    let obs = cam.observe(&layout(), &mut || 0.0);
    assert!(matches!(
        estimate_aim(&obs[..1], &layout(), &cfg()),
        Err(AimError::NotEnoughTags { found: 1, needed: 2 })
    ));
    // A tag reported with the wrong id (swapped position) makes the fit inconsistent.
    let mut bad = obs.clone();
    bad[0].id = obs[5].id;
    bad[5].id = obs[0].id;
    assert!(matches!(
        estimate_aim(&bad, &layout(), &cfg()),
        Err(AimError::PoorFit { .. })
    ));
    // Unknown ids are ignored.
    let mut extra = obs.clone();
    extra.push(TagObservation {
        id: 999,
        corners: [[1.0, 1.0]; 4],
    });
    assert!(estimate_aim(&extra, &layout(), &cfg()).is_ok());
}

#[test]
fn aiming_beside_the_screen_gives_no_pointer() {
    let cam = Camera {
        pos: [ASPECT / 2.0, 0.5, -2.5],
        r: rot(0.45, 0.0, 0.0), // ~26° to the right: still sees the left part of the border
    };
    let est = estimate_aim(&cam.observe(&layout(), &mut || 0.0), &layout(), &cfg()).unwrap();
    assert!(est.x > 1.0, "{est:?}");
    assert_eq!(est.pointer(), None);
}

// ---------------------------------------------------------------------------------------------------------------
// Gyro fusion

fn fusion_cfg(focal: f64) -> FusionConfig {
    FusionConfig {
        focal_px: focal,
        principal: [W / 2.0, H / 2.0],
        aim_image_point: [W / 2.0, H / 2.0],
        max_gyro_only_s: 2.0,
    }
}

/// Rotates the synthetic camera by body-frame angular velocity `w` for `dt` (same convention as the fusion).
fn spin(cam: &mut Camera, w: [f64; 3], dt: f64) {
    cam.r = mul(
        &cam.r,
        &wiiu_tracking::fusion::exp_so3([w[0] * dt, w[1] * dt, w[2] * dt]),
    );
}

fn start_cam() -> Camera {
    Camera {
        pos: [ASPECT / 2.0 + 0.2, 0.45, -2.8],
        r: rot(-0.05, 0.03, 0.1),
    }
}

#[test]
fn gyro_propagation_is_exact_for_pure_rotation() {
    let mut cam = start_cam();
    let mut fusion = PointerFusion::new(fusion_cfg(F));
    let est = estimate_aim(&cam.observe(&layout(), &mut || 0.0), &layout(), &cfg()).unwrap();
    fusion.camera_fix(&est);
    let w = [0.3, -0.4, 0.8]; // rad/s: pitch, yaw, roll
    for _ in 0..50 {
        spin(&mut cam, w, 0.005);
        fusion.gyro(w, 0.005);
    }
    let truth = cam.hit([W / 2.0, H / 2.0]).unwrap();
    let PointerState::Tracking { x, y } = fusion.state() else {
        panic!("{:?}", fusion.state())
    };
    assert!(
        (x - truth[0]).abs() < 1e-6 && (y - truth[1]).abs() < 1e-6,
        "({x}, {y}) vs {truth:?}"
    );
}

#[test]
fn focal_length_error_only_scales_gyro_motion() {
    let mut cam = start_cam();
    let mut fusion = PointerFusion::new(fusion_cfg(F * 1.1)); // 10 % wrong FOV guess
    let est = estimate_aim(&cam.observe(&layout(), &mut || 0.0), &layout(), &cfg()).unwrap();
    fusion.camera_fix(&est);
    let w = [0.0, 0.35, 0.0]; // ~5° yaw over 0.25 s
    for _ in 0..50 {
        spin(&mut cam, w, 0.005);
        fusion.gyro(w, 0.005);
    }
    let truth = cam.hit([W / 2.0, H / 2.0]).unwrap();
    let PointerState::Tracking { x, y } = fusion.state() else {
        panic!()
    };
    assert!(
        (x - truth[0]).abs() < 0.02 && (y - truth[1]).abs() < 0.02,
        "({x}, {y}) vs {truth:?}"
    );
}

#[test]
fn camera_fixes_bound_gyro_bias_drift() {
    let mut cam = start_cam();
    let mut fusion = PointerFusion::new(fusion_cfg(F));
    let bias = [0.02, -0.03, 0.01]; // rad/s, typical uncalibrated phone gyro bias
    let mut max_err: f64 = 0.0;
    for step in 0..400 {
        // 2 s of slow sweeping at 200 Hz, camera fix every 33 ms (≈30 fps).
        let w = [0.2 * (step as f64 * 0.02).sin(), 0.3 * (step as f64 * 0.015).cos(), 0.0];
        spin(&mut cam, w, 0.005);
        fusion.gyro([w[0] + bias[0], w[1] + bias[1], w[2] + bias[2]], 0.005);
        if step % 7 == 0 {
            if let Ok(est) = estimate_aim(&cam.observe(&layout(), &mut || 0.0), &layout(), &cfg()) {
                fusion.camera_fix(&est);
            }
        }
        if let (Some(truth), PointerState::Tracking { x, y }) = (cam.hit([W / 2.0, H / 2.0]), fusion.state()) {
            max_err = max_err.max((x - truth[0]).abs().max((y - truth[1]).abs()));
        }
    }
    assert!(max_err < 0.005, "max error {max_err}");
}

#[test]
fn markers_lost_switches_to_gyro_only_then_lost() {
    let cam = start_cam();
    let mut fusion = PointerFusion::new(fusion_cfg(F));
    let est = estimate_aim(&cam.observe(&layout(), &mut || 0.0), &layout(), &cfg()).unwrap();
    fusion.camera_fix(&est);
    assert!(matches!(fusion.state(), PointerState::Tracking { .. }));
    fusion.advance(0.5);
    assert!(matches!(fusion.state(), PointerState::GyroOnly { .. }));
    assert!(fusion.state().pointer().is_some());
    fusion.advance(1.6);
    assert_eq!(fusion.state(), PointerState::Lost);
    assert_eq!(fusion.state().pointer(), None);
}

// ---------------------------------------------------------------------------------------------------------------
// Tracker: latency compensation (frame arrives after the gyro samples recorded since its capture)

#[test]
fn tracker_replays_gyro_recorded_after_the_late_frame() {
    use wiiu_tracking::pipeline::{DeviceGyro, Tracker};
    let mut cam = start_cam();
    let mut tracker = Tracker::new(layout(), W as u16, H as u16, 2.0 * (W / 2.0 / F).atan().to_degrees());
    // Frame captured at t = 1 s.
    let capture_us = 1_000_000u64;
    let obs_at_capture = cam.observe(&layout(), &mut || 0.0);
    // The phone keeps turning for 80 ms (network + detection latency); gyro arrives first.
    let w_cam = [0.4, -0.6, 0.3];
    let w_device = [w_cam[0], -w_cam[1], -w_cam[2]]; // inverse of device_to_camera
    let mut gyro = Vec::new();
    for i in 1..=16 {
        spin(&mut cam, w_cam, 0.005);
        gyro.push(DeviceGyro {
            t_us: capture_us + i * 5_000,
            rate: w_device,
        });
    }
    tracker.on_gyro(&gyro);
    tracker.on_observations(capture_us, &obs_at_capture).unwrap();
    let truth_now = cam.hit([W / 2.0, H / 2.0]).unwrap();
    let PointerState::Tracking { x, y } = tracker.state() else {
        panic!("{:?}", tracker.state())
    };
    assert!(
        (x - truth_now[0]).abs() < 1e-6 && (y - truth_now[1]).abs() < 1e-6,
        "({x},{y}) vs {truth_now:?}"
    );

    // An older frame arriving late must not rewind the pointer.
    tracker.on_observations(capture_us - 50_000, &obs_at_capture).unwrap();
    let PointerState::Tracking { x: x2, y: y2 } = tracker.state() else {
        panic!()
    };
    assert_eq!((x, y), (x2, y2));
}

// ---------------------------------------------------------------------------------------------------------------
// Tracker: online gyro check (phones report sensor axes/units differently; the tracker must notice and adapt)

/// Sweeps the synthetic camera for `secs` with 200 Hz gyro and 30 fps fixes. `report` turns the true device rate
/// into what a (possibly faulty) phone reports. Returns the tracker and the mean pointer error measured just before
/// each fix (pure gyro prediction over 33 ms), over the last second.
fn sweep(secs: f64, mut report: impl FnMut([f64; 3]) -> [f64; 3]) -> (wiiu_tracking::pipeline::Tracker, f64) {
    use wiiu_tracking::pipeline::{DeviceGyro, Tracker};
    let mut cam = start_cam();
    let mut tracker = Tracker::new(layout(), W as u16, H as u16, 2.0 * (W / 2.0 / F).atan().to_degrees());
    let steps = (secs * 200.0) as u64;
    let (mut err_sum, mut err_n) = (0.0, 0);
    for step in 1..=steps {
        let t = step as f64 / 200.0;
        let w_cam = [
            0.5 * (2.0 * std::f64::consts::PI * 0.7 * t).sin(),
            0.7 * (2.0 * std::f64::consts::PI * 0.5 * t).cos(),
            0.0,
        ];
        spin(&mut cam, w_cam, 0.005);
        let w_device = [w_cam[0], -w_cam[1], -w_cam[2]];
        let t_us = step * 5_000;
        tracker.on_gyro(&[DeviceGyro {
            t_us,
            rate: report(w_device),
        }]);
        if step % 7 == 0 {
            if t > secs - 1.0 {
                if let (Some(truth), Some((x, y))) = (cam.hit([W / 2.0, H / 2.0]), tracker.state().pointer()) {
                    err_sum += (x - truth[0]).hypot(y - truth[1]);
                    err_n += 1;
                }
            }
            let _ = tracker.on_observations(t_us, &cam.observe(&layout(), &mut || 0.0));
        }
    }
    (tracker, err_sum / err_n.max(1) as f64)
}

#[test]
fn gyro_check_keeps_an_honest_gyro() {
    let (tracker, err) = sweep(4.0, |w| w);
    let m = tracker.gyro_map();
    assert!((m[0][0] - 1.0).abs() < 0.15 && (m[1][1] - 1.0).abs() < 0.15, "{m:?}");
    assert!(m[0][1].abs() < 0.15 && m[1][0].abs() < 0.15, "{m:?}");
    assert!(err < 0.002, "prediction error {err}");
}

#[test]
fn gyro_check_corrects_a_flipped_axis_and_wrong_units() {
    // Phone reports yaw with the wrong sign and everything 3× too large.
    let (_, err_hold) = sweep(4.0, |_| [0.0; 3]);
    let (tracker, err) = sweep(4.0, |w| [3.0 * w[0], -3.0 * w[1], 3.0 * w[2]]);
    let m = tracker.gyro_map();
    assert!((m[0][0] + 1.0 / 3.0).abs() < 0.1, "x gain {m:?}");
    assert!((m[1][1] - 1.0 / 3.0).abs() < 0.1, "y gain {m:?}");
    assert!(
        err < 0.25 * err_hold,
        "corrected gyro {err} vs holding still {err_hold}"
    );
}

#[test]
fn gyro_check_ignores_a_gyro_that_predicts_nothing() {
    let mut rng = Lcg(7);
    let (tracker, err) = sweep(4.0, |_| [rng.gauss(0.6), rng.gauss(0.6), rng.gauss(0.6)]);
    let (_, err_hold) = sweep(4.0, |_| [0.0; 3]);
    let m = tracker.gyro_map();
    assert!(m.iter().flatten().all(|v| v.abs() < 0.2), "{m:?}");
    assert!(err < 1.5 * err_hold, "noise gyro {err} vs holding still {err_hold}");
}
