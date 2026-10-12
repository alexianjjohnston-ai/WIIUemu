//! JPEG decoding and AprilTag detection (feature `detector`; wraps the vendored AprilTag C library).
//!
//! `apriltag::Detector` holds raw C pointers and is not `Send`: create one [`TagDetector`] per thread.

use crate::aim::TagObservation;

#[derive(Debug, thiserror::Error)]
pub enum DetectError {
    #[error("jpeg: {0}")]
    Jpeg(#[from] jpeg_decoder::Error),
    #[error("decoded image is {got:?}, header says {expected:?}")]
    SizeMismatch { expected: (u16, u16), got: (u16, u16) },
    #[error("unsupported pixel format {0:?}")]
    PixelFormat(jpeg_decoder::PixelFormat),
    #[error("detector setup failed: {0}")]
    Setup(String),
}

/// Decodes a baseline JPEG to 8-bit grayscale. Rejects images whose size differs from `expected`.
pub fn decode_jpeg_gray(jpeg: &[u8], expected: (u16, u16)) -> Result<Vec<u8>, DetectError> {
    let mut decoder = jpeg_decoder::Decoder::new(jpeg);
    // Read the header first so a lying size is rejected before decoding megapixels.
    decoder.read_info()?;
    let info = decoder.info().expect("read_info succeeded");
    let got = (info.width, info.height);
    if got != expected {
        return Err(DetectError::SizeMismatch { expected, got });
    }
    let pixels = decoder.decode()?;
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => Ok(pixels),
        jpeg_decoder::PixelFormat::RGB24 => Ok(pixels
            .chunks_exact(3)
            // ITU-R BT.601 luma, integer approximation.
            .map(|p| ((77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2])) >> 8) as u8)
            .collect()),
        other => Err(DetectError::PixelFormat(other)),
    }
}

pub struct TagDetector {
    detector: apriltag::Detector,
}

impl TagDetector {
    /// tag36h11 with 1-bit error correction (keeps false positives very low at TV-border sizes).
    pub fn new() -> Result<Self, DetectError> {
        let detector = apriltag::DetectorBuilder::new()
            .add_family_bits(apriltag::Family::tag_36h11(), 1)
            .build()
            .map_err(|e| DetectError::Setup(format!("{e:?}")))?;
        Ok(Self { detector })
    }

    /// Detects tags in a `width × height` grayscale image (row-major, no padding).
    pub fn detect(&mut self, gray: &[u8], width: usize, height: usize) -> Vec<TagObservation> {
        assert_eq!(gray.len(), width * height, "gray buffer size");
        let Ok(mut img) = apriltag::Image::zeros_with_stride(width, height, width) else {
            return Vec::new();
        };
        let stride = img.stride();
        let dst = img.as_slice_mut();
        for (y, row) in gray.chunks_exact(width).enumerate() {
            dst[y * stride..y * stride + width].copy_from_slice(row);
        }
        self.detector
            .detect(&img)
            .iter()
            .map(|d| {
                let c = d.corners();
                TagObservation {
                    id: d.id() as u32,
                    corners: [c[0], c[1], c[2], c[3]],
                }
            })
            .collect()
    }
}
