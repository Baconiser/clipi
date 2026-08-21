//! Windows HWND show/hide. A hidden eframe window does not process
//! request_repaint, so tray and hotkey callbacks must ShowWindow themselves
//! while still inside the input event (required for SetForegroundWindow).

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct Handle {
    #[allow(dead_code)]
    hwnd: Arc<AtomicIsize>,
    shown: Arc<AtomicBool>,
    exiting: Arc<AtomicBool>,
}

impl Handle {
    pub fn new() -> Self {
        Self {
            hwnd: Arc::new(AtomicIsize::new(0)),
            shown: Arc::new(AtomicBool::new(false)),
            exiting: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_shown(&self) -> bool {
        self.shown.load(Ordering::SeqCst)
    }

    #[cfg(windows)]
    pub fn capture(&self, handle: &impl raw_window_handle::HasWindowHandle) {
        use raw_window_handle::RawWindowHandle;
        if let Ok(h) = handle.window_handle() {
            if let RawWindowHandle::Win32(w) = h.as_raw() {
                self.hwnd.store(w.hwnd.get(), Ordering::SeqCst);
            }
        }
    }

    #[cfg(not(windows))]
    pub fn capture(&self, _handle: &impl raw_window_handle::HasWindowHandle) {}

    pub fn show(&self) {
        self.shown.store(true, Ordering::SeqCst);
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
            };
            let hwnd = self.hwnd.load(Ordering::SeqCst);
            if hwnd == 0 {
                return;
            }
            let hwnd = hwnd as HWND;
            ShowWindow(hwnd, SW_RESTORE);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }

    pub fn hide(&self) {
        self.shown.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            let hwnd = self.hwnd.load(Ordering::SeqCst);
            if hwnd == 0 {
                return;
            }
            ShowWindow(hwnd as HWND, SW_HIDE);
        }
    }

    pub fn is_exiting(&self) -> bool {
        self.exiting.load(Ordering::SeqCst)
    }

    pub fn quit(&self) {
        self.exiting.store(true, Ordering::SeqCst);
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                PostMessageW, PostQuitMessage, WM_CLOSE,
            };
            let hwnd = self.hwnd.load(Ordering::SeqCst);
            if hwnd != 0 {
                PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
            }
            PostQuitMessage(0);
        }
    }
}
