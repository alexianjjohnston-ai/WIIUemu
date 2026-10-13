//! [`Desktop`] on Windows, through `wiiu-desktop-sys`.

use super::cemu::WindowInfo;
use super::driver::Desktop;
use super::recovery::SavedWindow;
use wiiu_desktop_sys::tv::{self, BorderWindow, WindowState};

#[derive(Default)]
pub struct WindowsDesktop {
    border: Option<BorderWindow>,
}

impl WindowsDesktop {
    pub fn new() -> Self {
        tv::make_dpi_aware();
        Self::default()
    }
}

fn state(saved: &SavedWindow) -> WindowState {
    WindowState {
        style: saved.style,
        ex_style: saved.ex_style,
        menu: saved.menu,
        placement: saved.placement,
        show_cmd: saved.show_cmd,
    }
}

impl Desktop for WindowsDesktop {
    fn windows(&self) -> Vec<WindowInfo> {
        tv::top_level_windows()
            .into_iter()
            .map(|w| WindowInfo {
                handle: w.handle,
                pid: w.pid,
                title: w.title,
                exe_name: std::path::Path::new(&w.exe_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            })
            .collect()
    }

    fn process(&self, pid: u32) -> Option<(u32, u64)> {
        tv::process_start_time(pid).map(|start| (pid, start))
    }

    fn owner(&self, handle: u64) -> Option<u32> {
        tv::window_owner_pid(handle)
    }

    fn monitor(&self, handle: u64) -> Option<[i32; 4]> {
        tv::monitor_rect(handle)
    }

    fn save(&self, handle: u64) -> Option<SavedWindow> {
        let pid = tv::window_owner_pid(handle)?;
        let process_start = tv::process_start_time(pid)?;
        let s = tv::save_window(handle)?;
        Some(SavedWindow {
            handle,
            pid,
            process_start,
            style: s.style,
            ex_style: s.ex_style,
            menu: s.menu,
            placement: s.placement,
            show_cmd: s.show_cmd,
        })
    }

    fn menu(&self, handle: u64) -> u64 {
        tv::window_menu(handle)
    }

    fn place(&mut self, handle: u64, client: [i32; 4]) -> bool {
        tv::place_client(handle, client)
    }

    fn keep(&mut self, handle: u64, client: [i32; 4]) -> bool {
        tv::keep_client(handle, client)
    }

    fn restore(&mut self, saved: &SavedWindow) -> bool {
        tv::restore_window(saved.handle, &state(saved))
    }

    fn show_border(&mut self, monitor: [i32; 4], white: Vec<[i32; 4]>) -> bool {
        self.border = BorderWindow::open(monitor, white);
        self.border.is_some()
    }

    fn hide_border(&mut self) {
        self.border = None;
    }
}
