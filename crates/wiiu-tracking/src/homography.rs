//! Plane-to-plane homography estimation (normalised DLT, Hartley & Zisserman alg. 4.2).

use crate::linalg::{symmetric_eigen, Mat3, Vec2};

/// Similarity transform that moves the centroid to the origin and scales mean distance to √2.
fn normalizer(points: &[Vec2]) -> Option<Mat3> {
    let n = points.len() as f64;
    let cx = points.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = points.iter().map(|p| p[1]).sum::<f64>() / n;
    let mean_dist = points.iter().map(|p| (p[0] - cx).hypot(p[1] - cy)).sum::<f64>() / n;
    if !mean_dist.is_finite() || mean_dist < 1e-12 {
        return None;
    }
    let s = std::f64::consts::SQRT_2 / mean_dist;
    Some(Mat3([[s, 0.0, -s * cx], [0.0, s, -s * cy], [0.0, 0.0, 1.0]]))
}

/// Estimates `H` with `dst ≈ H(src)` from ≥ 4 correspondences (least squares for > 4).
/// Returns `None` for degenerate input (too few points, collinear points, non-finite values).
pub fn estimate(src: &[Vec2], dst: &[Vec2]) -> Option<Mat3> {
    if src.len() != dst.len() || src.len() < 4 {
        return None;
    }
    if src.iter().chain(dst).any(|p| !p[0].is_finite() || !p[1].is_finite()) {
        return None;
    }
    let ts = normalizer(src)?;
    let td = normalizer(dst)?;

    // Accumulate AᵀA directly (9×9) instead of storing the 2N×9 matrix.
    let mut ata = [[0.0f64; 9]; 9];
    for (s, d) in src.iter().zip(dst) {
        let [x, y] = ts.transform(*s)?;
        let [u, v] = td.transform(*d)?;
        let rows = [
            [-x, -y, -1.0, 0.0, 0.0, 0.0, u * x, u * y, u],
            [0.0, 0.0, 0.0, -x, -y, -1.0, v * x, v * y, v],
        ];
        for r in &rows {
            for i in 0..9 {
                for j in 0..9 {
                    ata[i][j] += r[i] * r[j];
                }
            }
        }
    }
    let (vals, vecs) = symmetric_eigen(ata);
    let (min_idx, _) = vals.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1))?;
    // Degeneracy check: a well-posed problem has exactly one (near-)zero eigenvalue.
    let mut sorted = vals;
    sorted.sort_by(f64::total_cmp);
    if sorted[1] < 1e-9 * sorted[8].max(1e-300) {
        return None;
    }
    let h: Vec<f64> = (0..9).map(|i| vecs[i][min_idx]).collect();
    let hn = Mat3([[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], h[8]]]);
    let h = td.inverse()?.mul(&hn).mul(&ts);
    // Normalise so h33 = 1 when possible (purely cosmetic, keeps numbers readable).
    let scale = if h.0[2][2].abs() > 1e-12 { h.0[2][2] } else { 1.0 };
    let mut out = h.0;
    for row in out.iter_mut() {
        for v in row.iter_mut() {
            *v /= scale;
        }
    }
    Some(Mat3(out))
}

/// Root-mean-square distance between `H(src)` and `dst`.
pub fn rms_error(h: &Mat3, src: &[Vec2], dst: &[Vec2]) -> f64 {
    let sum: f64 = src
        .iter()
        .zip(dst)
        .map(|(s, d)| match h.transform(*s) {
            Some(p) => (p[0] - d[0]).powi(2) + (p[1] - d[1]).powi(2),
            None => f64::INFINITY,
        })
        .sum();
    (sum / src.len().max(1) as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_known_homography_exactly() {
        let h = Mat3([[1.2, 0.1, 30.0], [-0.05, 0.9, 12.0], [0.0004, -0.0002, 1.0]]);
        let src: Vec<Vec2> = [
            [0.0, 0.0],
            [640.0, 0.0],
            [640.0, 360.0],
            [0.0, 360.0],
            [320.0, 100.0],
            [50.0, 300.0],
        ]
        .to_vec();
        let dst: Vec<Vec2> = src.iter().map(|p| h.transform(*p).unwrap()).collect();
        let est = estimate(&src, &dst).unwrap();
        for p in [[100.0, 200.0], [600.0, 20.0], [320.0, 180.0]] {
            let a = h.transform(p).unwrap();
            let b = est.transform(p).unwrap();
            assert!(
                (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6,
                "{a:?} vs {b:?}"
            );
        }
        assert!(rms_error(&est, &src, &dst) < 1e-6);
    }

    #[test]
    fn rejects_degenerate_input() {
        let line: Vec<Vec2> = (0..6).map(|i| [i as f64, 2.0 * i as f64]).collect();
        let other: Vec<Vec2> = (0..6).map(|i| [i as f64, (i * i) as f64]).collect();
        assert!(estimate(&line, &other).is_none(), "collinear source points");
        assert!(estimate(&line[..3], &other[..3]).is_none(), "too few points");
        let mut bad = other.clone();
        bad[0][0] = f64::NAN;
        assert!(estimate(&other, &bad).is_none(), "NaN");
        let same = vec![[1.0, 1.0]; 5];
        assert!(estimate(&same, &same).is_none(), "coincident points");
    }
}
