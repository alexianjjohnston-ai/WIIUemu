//! Camera-based absolute pointing (Milestone 2).
//!
//! Pure math, no I/O: marker layout around the game image, image → screen homography, aim point, calibration.
//! Detection itself (finding AprilTags in camera frames) happens on the phone; see `docs/tracking.md`.

pub mod aim;
#[cfg(feature = "detector")]
pub mod detect;
pub mod fusion;
pub mod homography;
pub mod linalg;
pub mod markers;
pub mod pipeline;
pub mod tag36h11;
pub mod tags;

pub use aim::{calibrate_aim_point, estimate_aim, AimConfig, AimError, AimEstimate, TagObservation};
pub use fusion::{FusionConfig, PointerFusion, PointerState};
pub use markers::{BorderConfig, Marker, MarkerLayout};
