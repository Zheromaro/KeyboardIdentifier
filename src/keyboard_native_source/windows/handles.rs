use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};

use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DeviceHandle(isize);

impl From<HANDLE> for DeviceHandle {
    fn from(handle: HANDLE) -> Self {
        Self(handle.0 as isize)
    }
}

#[derive(Debug)]
pub(crate) struct OwnedHandle(HANDLE);

impl OwnedHandle {
    pub(crate) fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_invalid() {
            None
        } else {
            Some(Self(handle))
        }
    }

    pub(crate) fn get(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Debug)]
pub(crate) struct WindowHandleSlot {
    value: AtomicIsize,
}

impl WindowHandleSlot {
    pub(crate) fn new() -> Self {
        Self {
            value: AtomicIsize::new(0),
        }
    }

    pub(crate) fn store(&self, hwnd: HWND) {
        self.value.store(hwnd.0 as isize, Ordering::Release);
    }

    pub(crate) fn clear(&self) {
        self.value.store(0, Ordering::Release);
    }

    pub(crate) fn close(&self) {
        let value = self.value.load(Ordering::Acquire);

        if value == 0 {
            return;
        }

        let hwnd = HWND(value as *mut c_void);

        unsafe {
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
