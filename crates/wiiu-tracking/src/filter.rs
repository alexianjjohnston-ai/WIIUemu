//! One Euro filter (Casiez et al., CHI 2012): a low-pass whose cutoff rises with speed. It removes jitter while
//! the pointer is nearly still, and adds almost no lag during fast moves.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OneEuroConfig {
    /// Cutoff (Hz) at rest. Lower = smoother but laggier when moving slowly.
    pub min_cutoff_hz: f64,
    /// How fast the cutoff rises with speed (per screen-width/s).
    pub beta: f64,
    /// Cutoff for the speed estimate itself.
    pub d_cutoff_hz: f64,
}

impl Default for OneEuroConfig {
    /// Tuned for a pointer in game-normalised units (0..1 across the screen).
    fn default() -> Self {
        Self {
            min_cutoff_hz: 1.2,
            beta: 8.0,
            d_cutoff_hz: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OneEuro2 {
    cfg: OneEuroConfig,
    prev: Option<(f64, [f64; 2], [f64; 2])>,
}

fn alpha(cutoff_hz: f64, dt: f64) -> f64 {
    let tau = 1.0 / (2.0 * std::f64::consts::PI * cutoff_hz);
    1.0 / (1.0 + tau / dt)
}

impl OneEuro2 {
    pub fn new(cfg: OneEuroConfig) -> Self {
        Self { cfg, prev: None }
    }

    /// Forget the history (e.g. tracking was lost), so the next value is passed through unfiltered.
    pub fn reset(&mut self) {
        self.prev = None;
    }

    /// Filters a 2D value observed at time `t` (seconds, increasing).
    pub fn filter(&mut self, t: f64, x: [f64; 2]) -> [f64; 2] {
        let Some((t0, x0, dx0)) = self.prev else {
            self.prev = Some((t, x, [0.0; 2]));
            return x;
        };
        let dt = t - t0;
        if dt.is_nan() || dt <= 1e-6 {
            return x0;
        }
        let ad = alpha(self.cfg.d_cutoff_hz, dt);
        let dx: [f64; 2] = std::array::from_fn(|i| ad * (x[i] - x0[i]) / dt + (1.0 - ad) * dx0[i]);
        let speed = dx[0].hypot(dx[1]);
        let a = alpha(self.cfg.min_cutoff_hz + self.cfg.beta * speed, dt);
        let out: [f64; 2] = std::array::from_fn(|i| a * x[i] + (1.0 - a) * x0[i]);
        self.prev = Some((t, out, dx));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_jitter_at_rest_and_follows_fast_moves() {
        let mut f = OneEuro2::new(OneEuroConfig::default());
        // Still pointer at 0.5 with ±0.5% alternating noise at 30 Hz.
        let mut max_dev: f64 = 0.0;
        for i in 0..60 {
            let n = if i % 2 == 0 { 0.005 } else { -0.005 };
            let y = f.filter(i as f64 / 30.0, [0.5 + n, 0.5]);
            if i > 10 {
                max_dev = max_dev.max((y[0] - 0.5).abs());
            }
        }
        assert!(max_dev < 0.002, "jitter {max_dev}");
        // Fast sweep of 2 screens/s: lag stays small.
        let t0 = 2.0;
        let mut last = [0.5, 0.5];
        for i in 0..15 {
            let t = t0 + i as f64 / 30.0;
            last = f.filter(t, [0.5 + 2.0 * (t - t0), 0.5]);
        }
        let truth = 0.5 + 2.0 * 14.0 / 30.0;
        assert!((truth - last[0]).abs() < 0.06, "lag {}", truth - last[0]);
    }

    #[test]
    fn reset_passes_the_next_value_through() {
        let mut f = OneEuro2::new(OneEuroConfig::default());
        f.filter(0.0, [0.0, 0.0]);
        f.reset();
        assert_eq!(f.filter(0.1, [0.9, 0.9]), [0.9, 0.9]);
    }
}
