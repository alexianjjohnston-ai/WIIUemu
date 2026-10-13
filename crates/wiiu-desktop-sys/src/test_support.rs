//! Test-only helpers: a stand-in "Cemu" window (title bar, resizable frame, menu) on its own message-loop thread.

use std::sync::mpsc;
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMenu, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, PostMessageW, PostQuitMessage, RegisterClassW, ShowWindow, TranslateMessage,
    GWL_STYLE, MF_STRING, MSG, SW_SHOWNOACTIVATE, WM_CLOSE, WM_DESTROY, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            // SAFETY: destroying our own window on its own thread.
            unsafe { DestroyWindow(h) };
            0
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: default handling with the arguments we were given.
        _ => unsafe { DefWindowProcW(h, msg, wparam, lparam) },
    }
}

/// A normal top-level window with a caption, a thick frame and a menu, like Cemu's main window.
pub struct StandIn {
    handle: u64,
    thread: Option<JoinHandle<()>>,
}

impl StandIn {
    pub fn new(title: &str, rect: [i32; 4]) -> StandIn {
        let title = title.to_string();
        let (tx, rx) = mpsc::channel::<u64>();
        let thread = std::thread::spawn(move || {
            let class = wide("WiiUConnectTestStandIn");
            let title = wide(&title);
            let item = wide("&File");
            // SAFETY: buffers outlive the calls; standard window creation and message loop on this thread.
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let mut wc: WNDCLASSW = std::mem::zeroed();
                wc.lpfnWndProc = Some(proc);
                wc.hInstance = instance;
                wc.lpszClassName = class.as_ptr();
                RegisterClassW(&wc);
                let menu = CreateMenu();
                AppendMenuW(menu, MF_STRING, 1, item.as_ptr());
                let h = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW,
                    rect[0],
                    rect[1],
                    rect[2] - rect[0],
                    rect[3] - rect[1],
                    std::ptr::null_mut(),
                    menu,
                    instance,
                    std::ptr::null(),
                );
                ShowWindow(h, SW_SHOWNOACTIVATE);
                let _ = tx.send(h as usize as u64);
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
        let handle = rx.recv().expect("stand-in window created");
        assert_ne!(handle, 0, "CreateWindowExW failed");
        StandIn {
            handle,
            thread: Some(thread),
        }
    }

    pub fn handle(&self) -> u64 {
        self.handle
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        // SAFETY: posting to our own window; its thread destroys it and leaves the loop.
        unsafe {
            PostMessageW(self.handle as usize as HWND, WM_CLOSE, 0, 0);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn menu_of(handle: u64) -> u64 {
    // SAFETY: plain query.
    unsafe { GetMenu(handle as usize as HWND) as usize as u64 }
}

pub fn style_of(handle: u64) -> i64 {
    // SAFETY: plain query.
    unsafe { GetWindowLongPtrW(handle as usize as HWND, GWL_STYLE) as i64 }
}

pub fn window_rect(handle: u64) -> [i32; 4] {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: local out-parameter.
    unsafe { GetWindowRect(handle as usize as HWND, &mut r) };
    [r.left, r.top, r.right, r.bottom]
}
