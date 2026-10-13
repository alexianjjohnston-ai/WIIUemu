//! The TV border (ADR-0004): finding Cemu's windows, the black border window with the tags, and moving Cemu's
//! window into it and back.

use std::sync::mpsc;
use std::sync::Mutex;
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{CloseHandle, FALSE, FILETIME, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, EndPaint, FillRect, GetMonitorInfoW, GetStockObject, MonitorFromWindow, BLACK_BRUSH,
    HBRUSH, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, WHITE_BRUSH,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, GetClientRect,
    GetMenu, GetMessageW, GetWindowLongPtrW, GetWindowPlacement, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsHungAppWindow, IsIconic, IsMenu, IsWindow, IsWindowVisible, IsZoomed, PostMessageW,
    PostQuitMessage, RegisterClassW, SetMenu, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow,
    TranslateMessage, GWL_EXSTYLE, GWL_STYLE, HMENU, HWND_TOP, MSG, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, SW_SHOWNORMAL, WINDOWPLACEMENT, WM_CLOSE, WM_DESTROY,
    WM_ERASEBKGND, WM_PAINT, WNDCLASSW, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_THICKFRAME,
    WS_VISIBLE,
};

fn hwnd(handle: u64) -> HWND {
    handle as usize as HWND
}

fn to_rect(r: [i32; 4]) -> RECT {
    RECT {
        left: r[0],
        top: r[1],
        right: r[2],
        bottom: r[3],
    }
}

fn from_rect(r: &RECT) -> [i32; 4] {
    [r.left, r.top, r.right, r.bottom]
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Window coordinates in physical pixels on every monitor. The exe's manifest already asks for this; tests and
/// other hosts call it explicitly. Failure (already set) is fine.
pub fn make_dpi_aware() {
    // SAFETY: no pointers; the context constant is a documented sentinel value.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// A visible top-level window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopWindow {
    pub handle: u64,
    pub pid: u32,
    pub title: String,
    /// Full path of the process's executable ("" when the process can't be queried).
    pub exe_path: String,
}

unsafe extern "system" fn collect_window(h: HWND, out: LPARAM) -> windows_sys::core::BOOL {
    // SAFETY: `out` is the `&mut Vec<u64>` passed to EnumWindows below, alive for the whole call.
    let out = unsafe { &mut *(out as *mut Vec<u64>) };
    out.push(h as usize as u64);
    TRUE
}

fn window_title(h: HWND) -> String {
    // SAFETY: the buffer is `len + 1` u16s and GetWindowTextW writes at most `buf.len()` including the NUL.
    unsafe {
        let len = GetWindowTextLengthW(h);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

fn exe_path(pid: u32) -> Option<String> {
    // SAFETY: the process handle is closed on every path; the buffer size is passed in and updated by the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..size as usize]))
    }
}

/// Visible top-level windows of all processes.
pub fn top_level_windows() -> Vec<TopWindow> {
    let mut handles: Vec<u64> = Vec::new();
    // SAFETY: the callback only pushes into `handles`, which outlives the call.
    unsafe {
        EnumWindows(Some(collect_window), &mut handles as *mut Vec<u64> as LPARAM);
    }
    handles
        .into_iter()
        .filter(|&h| {
            // SAFETY: plain query on a handle EnumWindows just returned (stale handles return FALSE).
            unsafe { IsWindowVisible(hwnd(h)) != 0 }
        })
        .map(|h| {
            let pid = window_owner_pid(h).unwrap_or(0);
            TopWindow {
                handle: h,
                pid,
                title: window_title(hwnd(h)),
                exe_path: exe_path(pid).unwrap_or_default(),
            }
        })
        .collect()
}

/// Creation time of process `pid` (FILETIME as u64), to tell a reused pid from the same process.
pub fn process_start_time(pid: u32) -> Option<u64> {
    // SAFETY: the FILETIMEs are local out-parameters; the handle is closed on every path.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if process.is_null() {
            return None;
        }
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let ok = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(process);
        (ok != 0).then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}

/// The process that owns window `handle`, if the window still exists.
pub fn window_owner_pid(handle: u64) -> Option<u32> {
    // SAFETY: `pid` is a local out-parameter.
    unsafe {
        if IsWindow(hwnd(handle)) == 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd(handle), &mut pid);
        (pid != 0).then_some(pid)
    }
}

/// The rectangle (left, top, right, bottom) of the monitor that shows most of window `handle`.
pub fn monitor_rect(handle: u64) -> Option<[i32; 4]> {
    // SAFETY: MONITORINFO is zero-initialised with its size set, as GetMonitorInfoW requires.
    unsafe {
        let monitor = MonitorFromWindow(hwnd(handle), MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(monitor, &mut info) != 0).then(|| from_rect(&info.rcMonitor))
    }
}

/// A window's frame state before the border changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowState {
    pub style: i64,
    pub ex_style: i64,
    pub menu: u64,
    /// Normal (restored) position.
    pub placement: [i32; 4],
    pub show_cmd: u32,
}

fn placement(h: HWND) -> Option<WINDOWPLACEMENT> {
    // SAFETY: WINDOWPLACEMENT is zero-initialised with its length set, as GetWindowPlacement requires.
    unsafe {
        let mut wp: WINDOWPLACEMENT = std::mem::zeroed();
        wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        (GetWindowPlacement(h, &mut wp) != 0).then_some(wp)
    }
}

pub fn save_window(handle: u64) -> Option<WindowState> {
    let h = hwnd(handle);
    // SAFETY: plain queries on a window handle; a stale handle makes IsWindow fail first.
    unsafe {
        if IsWindow(h) == 0 {
            return None;
        }
        let wp = placement(h)?;
        Some(WindowState {
            style: GetWindowLongPtrW(h, GWL_STYLE) as i64,
            ex_style: GetWindowLongPtrW(h, GWL_EXSTYLE) as i64,
            menu: GetMenu(h) as usize as u64,
            placement: from_rect(&wp.rcNormalPosition),
            show_cmd: wp.showCmd,
        })
    }
}

/// The client area of window `handle` in screen coordinates.
pub fn client_rect(handle: u64) -> Option<[i32; 4]> {
    let h = hwnd(handle);
    // SAFETY: RECT and POINTs are local out-parameters.
    unsafe {
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(h, &mut rc) == 0 {
            return None;
        }
        let mut tl = POINT { x: rc.left, y: rc.top };
        let mut br = POINT {
            x: rc.right,
            y: rc.bottom,
        };
        if ClientToScreen(h, &mut tl) == 0 || ClientToScreen(h, &mut br) == 0 {
            return None;
        }
        Some([tl.x, tl.y, br.x, br.y])
    }
}

fn set_window_rect(h: HWND, r: [i32; 4], raise: bool) {
    let z = if raise { 0 } else { SWP_NOZORDER };
    // SAFETY: plain call with value arguments.
    unsafe {
        SetWindowPos(
            h,
            HWND_TOP,
            r[0],
            r[1],
            r[2] - r[0],
            r[3] - r[1],
            SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE | z,
        );
    }
}

/// The window's menu handle, 0 for none.
pub fn window_menu(handle: u64) -> u64 {
    // SAFETY: plain query.
    unsafe { GetMenu(hwnd(handle)) as usize as u64 }
}

/// Window `handle` exists and isn't "Not responding". Changing a hung window's style or position would block
/// until it answers.
fn responsive(h: HWND) -> bool {
    // SAFETY: plain queries.
    unsafe { IsWindow(h) != 0 && IsHungAppWindow(h) == 0 }
}

/// Removes the title bar, the resizable frame and the menu of window `handle`, and moves it so that its client
/// area is exactly `client` (screen coordinates), above other normal windows. Returns whether it ended up there.
pub fn place_client(handle: u64, client: [i32; 4]) -> bool {
    fit_client(handle, client, true)
}

/// Like [`place_client`] for a window that is already placed: changes only what drifted, never raises it, and
/// leaves it alone while it is minimised or not responding.
pub fn keep_client(handle: u64, client: [i32; 4]) -> bool {
    let h = hwnd(handle);
    if !responsive(h) {
        return false;
    }
    // SAFETY: plain queries.
    let (minimised, framed, menu) = unsafe {
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        (
            IsIconic(h) != 0,
            style & (WS_CAPTION | WS_THICKFRAME) as isize != 0 || IsZoomed(h) != 0,
            !GetMenu(h).is_null(),
        )
    };
    if minimised {
        return true;
    }
    if !framed && !menu && client_rect(handle) == Some(client) {
        return true;
    }
    fit_client(handle, client, false)
}

fn fit_client(handle: u64, client: [i32; 4], raise: bool) -> bool {
    let h = hwnd(handle);
    if !responsive(h) {
        return false;
    }
    // SAFETY: style changes and positioning through documented calls on a window handle; RECT is local.
    unsafe {
        if IsIconic(h) != 0 || IsZoomed(h) != 0 {
            // Shows it at its normal size without taking the focus.
            ShowWindow(h, SW_SHOWNOACTIVATE);
        }
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        let frame = (WS_CAPTION | WS_THICKFRAME) as isize;
        let new_style = style & !frame;
        if new_style != style {
            SetWindowLongPtrW(h, GWL_STYLE, new_style);
        }
        if !GetMenu(h).is_null() {
            SetMenu(h, std::ptr::null_mut());
        }
        let ex_style = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let mut r = to_rect(client);
        AdjustWindowRectEx(&mut r, new_style as u32, FALSE, ex_style as u32);
        let mut outer = from_rect(&r);
        set_window_rect(h, outer, raise);
        // Some frames don't match AdjustWindowRectEx exactly (custom borders, DPI rounding): correct once.
        if let Some(got) = client_rect(handle) {
            if got != client {
                for i in 0..4 {
                    outer[i] += client[i] - got[i];
                }
                set_window_rect(h, outer, raise);
            }
        }
    }
    client_rect(handle) == Some(client)
}

/// Puts back the style, extended style, menu and normal position saved by [`save_window`]. False when the
/// window is gone or not responding.
pub fn restore_window(handle: u64, saved: &WindowState) -> bool {
    let h = hwnd(handle);
    if !responsive(h) {
        return false;
    }
    // SAFETY: documented calls on a window handle; the menu handle is checked with IsMenu before use.
    unsafe {
        SetWindowLongPtrW(h, GWL_STYLE, saved.style as isize);
        SetWindowLongPtrW(h, GWL_EXSTYLE, saved.ex_style as isize);
        let menu = saved.menu as usize as HMENU;
        if !menu.is_null() && IsMenu(menu) != 0 {
            SetMenu(h, menu);
        }
        if let Some(mut wp) = placement(h) {
            wp.rcNormalPosition = to_rect(saved.placement);
            // A normal window comes back without taking the focus.
            wp.showCmd = if saved.show_cmd == SW_SHOWNORMAL as u32 {
                SW_SHOWNOACTIVATE as u32
            } else {
                saved.show_cmd
            };
            SetWindowPlacement(h, &wp);
        }
        SetWindowPos(
            h,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        ) != 0
    }
}

/// White cells of the border window, in its client coordinates. Only one border window exists at a time.
static WHITE_CELLS: Mutex<Vec<[i32; 4]>> = Mutex::new(Vec::new());

unsafe extern "system" fn border_proc(h: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            // SAFETY: BeginPaint/EndPaint pair on this window with a local PAINTSTRUCT; stock brushes need no
            // cleanup.
            unsafe {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(h, &mut ps);
                FillRect(dc, &ps.rcPaint, GetStockObject(BLACK_BRUSH) as HBRUSH);
                let white = GetStockObject(WHITE_BRUSH) as HBRUSH;
                for r in WHITE_CELLS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
                    FillRect(dc, &to_rect(*r), white);
                }
                EndPaint(h, &ps);
            }
            0
        }
        // The whole client area is painted in WM_PAINT.
        WM_ERASEBKGND => 1,
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

/// The black full-screen window with the tags. Closes when dropped.
pub struct BorderWindow {
    handle: u64,
    thread: Option<JoinHandle<()>>,
}

impl BorderWindow {
    /// Opens it over `monitor` (left, top, right, bottom). `white` cells are relative to the monitor's top-left.
    pub fn open(monitor: [i32; 4], white: Vec<[i32; 4]>) -> Option<Self> {
        *WHITE_CELLS.lock().unwrap_or_else(|e| e.into_inner()) = white;
        let (tx, rx) = mpsc::channel::<u64>();
        let thread = std::thread::Builder::new()
            .name("tv-border-window".into())
            .spawn(move || {
                let h = create_border_window(monitor);
                let _ = tx.send(h as usize as u64);
                if h.is_null() {
                    return;
                }
                // SAFETY: standard message loop for the window this thread created.
                unsafe {
                    let mut msg: MSG = std::mem::zeroed();
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            })
            .ok()?;
        let handle = rx.recv().ok()?;
        if handle == 0 {
            let _ = thread.join();
            return None;
        }
        Some(Self {
            handle,
            thread: Some(thread),
        })
    }

    pub fn handle(&self) -> u64 {
        self.handle
    }
}

impl Drop for BorderWindow {
    fn drop(&mut self) {
        // SAFETY: posting to our own window; the thread destroys it and leaves its loop.
        unsafe {
            PostMessageW(hwnd(self.handle), WM_CLOSE, 0, 0);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn create_border_window(monitor: [i32; 4]) -> HWND {
    let class = wide("WiiUConnectTvBorder");
    // SAFETY: the class name buffer outlives both calls; registering twice fails harmlessly (class exists).
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(border_proc);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        let title = wide("");
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            monitor[0],
            monitor[1],
            monitor[2] - monitor[0],
            monitor[3] - monitor[1],
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    }
}
