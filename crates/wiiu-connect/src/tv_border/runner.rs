//! The `tv-border` thread: every tick, count the cameras, decide, and let the driver apply it.

use super::controller::Controller;
use super::driver::{Desktop, Driver, TvStatus};
use super::TvBorderMode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often the thread re-checks the cameras and Cemu's window.
pub const TICK: Duration = Duration::from_millis(250);

/// State shared with the setup page and the tracking workers.
#[derive(Debug)]
pub struct TvBorder {
    mode: Mutex<TvBorderMode>,
    status: Mutex<TvStatus>,
    /// True while the border is around the game (phones show a matching calibration hint).
    on: AtomicBool,
}

impl TvBorder {
    pub fn new(mode: TvBorderMode, status: TvStatus) -> Arc<Self> {
        Arc::new(Self {
            mode: Mutex::new(mode),
            status: Mutex::new(status),
            on: AtomicBool::new(false),
        })
    }

    pub fn mode(&self) -> TvBorderMode {
        *self.mode.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_mode(&self, mode: TvBorderMode) {
        *self.mode.lock().unwrap_or_else(|e| e.into_inner()) = mode;
    }

    pub fn status(&self) -> TvStatus {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// No TV border on this platform yet.
    pub fn report_unsupported(&self) {
        self.report(TvStatus::Unsupported);
    }

    fn report(&self, status: TvStatus) {
        self.on.store(matches!(status, TvStatus::On { .. }), Ordering::Relaxed);
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }
}

/// Runs until `stop` turns true, then undoes the border and returns the driver.
pub fn run<D: Desktop>(
    mut driver: Driver<D>,
    shared: Arc<TvBorder>,
    cameras: impl Fn(Instant) -> usize,
    stop: tokio::sync::watch::Receiver<bool>,
    tick: Duration,
) -> Driver<D> {
    driver.recover();
    let mut controller = Controller::new();
    while !*stop.borrow() {
        let now = Instant::now();
        let want = controller.want_shown(shared.mode(), cameras(now), now);
        shared.report(driver.tick(want));
        std::thread::sleep(tick);
    }
    driver.shutdown();
    shared.report(TvStatus::Off);
    driver
}

#[cfg(test)]
mod tests {
    use super::super::fake::{record_path, Fake};
    use super::*;

    fn wait_for(shared: &TvBorder, pred: impl Fn(&TvStatus) -> bool) -> TvStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let s = shared.status();
            if pred(&s) || Instant::now() > deadline {
                return s;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn follows_the_cameras_and_restores_cemu_on_stop() {
        let path = record_path("runner");
        let shared = TvBorder::new(TvBorderMode::Auto, TvStatus::Off);
        let cameras = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let thread = {
            let (shared, cameras) = (shared.clone(), cameras.clone());
            let driver = Driver::new(Fake::with_cemu(path.clone()), path.clone());
            std::thread::spawn(move || {
                run(
                    driver,
                    shared,
                    move |_| cameras.load(Ordering::Relaxed),
                    stop_rx,
                    Duration::from_millis(5),
                )
            })
        };
        assert_eq!(wait_for(&shared, |s| *s == TvStatus::Off), TvStatus::Off);
        assert!(!shared.is_on());
        cameras.store(1, Ordering::Relaxed);
        assert!(matches!(
            wait_for(&shared, |s| matches!(s, TvStatus::On { .. })),
            TvStatus::On { .. }
        ));
        assert!(shared.is_on());
        // The setup switch wins over the cameras.
        shared.set_mode(TvBorderMode::Off);
        assert_eq!(wait_for(&shared, |s| *s == TvStatus::Off), TvStatus::Off);
        shared.set_mode(TvBorderMode::On);
        assert!(matches!(
            wait_for(&shared, |s| matches!(s, TvStatus::On { .. })),
            TvStatus::On { .. }
        ));
        stop_tx.send(true).unwrap();
        let driver = thread.join().unwrap();
        assert_eq!(
            driver.into_desktop().restored.len(),
            2,
            "restored when switched off and on stop"
        );
        assert!(!shared.is_on());
        assert_eq!(shared.status(), TvStatus::Off);
        assert!(!path.exists());
    }
}
