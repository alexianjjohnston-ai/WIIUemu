//! Minimal dense linear algebra for 3×3 homographies and small symmetric eigenproblems.

pub type Vec2 = [f64; 2];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3(pub [[f64; 3]; 3]);

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    pub fn mul(&self, o: &Mat3) -> Mat3 {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Mat3(r)
    }

    pub fn det(&self) -> f64 {
        let m = &self.0;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    pub fn inverse(&self) -> Option<Mat3> {
        let d = self.det();
        if !d.is_finite() || d.abs() < 1e-12 {
            return None;
        }
        let m = &self.0;
        let c = |r0: usize, c0: usize, r1: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
        let adj = [
            [c(1, 1, 2, 2), -c(0, 1, 2, 2), c(0, 1, 1, 2)],
            [-c(1, 0, 2, 2), c(0, 0, 2, 2), -c(0, 0, 1, 2)],
            [c(1, 0, 2, 1), -c(0, 0, 2, 1), c(0, 0, 1, 1)],
        ];
        let mut r = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                r[i][j] = adj[i][j] / d;
            }
        }
        Some(Mat3(r))
    }

    /// Applies the projective transform to a point. `None` if the point maps to infinity.
    pub fn transform(&self, p: Vec2) -> Option<Vec2> {
        let m = &self.0;
        let w = m[2][0] * p[0] + m[2][1] * p[1] + m[2][2];
        if w.abs() < 1e-12 || !w.is_finite() {
            return None;
        }
        Some([
            (m[0][0] * p[0] + m[0][1] * p[1] + m[0][2]) / w,
            (m[1][0] * p[0] + m[1][1] * p[1] + m[1][2]) / w,
        ])
    }
}

/// Eigen-decomposition of a symmetric N×N matrix with the cyclic Jacobi method.
/// Returns `(eigenvalues, eigenvectors as columns)`.
pub fn symmetric_eigen<const N: usize>(mut a: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut v = [[0.0; N]; N];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _sweep in 0..100 {
        let off: f64 = (0..N)
            .flat_map(|i| (0..N).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        if off < 1e-30 {
            break;
        }
        for p in 0..N {
            for q in (p + 1)..N {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                let (rp, rq) = (a[p], a[q]);
                for (k, (apk, aqk)) in rp.iter().zip(rq.iter()).enumerate() {
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let vkp = row[p];
                    let vkq = row[q];
                    row[p] = c * vkp - s * vkq;
                    row[q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut eig = [0.0; N];
    for (i, e) in eig.iter_mut().enumerate() {
        *e = a[i][i];
    }
    (eig, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_round_trips() {
        let m = Mat3([[2.0, 0.5, 1.0], [0.1, 1.5, -2.0], [0.01, 0.02, 1.0]]);
        let i = m.inverse().unwrap();
        let p = m.mul(&i);
        for (r, row) in p.0.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                assert!((v - if r == c { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
        assert!(Mat3([[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [0.0, 0.0, 1.0]])
            .inverse()
            .is_none());
    }

    #[test]
    fn jacobi_finds_eigenpairs() {
        let a = [[4.0, 1.0, 0.0], [1.0, 3.0, 1.0], [0.0, 1.0, 2.0]];
        let (vals, vecs) = symmetric_eigen(a);
        for k in 0..3 {
            for i in 0..3 {
                let av: f64 = (0..3).map(|j| a[i][j] * vecs[j][k]).sum();
                assert!((av - vals[k] * vecs[i][k]).abs() < 1e-9);
            }
        }
    }
}
