//! Aim-point estimation: where on the game image is the phone pointing?
//!
//! For a pinhole camera, the map between image pixels and the (planar) screen is a homography `H`. The phone's
//! pointing direction is a fixed ray through the camera centre, which corresponds to a fixed image point `a`
//! (the principal point when pointing along the camera axis, otherwise found by calibration). The aim point is
//! simply `H(a)`. This needs no camera intrinsics, and roll of the phone around the pointing axis does not move
//! the aim point, matching a real Wii Remote.

use crate::homography;
use crate::linalg::{Mat3, Vec2};
use crate::markers::MarkerLayout;

#[derive(Debug, Clone, PartialEq)]
pub struct TagObservation {
    pub id: u32,
    /// Corner pixels in [`crate::markers::CORNER_ORDER`].
    pub corners: [Vec2; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimConfig {
    /// Image point corresponding to the pointing direction (pixels). Default: image centre.
    pub aim_image_point: Vec2,
    /// Reject fits whose reprojection error exceeds this many pixels.
    pub max_rms_px: f64,
    /// Minimum number of distinct tags (one tag's 4 corners extrapolate poorly across the screen).
    pub min_tags: usize,
}

impl AimConfig {
    pub fn centred(image_width: f64, image_height: f64) -> Self {
        Self {
            aim_image_point: [image_width / 2.0, image_height / 2.0],
            max_rms_px: 3.0,
            min_tags: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimEstimate {
    /// Game-normalised aim point; may lie outside 0..=1 when aiming beside the screen.
    pub x: f64,
    pub y: f64,
    /// Reprojection RMS error of the tag corners, in pixels.
    pub rms_px: f64,
    pub tags_used: usize,
    /// Image → game-normalised homography.
    pub image_to_game: Mat3,
}

impl AimEstimate {
    /// Protocol `pointer` value: `None` when aiming off the game image.
    pub fn pointer(&self) -> Option<(f64, f64)> {
        ((0.0..=1.0).contains(&self.x) && (0.0..=1.0).contains(&self.y)).then_some((self.x, self.y))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AimError {
    NotEnoughTags {
        found: usize,
        needed: usize,
    },
    Degenerate,
    PoorFit {
        rms_px: f64,
    },
    /// Pointing direction is parallel to the screen plane.
    AimAtInfinity,
}

/// Estimates the aim point from detected tags. Unknown tag ids and duplicate detections are ignored.
pub fn estimate_aim(
    observations: &[TagObservation],
    layout: &MarkerLayout,
    cfg: &AimConfig,
) -> Result<AimEstimate, AimError> {
    let mut seen = Vec::new();
    let mut img = Vec::new();
    let mut game = Vec::new();
    for o in observations {
        if seen.contains(&o.id) {
            continue;
        }
        if let Some(m) = layout.marker(o.id) {
            seen.push(o.id);
            img.extend_from_slice(&o.corners);
            game.extend_from_slice(&m.corners);
        }
    }
    if seen.len() < cfg.min_tags.max(1) {
        return Err(AimError::NotEnoughTags {
            found: seen.len(),
            needed: cfg.min_tags.max(1),
        });
    }
    let h = homography::estimate(&img, &game).ok_or(AimError::Degenerate)?;
    let game_to_image = h.inverse().ok_or(AimError::Degenerate)?;
    let rms_px = homography::rms_error(&game_to_image, &game, &img);
    if !rms_px.is_finite() || rms_px > cfg.max_rms_px {
        return Err(AimError::PoorFit { rms_px });
    }
    let [x, y] = h.transform(cfg.aim_image_point).ok_or(AimError::AimAtInfinity)?;
    Ok(AimEstimate {
        x,
        y,
        rms_px,
        tags_used: seen.len(),
        image_to_game: h,
    })
}

/// Calibration: given a fit made while the user aims at `target` (game-normalised), returns the image point that
/// represents the phone's pointing direction. Store it per device and use it as [`AimConfig::aim_image_point`].
pub fn calibrate_aim_point(estimate: &AimEstimate, target: Vec2) -> Option<Vec2> {
    estimate.image_to_game.inverse()?.transform(target)
}
