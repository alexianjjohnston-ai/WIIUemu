//! Rendering of `tag36h11` markers as cell grids.

use crate::tag36h11::{BIT_X, BIT_Y, CODES, NBITS, TOTAL_WIDTH, WIDTH_AT_BORDER};

pub const TAG_COUNT: usize = CODES.len();

/// `TOTAL_WIDTH × TOTAL_WIDTH` cells, `true` = white. Row-major, row 0 at the top.
pub type TagCells = [[bool; TOTAL_WIDTH]; TOTAL_WIDTH];

/// Renders tag `id` exactly like the AprilTag library's `apriltag_to_image`: a 1-cell white quiet zone, a
/// 1-cell black border, and 6×6 data cells. Detectors report the corners of the black border, which spans
/// cells 1..=8, i.e. the inner `WIDTH_AT_BORDER` cells.
pub fn render(id: usize) -> Option<TagCells> {
    let code = *CODES.get(id)?;
    let mut cells = [[false; TOTAL_WIDTH]; TOTAL_WIDTH];
    let last = TOTAL_WIDTH - 1;
    for (y, row) in cells.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            *cell = x == 0 || y == 0 || x == last || y == last;
        }
    }
    let border_start = (TOTAL_WIDTH - WIDTH_AT_BORDER) / 2;
    for i in 0..NBITS {
        if code & (1u64 << (NBITS - i - 1)) != 0 {
            cells[BIT_Y[i] as usize + border_start][BIT_X[i] as usize + border_start] = true;
        }
    }
    Some(cells)
}

/// The tag as a string of `TOTAL_WIDTH` rows of `'1'` (white) / `'0'` (black), joined by `'/'`. Compact form for
/// JSON and test fixtures.
pub fn render_string(id: usize) -> Option<String> {
    let cells = render(id)?;
    Some(
        cells
            .iter()
            .map(|row| row.iter().map(|&w| if w { '1' } else { '0' }).collect::<String>())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag36h11::MIN_HAMMING;

    /// Rotate a 6×6 code by 90° using the library's bit layout.
    fn rotate90(code: u64) -> u64 {
        // Map each bit position (x, y) in 1..=6 to (7 - y, x) — the same rotation the library uses when matching.
        let mut out = 0u64;
        for i in 0..NBITS {
            if code & (1u64 << (NBITS - i - 1)) != 0 {
                let (x, y) = (BIT_X[i], BIT_Y[i]);
                let (nx, ny) = (7 - y, x);
                let j = (0..NBITS).find(|&j| BIT_X[j] == nx && BIT_Y[j] == ny).unwrap();
                out |= 1u64 << (NBITS - j - 1);
            }
        }
        out
    }

    #[test]
    fn table_is_intact() {
        assert_eq!(CODES.len(), 587);
        assert_eq!(CODES[0], 0x0000_000d_7e00_984b);
        assert_eq!(CODES[586], 0x0000_000e_8b77_2fe0);
        // Bit positions cover the 6×6 data area exactly once.
        let mut seen = [[false; 6]; 6];
        for i in 0..NBITS {
            let (x, y) = (BIT_X[i] as usize - 1, BIT_Y[i] as usize - 1);
            assert!(!seen[y][x]);
            seen[y][x] = true;
        }
    }

    #[test]
    fn rotation_is_a_permutation_of_order_four() {
        for &c in CODES.iter().take(20) {
            assert_eq!(rotate90(rotate90(rotate90(rotate90(c)))), c);
            assert_eq!(rotate90(c).count_ones(), c.count_ones());
        }
    }

    #[test]
    fn codes_keep_minimum_hamming_distance_under_rotation() {
        // A corrupted table entry would almost certainly break the family's distance guarantee.
        // Checking the first 120 codes against all 587 keeps the test fast while covering every entry.
        let rotations: Vec<[u64; 4]> = CODES
            .iter()
            .map(|&c| {
                let r1 = rotate90(c);
                let r2 = rotate90(r1);
                [c, r1, r2, rotate90(r2)]
            })
            .collect();
        for (i, a) in CODES.iter().enumerate().take(120) {
            for (j, rb) in rotations.iter().enumerate() {
                for (k, &b) in rb.iter().enumerate() {
                    if i == j && k == 0 {
                        continue;
                    }
                    let d = (a ^ b).count_ones();
                    assert!(d >= MIN_HAMMING, "codes {i} and {j} (rot {k}) differ in only {d} bits");
                }
            }
        }
    }

    #[test]
    #[allow(clippy::needless_range_loop)] // indexes rows and columns symmetrically
    fn render_has_quiet_zone_and_black_border() {
        let cells = render(0).unwrap();
        for i in 0..TOTAL_WIDTH {
            assert!(
                cells[0][i] && cells[9][i] && cells[i][0] && cells[i][9],
                "white quiet zone"
            );
        }
        for i in 1..9 {
            assert!(
                !cells[1][i] && !cells[8][i] && !cells[i][1] && !cells[i][8],
                "black border"
            );
        }
        assert!(render(587).is_none());
    }
}
