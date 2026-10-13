//! Where the game and the tags go on a real screen, in pixels: the same math as `web/tracking.html`, so the TV
//! border window (Windows) and the browser page draw identical borders.

use crate::markers::MarkerLayout;
use crate::tags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    /// Exclusive.
    pub right: i32,
    /// Exclusive.
    pub bottom: i32,
}

impl PixelRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenLayout {
    /// The game image (Cemu's client area goes here).
    pub game: PixelRect,
    /// White tag cells (quiet zones and data bits); everything else on the screen is black.
    pub white: Vec<PixelRect>,
}

/// Pixel layout for a `width` × `height` screen; coordinates relative to its top-left. `None` for an empty screen.
pub fn screen_layout(width: u32, height: u32, layout: &MarkerLayout) -> Option<ScreenLayout> {
    if width == 0 || height == 0 || layout.markers.is_empty() {
        return None;
    }
    let (w, h) = (f64::from(width), f64::from(height));
    // Bounding box of all tags incl. their 1-cell quiet zone, in game-height units (x scaled by aspect).
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for m in &layout.markers {
        let [bl, br, _, tl] = m.corners;
        let cell = (bl[1] - tl[1]) / 8.0;
        min_x = min_x.min(tl[0] * layout.aspect - cell);
        max_x = max_x.max(br[0] * layout.aspect + cell);
        min_y = min_y.min(tl[1] - cell);
        max_y = max_y.max(bl[1] + cell);
    }
    let margin = 0.02 * w.min(h);
    let s = ((w - 2.0 * margin) / (max_x - min_x)).min((h - 2.0 * margin) / (max_y - min_y));
    let ox = (w - (max_x - min_x) * s) / 2.0 - min_x * s;
    let oy = (h - (max_y - min_y) * s) / 2.0 - min_y * s;
    let px = |x: f64, y: f64| (ox + x * layout.aspect * s, oy + y * s);
    let r = |v: f64| v.round() as i32;
    let (gx0, gy0) = px(0.0, 0.0);
    let (gx1, gy1) = px(1.0, 1.0);
    let game = PixelRect {
        left: r(gx0),
        top: r(gy0),
        right: r(gx1),
        bottom: r(gy1),
    };
    let mut white = Vec::new();
    for m in &layout.markers {
        let cells = tags::render(m.id as usize)?;
        let tl = m.corners[3];
        let (bx, by) = px(tl[0], tl[1]);
        let (ex, _) = px(m.corners[1][0], 0.0);
        let cell = (ex - bx) / 8.0;
        for (row, line) in cells.iter().enumerate() {
            for (col, &is_white) in line.iter().enumerate() {
                if is_white {
                    let (c, rr) = (col as f64, row as f64);
                    white.push(PixelRect {
                        left: r(bx + (c - 1.0) * cell),
                        top: r(by + (rr - 1.0) * cell),
                        right: r(bx + c * cell),
                        bottom: r(by + rr * cell),
                    });
                }
            }
        }
    }
    Some(ScreenLayout { game, white })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markers::BorderConfig;

    const SCREENS: [(u32, u32); 6] = [
        (1280, 720),
        (1920, 1080),
        (2560, 1440),
        (3840, 2160),
        (2560, 1080),
        (1024, 768),
    ];

    fn layout() -> MarkerLayout {
        MarkerLayout::border(&BorderConfig::default())
    }

    fn overlaps(a: &PixelRect, b: &PixelRect) -> bool {
        a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
    }

    #[test]
    fn game_is_16_9_centred_and_on_screen() {
        for (w, h) in SCREENS {
            let s = screen_layout(w, h, &layout()).expect("layout");
            let g = s.game;
            assert!(
                g.left >= 0 && g.top >= 0 && g.right <= w as i32 && g.bottom <= h as i32,
                "{w}x{h}: {g:?}"
            );
            let ideal_w = g.height() as f64 * 16.0 / 9.0;
            assert!((g.width() as f64 - ideal_w).abs() <= 1.5, "{w}x{h}: {g:?} is not 16:9");
            assert!(
                (g.left - (w as i32 - g.right)).abs() <= 1,
                "{w}x{h}: not centred horizontally"
            );
            assert!(
                (g.top - (h as i32 - g.bottom)).abs() <= 1,
                "{w}x{h}: not centred vertically"
            );
        }
    }

    #[test]
    fn white_cells_stay_on_screen_and_off_the_game() {
        for (w, h) in SCREENS {
            let s = screen_layout(w, h, &layout()).expect("layout");
            assert!(!s.white.is_empty());
            for c in &s.white {
                assert!(
                    c.left >= 0 && c.top >= 0 && c.right <= w as i32 && c.bottom <= h as i32,
                    "{w}x{h}: {c:?}"
                );
                assert!(c.width() > 0 && c.height() > 0, "{w}x{h}: empty cell {c:?}");
                assert!(
                    !overlaps(c, &s.game),
                    "{w}x{h}: cell {c:?} covers the game {:?}",
                    s.game
                );
            }
        }
    }

    #[test]
    fn full_hd_matches_the_tracking_page() {
        // web/tracking.html on a 1920×1080 canvas: margin 21.6 px; gap 0.03 + tag 0.20 + quiet zone 0.025 on each
        // side, so the border spans 1.51 game heights and the scale is (1080 − 43.2) / 1.51 ≈ 686.6 px.
        let g = screen_layout(1920, 1080, &layout()).unwrap().game;
        assert!((g.width() - 1221).abs() <= 1 && (g.height() - 687).abs() <= 1, "{g:?}");
    }

    #[test]
    fn quiet_zone_starts_one_cell_outside_the_marker_corners() {
        let l = layout();
        let s = screen_layout(1920, 1080, &l).unwrap();
        let g = s.game;
        let scale = g.height() as f64; // pixels per game height
        let m = l.marker(0).unwrap(); // top-left tag
        let tl = m.corners[3];
        let cell = (m.corners[0][1] - tl[1]) / 8.0 * scale;
        let x = g.left as f64 + tl[0] * l.aspect * scale - cell;
        let y = g.top as f64 + tl[1] * scale - cell;
        // The tag's top-left quiet-zone cell is white and sits there (± rounding).
        assert!(
            s.white
                .iter()
                .any(|c| (c.left as f64 - x).abs() <= 2.0 && (c.top as f64 - y).abs() <= 2.0),
            "no white cell near ({x:.1}, {y:.1})"
        );
    }

    #[test]
    fn empty_screens_have_no_layout() {
        assert!(screen_layout(0, 1080, &layout()).is_none());
        assert!(screen_layout(1920, 0, &layout()).is_none());
    }
}
