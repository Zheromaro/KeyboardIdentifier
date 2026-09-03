use super::enumerator::DeviceEnumerator;
use super::errors::handle_key;
use super::raw_input::RawInput;
use crate::keyboard_source::{Keyboard, KeyboardEvent};

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::null_mut;
use tokio::sync::mpsc;

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::UI::Input::{
    RAWINPUTDEVICE, RIDEV_REMOVE, RIM_TYPEKEYBOARD, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GIDC_ARRIVAL, GIDC_REMOVAL, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE, MSG,
    PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_CREATE, WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_KEYDOWN, WM_NCDESTROY,
    WM_SYSKEYDOWN, WNDCLASSEXW,
};
use windows::core::w;

const WINDOW_CLASS_NAME: windows::core::PCWSTR = w!("KeyboardIdentifierRawInputWindow");

pub(crate) struct WindowState {
    sender: mpsc::UnboundedSender<KeyboardEvent>,
    keyboards: HashMap<isize, Keyboard>,
}

impl WindowState {
    pub(crate) fn new(
        sender: mpsc::UnboundedSender<KeyboardEvent>,
        keyboards: HashMap<isize, Keyboard>,
    ) -> Self {
        Self { sender, keyboards }
    }

    fn emit(&self, event: KeyboardEvent) {
        let _ = self.sender.send(event);
    }

    fn device_change(&mut self, action: u32, handle: HANDLE) {
        let key = handle_key(handle);

        match action {
            GIDC_ARRIVAL => {
                let Some(keyboard) = DeviceEnumerator::keyboard_from_handle(handle) else {
                    return;
                };

                let was_present = self.keyboards.insert(key, keyboard.clone()).is_some();

                if !was_present {
                    self.emit(KeyboardEvent::Plugged(keyboard));
                }
            }

            GIDC_REMOVAL => {
                if let Some(keyboard) = self.keyboards.remove(&key) {
                    self.emit(KeyboardEvent::Unplugged(keyboard));
                }
            }

            _ => {}
        }
    }

    fn raw_input(&mut self, lparam: LPARAM) {
        let Some(raw) = RawInput::from_message(lparam) else {
            return;
        };
        if raw.header().dwType != RIM_TYPEKEYBOARD.0 {
            return;
        }

        let Some(message) = raw.keyboard_message() else {
            return;
        };

        if message != WM_KEYDOWN && message != WM_SYSKEYDOWN {
            return;
        }

        let handle = raw.header().hDevice;
        let key = handle_key(handle);

        let keyboard = match self.keyboards.get(&key) {
            Some(keyboard) => keyboard.clone(),
            None => {
                let Some(keyboard) = DeviceEnumerator::keyboard_from_handle(handle) else {
                    return;
                };

                self.keyboards.insert(key, keyboard.clone());
                keyboard
            }
        };

        self.emit(KeyboardEvent::Pressed(keyboard));
    }
}

pub(crate) struct MessageOnlyWindow {
    pub(crate) hwnd: HWND,
}

impl MessageOnlyWindow {
    pub(crate) fn create(
        instance: HINSTANCE,
        state: *mut WindowState,
    ) -> Result<Self, windows::core::Error> {
        unsafe {
            Self::register_class(instance)?;

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                WINDOW_CLASS_NAME,
                w!(""),
                WINDOW_STYLE::default(),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                HWND_MESSAGE,
                None,
                instance,
                Some(state as *const c_void),
            )?;

            Ok(Self { hwnd })
        }
    }

    pub(crate) fn run_message_loop(self) {
        unsafe {
            let mut message = MSG::default();

            loop {
                let result = GetMessageW(&mut message, None, 0, 0);

                if result.0 == -1 || result.0 == 0 {
                    break;
                }

                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    unsafe fn register_class(instance: HINSTANCE) -> Result<(), windows::core::Error> {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(Self::wnd_proc),
            hInstance: instance,
            lpszClassName: WINDOW_CLASS_NAME,
            ..Default::default()
        };

        let result = unsafe { RegisterClassExW(&class) };

        if result != 0 {
            return Ok(());
        }

        let error = unsafe { GetLastError() };

        if error == ERROR_CLASS_ALREADY_EXISTS {
            Ok(())
        } else {
            Err(windows::core::Error::from_win32())
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CREATE {
            let create = lparam.0 as *const CREATESTRUCTW;
            let state = unsafe { (*create).lpCreateParams as *mut WindowState };

            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            }

            return LRESULT(0);
        }

        let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };

        if msg == WM_CLOSE {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }

            return LRESULT(0);
        }

        if msg == WM_NCDESTROY {
            let unregister = RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: 0x06,
                dwFlags: RIDEV_REMOVE,
                hwndTarget: HWND(null_mut()),
            };

            let _ = unsafe {
                RegisterRawInputDevices(
                    std::slice::from_ref(&unregister),
                    std::mem::size_of::<RAWINPUTDEVICE>() as u32,
                )
            };

            if !state_ptr.is_null() {
                unsafe {
                    drop(Box::from_raw(state_ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }

            unsafe {
                PostQuitMessage(0);
            }

            return LRESULT(0);
        }

        if state_ptr.is_null() {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }

        let state = unsafe { &mut *state_ptr };

        match msg {
            WM_INPUT_DEVICE_CHANGE => {
                state.device_change(wparam.0 as u32, HANDLE(lparam.0 as *mut c_void));
                LRESULT(0)
            }

            WM_INPUT => {
                state.raw_input(lparam);
                LRESULT(0)
            }

            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}
