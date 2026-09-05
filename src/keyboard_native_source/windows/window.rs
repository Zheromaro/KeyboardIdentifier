use super::enumerator::{DeviceEnumerator, DiscoveredKeyboard};
use super::handles::DeviceHandle;
use super::keyboard_state::KeyboardState;
use super::raw_input::RawInput;
use crate::keyboard_source::KeyboardEvent;
use std::ffi::c_void;
use std::io;
use std::ptr::null_mut;
use tokio::sync::mpsc;
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::UI::Input::{
    RAWINPUTDEVICE, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GIDC_ARRIVAL, GIDC_REMOVAL, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE, MSG,
    PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_NCCREATE, WM_NCDESTROY,
    WNDCLASSEXW,
};
use windows::core::w;

const WINDOW_CLASS_NAME: windows::core::PCWSTR = w!("KeyboardIdentifierRawInputWindow");

pub(crate) struct WindowState {
    sender: mpsc::UnboundedSender<KeyboardEvent>,
    keyboards: KeyboardState,
}

impl WindowState {
    pub(crate) fn new(
        sender: mpsc::UnboundedSender<KeyboardEvent>,
        keyboards: Vec<DiscoveredKeyboard>,
    ) -> Self {
        let keyboards = KeyboardState::new(
            keyboards
                .into_iter()
                .map(|device| (device.handle, device.keyboard)),
        );

        Self { sender, keyboards }
    }

    fn emit(&self, event: KeyboardEvent) {
        let _ = self.sender.send(event);
    }

    fn handle_device_change(&mut self, action: u32, handle: HANDLE) {
        let device = DeviceHandle::from(handle);

        match action {
            GIDC_ARRIVAL => self.handle_arrival(device, handle),
            GIDC_REMOVAL => self.handle_removal(device),
            _ => {}
        }
    }

    fn handle_arrival(&mut self, device: DeviceHandle, handle: HANDLE) {
        let Some(keyboard) = DeviceEnumerator::keyboard_from_handle(handle) else {
            return;
        };

        let was_new = self.keyboards.insert(device, keyboard.clone());

        if was_new {
            self.emit(KeyboardEvent::Plugged(keyboard));
        }
    }

    fn handle_removal(&mut self, device: DeviceHandle) {
        if let Some(keyboard) = self.keyboards.remove(device) {
            self.emit(KeyboardEvent::Unplugged(keyboard));
        }
    }

    fn handle_input(&mut self, lparam: LPARAM) {
        let Ok(input) = RawInput::from_message(lparam) else {
            return;
        };

        if !input.is_keyboard() || !input.is_key_down() {
            return;
        }

        let handle = input.device();
        let device = DeviceHandle::from(handle);

        let keyboard = match self.keyboards.get(device) {
            Some(keyboard) => keyboard.clone(),

            None => {
                let Some(keyboard) = DeviceEnumerator::keyboard_from_handle(handle) else {
                    return;
                };

                self.keyboards.insert(device, keyboard.clone());

                keyboard
            }
        };

        self.emit(KeyboardEvent::Pressed(keyboard));
    }
}

pub(crate) struct MessageOnlyWindow {
    hwnd: HWND,
}

impl MessageOnlyWindow {
    pub(crate) fn create(
        instance: HINSTANCE,
        state: WindowState,
    ) -> Result<Self, windows::core::Error> {
        Self::register_class(instance)?;

        let state = Box::new(state);
        let state_ptr = Box::into_raw(state);

        let result = unsafe {
            CreateWindowExW(
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
                Some(state_ptr.cast::<c_void>()),
            )
        };

        match result {
            Ok(hwnd) => Ok(Self { hwnd }),

            Err(error) => {
                // SAFETY:
                // CreateWindowExW failed, therefore ownership of
                // `state_ptr` was never transferred to the window.
                unsafe {
                    drop(Box::from_raw(state_ptr));
                }

                Err(error)
            }
        }
    }

    pub(crate) fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub(crate) fn register_raw_input(&self) -> io::Result<()> {
        let device = RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x06,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: self.hwnd,
        };

        unsafe {
            RegisterRawInputDevices(
                std::slice::from_ref(&device),
                std::mem::size_of::<RAWINPUTDEVICE>() as u32,
            )
        }
        .map_err(io::Error::other)
    }

    pub(crate) fn run_message_loop(&self) {
        loop {
            let mut message = MSG::default();

            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };

            if result.0 == -1 || result.0 == 0 {
                break;
            }

            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    pub(crate) fn destroy(&self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }

    fn register_class(instance: HINSTANCE) -> Result<(), windows::core::Error> {
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
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCCREATE => {
                Self::attach_state(hwnd, lparam);

                LRESULT(1)
            }

            WM_CLOSE => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }

                LRESULT(0)
            }

            WM_INPUT_DEVICE_CHANGE => {
                Self::with_state(hwnd, |state| {
                    let handle = HANDLE(lparam.0 as *mut c_void);
                    state.handle_device_change(wparam.0 as u32, handle);
                });

                LRESULT(0)
            }

            WM_INPUT => {
                Self::with_state(hwnd, |state| {
                    state.handle_input(lparam);
                });

                LRESULT(0)
            }

            WM_NCDESTROY => {
                Self::unregister_raw_input();
                Self::detach_state(hwnd);

                unsafe {
                    PostQuitMessage(0);
                }

                LRESULT(0)
            }

            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn attach_state(hwnd: HWND, lparam: LPARAM) {
        // SAFETY:
        // During WM_NCCREATE, lParam points to the CREATESTRUCTW
        // supplied by CreateWindowExW.
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };

        let state = create.lpCreateParams;

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
        }
    }

    fn with_state(hwnd: HWND, callback: impl FnOnce(&mut WindowState)) {
        let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };

        if state_ptr.is_null() {
            return;
        }

        // SAFETY:
        // The pointer was created by Box::into_raw during window creation
        // and remains valid until WM_NCDESTROY.
        let state = unsafe { &mut *state_ptr };

        callback(state);
    }

    fn detach_state(hwnd: HWND) {
        let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }

        if state_ptr.is_null() {
            return;
        }

        // SAFETY:
        // Ownership was transferred to the window through Box::into_raw.
        // WM_NCDESTROY is the single point where the allocation is reclaimed.
        unsafe {
            drop(Box::from_raw(state_ptr));
        }
    }

    fn unregister_raw_input() {
        let device = RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x06,
            dwFlags: RIDEV_REMOVE,
            hwndTarget: HWND(null_mut()),
        };

        unsafe {
            let _ = RegisterRawInputDevices(
                std::slice::from_ref(&device),
                std::mem::size_of::<RAWINPUTDEVICE>() as u32,
            );
        }
    }
}
