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
