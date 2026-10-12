//! Full PC-side path on a real image: JPEG → decode → AprilTag C detector → Tracker → pointer.
#![cfg(feature = "detector")]

use wiiu_tracking::detect::{decode_jpeg_gray, TagDetector};
use wiiu_tracking::pipeline::Tracker;
use wiiu_tracking::{BorderConfig, MarkerLayout, PointerState};

const W: usize = 640;
const H: usize = 360;

fn render_frontal_gray(layout: &MarkerLayout, scale: f64) -> Vec<u8> {
    let ox = W as f64 / 2.0 - layout.aspect * scale / 2.0;
    let oy = H as f64 / 2.0 - scale / 2.0;
    let to_px = |x: f64, y: f64| (ox + x * layout.aspect * scale, oy + y * scale);
    let mut img = vec![90u8; W * H];
    for m in &layout.markers {
        let cells = wiiu_tracking::tags::render(m.id as usize).unwrap();
        let (x0, y0) = to_px(m.corners[3][0], m.corners[3][1]);
        let (x1, _) = to_px(m.corners[1][0], 0.0);
        let cell = (x1 - x0) / 8.0;
        for py in 0..H {
            for px in 0..W {
                let cx = ((px as f64 + 0.5 - x0) / cell + 1.0).floor();
                let cy = ((py as f64 + 0.5 - y0) / cell + 1.0).floor();
                if (0.0..10.0).contains(&cx) && (0.0..10.0).contains(&cy) {
                    img[py * W + px] = if cells[cy as usize][cx as usize] { 255 } else { 0 };
                }
            }
        }
    }
    img
}

fn jpeg(gray: &[u8], quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(gray, W as u16, H as u16, jpeg_encoder::ColorType::Luma)
        .unwrap();
    out
}

#[test]
fn jpeg_frame_produces_centre_pointer() {
    let layout = MarkerLayout::border(&BorderConfig::default());
    let data = jpeg(&render_frontal_gray(&layout, H as f64 * 0.7), 75);
    assert!(
        data.len() < 65_536,
        "fits the protocol payload limit: {} bytes",
        data.len()
    );
    let gray = decode_jpeg_gray(&data, (W as u16, H as u16)).unwrap();
    let mut det = TagDetector::new().unwrap();
    let obs = det.detect(&gray, W, H);
    assert_eq!(obs.len(), layout.markers.len(), "all tags survive JPEG q75");
    let mut tracker = Tracker::new(layout, W as u16, H as u16, 65.0);
    tracker.on_observations(1, &obs).unwrap();
    let PointerState::Tracking { x, y } = tracker.state() else {
        panic!()
    };
    assert!((x - 0.5).abs() < 0.01 && (y - 0.5).abs() < 0.01, "({x}, {y})");
}

#[test]
fn rejects_jpeg_whose_size_differs_from_the_header() {
    let layout = MarkerLayout::border(&BorderConfig::default());
    let data = jpeg(&render_frontal_gray(&layout, 200.0), 60);
    assert!(decode_jpeg_gray(&data, (320, 240)).is_err());
    assert!(decode_jpeg_gray(b"not a jpeg", (W as u16, H as u16)).is_err());
}
