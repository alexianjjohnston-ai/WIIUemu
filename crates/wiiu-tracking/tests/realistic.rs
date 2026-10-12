//! Realistic-camera benchmark: a phone held upright (portrait) at a distance from a monitor showing the
//! `/tracking` border, with perspective, optical blur, sensor noise and JPEG compression. Measures how many tags
//! survive and how much the computed pointer jitters, for different frame sizes and detector settings.
//!
//! Run: `cargo test -p wiiu-tracking --features detector --release --test realistic -- --ignored --nocapture`
#![cfg(feature = "detector")]

use wiiu_tracking::detect::{decode_jpeg_gray, TagDetector};
use wiiu_tracking::{estimate_aim, AimConfig, BorderConfig, MarkerLayout};

/// Physical width of the game area on the monitor (≈ 27" monitor showing `/tracking` fullscreen).
const GAME_WIDTH_M: f64 = 0.50;
/// Phone main camera, 16:9 video: horizontal FOV of the *long* side.
const LONG_SIDE_FOV_DEG: f64 = 65.0;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        let (u, v) = (self.next().max(1e-12), self.next());
        (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
    }
}

/// Renders what a portrait camera (`w`×`h`, h > w) sees at `dist_m` from the screen centre, with a small yaw/pitch.
fn render(layout: &MarkerLayout, w: usize, h: usize, dist_m: f64, yaw: f64, pitch: f64, rng: &mut Rng) -> Vec<u8> {
    let tags: Vec<_> = layout
        .markers
        .iter()
        .map(|m| (m, wiiu_tracking::tags::render(m.id as usize).unwrap()))
        .collect();
    let game_h_m = GAME_WIDTH_M / layout.aspect;
    let f = (h as f64 / 2.0) / (LONG_SIDE_FOV_DEG.to_radians() / 2.0).tan();
    let (cy_, sy_) = (yaw.cos(), yaw.sin());
    let (cp, sp) = (pitch.cos(), pitch.sin());
    // Supersample 2×2 for anti-aliasing (the "optical blur").
    let mut img = vec![0f64; w * h];
    for py in 0..h {
        for px in 0..w {
            let mut acc = 0.0;
            for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                // Ray in camera frame (x right, y down, z forward), rotated by pitch then yaw.
                let (x, y, z) = (
                    (px as f64 + ox - w as f64 / 2.0) / f,
                    (py as f64 + oy - h as f64 / 2.0) / f,
                    1.0,
                );
                let (y, z) = (y * cp - z * sp, y * sp + z * cp);
                let (x, z) = (x * cy_ + z * sy_, -x * sy_ + z * cy_);
                let t = dist_m / z;
                let (gx, gy) = (x * t / GAME_WIDTH_M + 0.5, y * t / game_h_m + 0.5);
                let mut v = if (0.0..=1.0).contains(&gx) && (0.0..=1.0).contains(&gy) {
                    40.0
                } else {
                    18.0
                };
                if !(-0.3..=1.3).contains(&gx) || !(-0.4..=1.4).contains(&gy) {
                    v = 110.0; // the room around the monitor
                }
                for (m, cells) in &tags {
                    let [x0, y0] = m.corners[3]; // top-left
                    let [x1, y1] = m.corners[1]; // bottom-right
                    let cx = ((gx - x0) / (x1 - x0) * 8.0 + 1.0).floor();
                    let cyy = ((gy - y0) / (y1 - y0) * 8.0 + 1.0).floor();
                    if (0.0..10.0).contains(&cx) && (0.0..10.0).contains(&cyy) {
                        v = if cells[cyy as usize][cx as usize] { 235.0 } else { 15.0 };
                    }
                }
                acc += v;
            }
            img[py * w + px] = acc / 4.0;
        }
    }
    // Slight extra blur (lens + small hand motion), then sensor noise.
    let mut out = vec![0u8; w * h];
    for py in 0..h {
        for px in 0..w {
            let mut s = 0.0;
            let mut n = 0.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (x, y) = (px as i32 + dx, py as i32 + dy);
                    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        let k = if dx == 0 && dy == 0 { 4.0 } else { 1.0 };
                        s += k * img[y as usize * w + x as usize];
                        n += k;
                    }
                }
            }
            out[py * w + px] = (s / n + 5.0 * rng.gauss()).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn jpeg(gray: &[u8], w: usize, h: usize, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(gray, w as u16, h as u16, jpeg_encoder::ColorType::Luma)
        .unwrap();
    out
}

struct Stats {
    tags_mean: f64,
    fix_rate: f64,
    jitter_pct: f64,
    bytes: usize,
    ms: f64,
}

fn run(layout: &MarkerLayout, w: usize, h: usize, decimate: f32, threads: u8, dist_m: f64, trials: usize) -> Stats {
    let mut det = TagDetector::with_options(decimate, threads).unwrap();
    let aim = AimConfig::centred(w as f64, h as f64);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (dist_m * 1000.0) as u64);
    let (mut tags, mut fixes, mut xs, mut ys, mut bytes, mut ms) = (0usize, 0usize, vec![], vec![], 0, 0.0);
    for _ in 0..trials {
        // Same pose each trial (phone held still): jitter = pure measurement noise.
        let gray = render(layout, w, h, dist_m, 0.02, -0.01, &mut rng);
        let data = jpeg(&gray, w, h, 60);
        bytes = bytes.max(data.len());
        let t0 = std::time::Instant::now();
        let gray = decode_jpeg_gray(&data, (w as u16, h as u16)).unwrap();
        let obs = det.detect(&gray, w, h);
        ms += t0.elapsed().as_secs_f64() * 1000.0;
        tags += obs.len();
        if let Ok(est) = estimate_aim(&obs, layout, &aim) {
            fixes += 1;
            xs.push(est.x);
            ys.push(est.y);
        }
    }
    let sd = |v: &[f64]| {
        if v.len() < 2 {
            return f64::NAN;
        }
        let m = v.iter().sum::<f64>() / v.len() as f64;
        (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    };
    Stats {
        tags_mean: tags as f64 / trials as f64,
        fix_rate: fixes as f64 / trials as f64,
        jitter_pct: 100.0 * sd(&xs).max(sd(&ys)),
        bytes,
        ms: ms / trials as f64,
    }
}

#[test]
#[ignore = "benchmark; run manually with --release --ignored --nocapture"]
fn realistic_portrait_camera_benchmark() {
    let big = BorderConfig {
        tag_size: 0.16,
        ..BorderConfig::default()
    };
    let configs: Vec<(&str, BorderConfig, usize, usize, f32, u8)> = match std::env::var("BENCH_SET").as_deref() {
        Ok("threads") => vec![
            ("540x960, decimate 1, 4 threads, tags 0.16", big, 540, 960, 1.0, 4),
            ("540x960, decimate 1.5, 4 threads, tags 0.16", big, 540, 960, 1.5, 4),
            (
                "540x960, decimate 1.5, 4 threads, tags 0.20",
                BorderConfig { tag_size: 0.20, ..big },
                540,
                960,
                1.5,
                4,
            ),
            ("720x1280, decimate 2, 4 threads, tags 0.16", big, 720, 1280, 2.0, 4),
        ],
        _ => vec![
            (
                "before: 360x640, decimate 2, tags 0.10",
                BorderConfig::default(),
                360,
                640,
                2.0,
                1,
            ),
            (
                "360x640, decimate 1, tags 0.10",
                BorderConfig::default(),
                360,
                640,
                1.0,
                1,
            ),
            (
                "540x960, decimate 1, tags 0.10",
                BorderConfig::default(),
                540,
                960,
                1.0,
                1,
            ),
            ("360x640, decimate 1, tags 0.16", big, 360, 640, 1.0, 1),
            ("540x960, decimate 1, tags 0.16", big, 540, 960, 1.0, 1),
        ],
    };
    for (name, cfg, w, h, dec, threads) in configs {
        let layout = MarkerLayout::border(&cfg);
        println!("\n{name}");
        for dist in [0.8, 1.2, 1.8, 2.5] {
            let s = run(&layout, w, h, dec, threads, dist, 20);
            println!(
                "  {dist:>3.1} m: tags {:>4.1}/10  fixes {:>3.0}%  jitter {:>6.3}% of screen  jpeg {:>5} B  decode+detect {:>5.1} ms",
                s.tags_mean,
                s.fix_rate * 100.0,
                s.jitter_pct,
                s.bytes,
                s.ms
            );
        }
    }
}
