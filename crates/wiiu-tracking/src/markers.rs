//! Placement of AprilTag markers in a border around the game image.
//!
//! Coordinates are **game-normalised**: (0, 0) is the top-left of the game image and (1, 1) the bottom-right,
//! the same space as the protocol's `pointer`. Markers sit just outside the game image, so their coordinates
//! are slightly below 0 or above 1.

use crate::linalg::Vec2;

/// Corner order of a tag observation. Matches the AprilTag library's `apriltag_detection_t.p` order
/// (tag frame x right / y down: (-1, 1), (1, 1), (1, -1), (-1, -1)), i.e. bottom-left, bottom-right,
/// top-right, top-left of the upright tag. Verified against the real detector in `tests/detector.rs`.
pub const CORNER_ORDER: [&str; 4] = ["bottom-left", "bottom-right", "top-right", "top-left"];

#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    /// AprilTag id (tag36h11 family).
    pub id: u32,
    /// Corners in game-normalised coordinates, in [`CORNER_ORDER`].
    pub corners: [Vec2; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkerLayout {
    /// Game image width / height (e.g. 16/9).
    pub aspect: f64,
    pub markers: Vec<Marker>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderConfig {
    /// Game image width / height.
    pub aspect: f64,
    /// Tag edge length as a fraction of the game image height.
    pub tag_size: f64,
    /// Gap between game image and tags, as a fraction of the game image height.
    pub gap: f64,
    /// Tags along the top and bottom edges (including corners), ≥ 2.
    pub per_horizontal_edge: usize,
    /// Tags along the left and right edges (including corners), ≥ 2.
    pub per_vertical_edge: usize,
    /// First tag id; ids are assigned consecutively clockwise from the top-left corner.
    pub first_id: u32,
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            aspect: 16.0 / 9.0,
            tag_size: 0.10,
            gap: 0.01,
            per_horizontal_edge: 4,
            per_vertical_edge: 3,
            first_id: 0,
        }
    }
}

impl MarkerLayout {
    /// Lays out square tags clockwise around the game image, starting at the top-left corner.
    pub fn border(cfg: &BorderConfig) -> Self {
        assert!(cfg.per_horizontal_edge >= 2 && cfg.per_vertical_edge >= 2);
        assert!(cfg.aspect > 0.0 && cfg.tag_size > 0.0 && cfg.gap >= 0.0);
        // Square in pixels: height fraction s ⇒ width fraction s / aspect.
        let sy = cfg.tag_size;
        let sx = cfg.tag_size / cfg.aspect;
        let gy = cfg.gap;
        let gx = cfg.gap / cfg.aspect;
        // Tag top-left positions on the ring just outside the game rect.
        let left = -gx - sx;
        let right = 1.0 + gx;
        let top = -gy - sy;
        let bottom = 1.0 + gy;
        let lerp = |a: f64, b: f64, i: usize, n: usize| a + (b - a) * i as f64 / (n - 1) as f64;

        let mut origins: Vec<Vec2> = Vec::new();
        let (h, v) = (cfg.per_horizontal_edge, cfg.per_vertical_edge);
        for i in 0..h {
            origins.push([lerp(left, right, i, h), top]); // top edge, left → right
        }
        for i in 1..v {
            origins.push([right, lerp(top, bottom, i, v)]); // right edge, top → bottom
        }
        for i in (0..h - 1).rev() {
            origins.push([lerp(left, right, i, h), bottom]); // bottom edge, right → left
        }
        for i in (1..v - 1).rev() {
            origins.push([left, lerp(top, bottom, i, v)]); // left edge, bottom → top
        }
        let markers = origins
            .into_iter()
            .enumerate()
            .map(|(i, [x, y])| Marker {
                id: cfg.first_id + i as u32,
                corners: [[x, y + sy], [x + sx, y + sy], [x + sx, y], [x, y]],
            })
            .collect();
        Self {
            aspect: cfg.aspect,
            markers,
        }
    }

    pub fn marker(&self, id: u32) -> Option<&Marker> {
        self.markers.iter().find(|m| m.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_border_has_unique_ids_around_the_game() {
        let l = MarkerLayout::border(&BorderConfig::default());
        // 4 top + 4 bottom + 1 extra per side edge = 10
        assert_eq!(l.markers.len(), 10);
        let mut ids: Vec<u32> = l.markers.iter().map(|m| m.id).collect();
        ids.dedup();
        assert_eq!(ids, (0..10).collect::<Vec<_>>());
        for m in &l.markers {
            // Every tag lies entirely outside the game image.
            let inside = m
                .corners
                .iter()
                .all(|c| (0.0..=1.0).contains(&c[0]) && (0.0..=1.0).contains(&c[1]));
            assert!(!inside, "marker {} overlaps the game", m.id);
            let xs_in = m.corners.iter().any(|c| c[0] > 0.0 && c[0] < 1.0);
            let ys_in = m.corners.iter().any(|c| c[1] > 0.0 && c[1] < 1.0);
            assert!(!(xs_in && ys_in), "marker {} intrudes into the game", m.id);
        }
    }

    #[test]
    fn tags_are_square_in_pixels() {
        let cfg = BorderConfig::default();
        let l = MarkerLayout::border(&cfg);
        for m in &l.markers {
            let w_px = (m.corners[1][0] - m.corners[0][0]) * cfg.aspect;
            let h_px = m.corners[0][1] - m.corners[3][1];
            assert!((w_px - h_px).abs() < 1e-12);
        }
    }

    #[test]
    fn corner_order_is_bl_br_tr_tl() {
        let l = MarkerLayout::border(&BorderConfig::default());
        let [bl, br, tr, tl] = l.markers[0].corners;
        assert!(bl[1] > tl[1] && br[1] > tr[1], "bottom has larger y (y down)");
        assert!(br[0] > bl[0] && tr[0] > tl[0]);
    }
}
