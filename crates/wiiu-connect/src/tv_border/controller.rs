//! When the border should show.

use super::TvBorderMode;
use std::time::{Duration, Instant};

/// In Auto mode the border stays this long after the last camera stops, so a quick reconnect doesn't make the game
/// jump between sizes.
pub const HIDE_DELAY: Duration = Duration::from_secs(3);

pub struct Controller {
    /// When a camera was last on (Auto).
    last_camera: Option<Instant>,
}

impl Controller {
    pub fn new() -> Self {
        Self { last_camera: None }
    }

    /// Whether the border should show now, given the setting and how many Wii Remote cameras are on.
    pub fn want_shown(&mut self, mode: TvBorderMode, cameras: usize, now: Instant) -> bool {
        if cameras > 0 {
            self.last_camera = Some(now);
        }
        match mode {
            TvBorderMode::Off => false,
            TvBorderMode::On => true,
            TvBorderMode::Auto => self
                .last_camera
                .is_some_and(|t| now.saturating_duration_since(t) < HIDE_DELAY),
        }
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_on_ignore_the_cameras() {
        let t = Instant::now();
        let mut c = Controller::new();
        assert!(!c.want_shown(TvBorderMode::Off, 2, t));
        assert!(c.want_shown(TvBorderMode::On, 0, t));
    }

    #[test]
    fn auto_follows_the_cameras_with_a_hide_delay() {
        let t = Instant::now();
        let mut c = Controller::new();
        assert!(!c.want_shown(TvBorderMode::Auto, 0, t));
        assert!(c.want_shown(TvBorderMode::Auto, 1, t));
        assert!(c.want_shown(TvBorderMode::Auto, 0, t + Duration::from_millis(2900)));
        assert!(!c.want_shown(TvBorderMode::Auto, 0, t + HIDE_DELAY));
        // A camera coming back shows it again at once.
        assert!(c.want_shown(TvBorderMode::Auto, 1, t + Duration::from_secs(5)));
    }

    #[test]
    fn a_short_dropout_never_hides_it() {
        let t = Instant::now();
        let mut c = Controller::new();
        assert!(c.want_shown(TvBorderMode::Auto, 1, t));
        for ms in (100..=1000).step_by(100) {
            assert!(c.want_shown(TvBorderMode::Auto, 0, t + Duration::from_millis(ms)));
        }
        assert!(c.want_shown(TvBorderMode::Auto, 1, t + Duration::from_millis(1100)));
        assert!(c.want_shown(TvBorderMode::Auto, 0, t + Duration::from_millis(4000)));
    }
}
