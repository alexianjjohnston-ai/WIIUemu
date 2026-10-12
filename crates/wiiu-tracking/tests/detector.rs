//! Regression test against the real AprilTag detector (vendored C library, `crates/wiiu-apriltag-sys`):
//! our rendered border must be detected with the right ids, and the detector's corner order must match
//! `wiiu_tracking::markers::CORNER_ORDER`, so `estimate_aim` receives correctly paired points.

#![cfg(feature = "detector")]

use wiiu_tracking::detect::TagDetector;
use wiiu_tracking::{estimate_aim, AimConfig, BorderConfig, MarkerLayout, TagObservation};

const W: usize = 640;
const H: usize = 360;

/// Renders the border frontally: game image centred, `scale` px per game-height unit. Returns the image and the
/// game-normalised → pixel mapping.
fn render_frontal(layout: &MarkerLayout, scale: f64) -> (Vec<u8>, impl Fn(f64, f64) -> (f64, f64)) {
    let ox = W as f64 / 2.0 - layout.aspect * scale / 2.0;
    let oy = H as f64 / 2.0 - scale / 2.0;
    let aspect = layout.aspect;
    let to_px = move |x: f64, y: f64| (ox + x * aspect * scale, oy + y * scale);
    let mut img = vec![90u8; W * H];
    for m in &layout.markers {
        let cells = wiiu_tracking::tags::render(m.id as usize).unwrap();
        let (x0, y0) = to_px(m.corners[3][0], m.corners[3][1]); // top-left of the black border
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
    (img, to_px)
}

fn detect(img: &[u8]) -> Vec<TagObservation> {
    TagDetector::new().unwrap().detect(img, W, H)
}

#[test]
fn real_detector_finds_every_rendered_tag_with_matching_corner_order() {
    let layout = MarkerLayout::border(&BorderConfig::default());
    let (img, to_px) = render_frontal(&layout, H as f64 * 0.7);
    let obs = detect(&img);
    let mut ids: Vec<u32> = obs.iter().map(|o| o.id).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..layout.markers.len() as u32).collect::<Vec<_>>());
    for o in &obs {
        let m = layout.marker(o.id).unwrap();
        for (i, c) in o.corners.iter().enumerate() {
            let (ex, ey) = to_px(m.corners[i][0], m.corners[i][1]);
            assert!(
                (c[0] - ex).abs() < 1.5 && (c[1] - ey).abs() < 1.5,
                "tag {} corner {i}: detector {c:?} vs expected ({ex:.1}, {ey:.1})",
                o.id
            );
        }
    }
}

#[test]
fn detected_tags_give_the_expected_aim_point() {
    let layout = MarkerLayout::border(&BorderConfig::default());
    let scale = H as f64 * 0.7;
    let (img, to_px) = render_frontal(&layout, scale);
    let obs = detect(&img);
    // Aim at the image centre, which is the game centre in this frontal render.
    let est = estimate_aim(&obs, &layout, &AimConfig::centred(W as f64, H as f64)).unwrap();
    assert!((est.x - 0.5).abs() < 0.01 && (est.y - 0.5).abs() < 0.01, "{est:?}");
    // And a point a quarter of the way in maps to (0.25, 0.25).
    let (qx, qy) = to_px(0.25, 0.25);
    let cfg = AimConfig {
        aim_image_point: [qx, qy],
        ..AimConfig::centred(W as f64, H as f64)
    };
    let est = estimate_aim(&obs, &layout, &cfg).unwrap();
    assert!((est.x - 0.25).abs() < 0.01 && (est.y - 0.25).abs() < 0.01, "{est:?}");
}
