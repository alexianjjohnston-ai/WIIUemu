//! Vendored AprilTag C detector (tag36h11), exposed through a primitive-only C shim (`shim/wiiu_apriltag.c`)
//! so no Rust code depends on C struct layouts. Upstream: AprilRobotics/apriltag, BSD 2-Clause; see
//! `vendor/README.md`.

use std::ffi::c_int;
use std::ptr::NonNull;

#[repr(C)]
struct RawDetector {
    _private: [u8; 0],
}

extern "C" {
    fn wiiu_at_create(nthreads: c_int, quad_decimate: f32) -> *mut RawDetector;
    fn wiiu_at_detect(
        h: *mut RawDetector,
        gray: *const u8,
        width: c_int,
        height: c_int,
        stride: c_int,
        max_out: c_int,
        ids: *mut i32,
        corners: *mut f64,
        margins: *mut f32,
    ) -> c_int;
    fn wiiu_at_destroy(h: *mut RawDetector);
}

/// One detected tag. Corners follow `apriltag_detection_t.p`: bottom-left, bottom-right, top-right, top-left of
/// the upright tag, in image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub id: u32,
    pub corners: [[f64; 2]; 4],
    pub decision_margin: f32,
}

/// Most tags reported per frame (the TV border has about 10).
pub const MAX_DETECTIONS: usize = 128;

pub struct Detector {
    raw: NonNull<RawDetector>,
}

// SAFETY: the detector (and its worker pool) may be used from any one thread at a time; `detect` takes
// `&mut self`, so it is never used concurrently.
unsafe impl Send for Detector {}

impl Detector {
    /// tag36h11 with 1-bit error correction. `quad_decimate` ≥ 1 trades accuracy for speed (upstream default 2).
    pub fn tag36h11(threads: u8, quad_decimate: f32) -> Option<Self> {
        // SAFETY: plain constructor; returns NULL on allocation failure.
        let raw = unsafe { wiiu_at_create(c_int::from(threads.max(1)), quad_decimate) };
        NonNull::new(raw).map(|raw| Self { raw })
    }

    /// Detects tags in a row-major 8-bit grayscale image without padding. Returns `None` on detector failure
    /// or if the buffer doesn't match `width × height`.
    pub fn detect(&mut self, gray: &[u8], width: usize, height: usize) -> Option<Vec<Detection>> {
        if width == 0 || height == 0 || width.checked_mul(height)? != gray.len() {
            return None;
        }
        let (w, h) = (c_int::try_from(width).ok()?, c_int::try_from(height).ok()?);
        let mut ids = [0i32; MAX_DETECTIONS];
        let mut corners = [0f64; MAX_DETECTIONS * 8];
        let mut margins = [0f32; MAX_DETECTIONS];
        // SAFETY: `gray` holds width*height bytes (checked above) and stays borrowed for the call; the output
        // buffers hold MAX_DETECTIONS entries, which is the `max_out` we pass.
        let n = unsafe {
            wiiu_at_detect(
                self.raw.as_ptr(),
                gray.as_ptr(),
                w,
                h,
                w,
                MAX_DETECTIONS as c_int,
                ids.as_mut_ptr(),
                corners.as_mut_ptr(),
                margins.as_mut_ptr(),
            )
        };
        let n = usize::try_from(n).ok()?.min(MAX_DETECTIONS);
        Some(
            (0..n)
                .map(|i| {
                    let c = &corners[i * 8..i * 8 + 8];
                    Detection {
                        id: ids[i] as u32,
                        corners: [[c[0], c[1]], [c[2], c[3]], [c[4], c[5]], [c[6], c[7]]],
                        decision_margin: margins[i],
                    }
                })
                .collect(),
        )
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `wiiu_at_create` and is destroyed exactly once.
        unsafe { wiiu_at_destroy(self.raw.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_image_has_no_tags_and_bad_sizes_are_rejected() {
        let mut d = Detector::tag36h11(1, 2.0).expect("detector");
        assert_eq!(d.detect(&vec![200u8; 320 * 240], 320, 240), Some(vec![]));
        assert_eq!(d.detect(&[0u8; 10], 320, 240), None);
        assert_eq!(d.detect(&[], 0, 0), None);
    }
}
