use super::{
    KeyEvent,
    device::{DeviceEnumerator, DeviceHandle, DiscoveredKeyboard, RawInput},
    key_mapping::*,
};
use crate::keyboard_source::{Keyboard, KeyboardEvent};
use keyboard_types::{KeyState, Modifiers};
use std::{
    collections::HashSet,
    ffi::c_void,
    io,
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{AtomicIsize, Ordering},
    },
};
use tokio::sync::mpsc;
use windows::Win32::{
    Foundation::{
        ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
    },
    UI::{
        Input::{
            KeyboardAndMouse::{VIRTUAL_KEY, VK_CAPITAL, VK_NUMLOCK, VK_SCROLL},
            RAWINPUTDEVICE, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE,
            RegisterRawInputDevices,
        },
        WindowsAndMessaging::{
            CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
            DispatchMessageW, GIDC_ARRIVAL, GIDC_REMOVAL, GWLP_USERDATA, GetMessageW,
            GetWindowLongPtrW, HWND_MESSAGE, MSG, PostMessageW, PostQuitMessage, RegisterClassExW,
            SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_INPUT,
            WM_INPUT_DEVICE_CHANGE, WM_NCCREATE, WM_NCDESTROY, WNDCLASSEXW,
        },
    },
};
use windows::core::w;

const WINDOW_CLASS_NAME: windows::core::PCWSTR = w!("KeyboardIdentifierRawInputWindow");

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
        if value != 0 {
            unsafe {
                let _ = PostMessageW(HWND(value as *mut c_void), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub(crate) struct WindowState {
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    keyboards: Vec<(DeviceHandle, Arc<Keyboard>)>,
    modifiers: Modifiers,
    pressed_keys: HashSet<(DeviceHandle, u16)>,
}

impl WindowState {
    pub(crate) fn new(
        sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
        keyboards: Vec<DiscoveredKeyboard>,
    ) -> Self {
        Self {
            sender,
            keyboards: keyboards
                .into_iter()
                .map(|d| (d.handle, Arc::new(d.keyboard)))
                .collect(),
            modifiers: Modifiers::empty(),
            pressed_keys: HashSet::new(),
        }
    }

    fn handle_device_change(&mut self, action: u32, handle: HANDLE) {
        let device = DeviceHandle::from(handle);
        match action {
            GIDC_ARRIVAL => {
                if !self.keyboards.iter().any(|(c, _)| c == &device) {
                    if let Some(keyboard) = DeviceEnumerator::keyboard_from_handle(handle) {
                        let keyboard = Arc::new(keyboard);
                        self.keyboards.push((device, Arc::clone(&keyboard)));
                        let _ = self
                            .sender
                            .blocking_send(Ok(KeyboardEvent::Plugged(keyboard)));
                    }
                }
            }
            GIDC_REMOVAL => {
                if let Some(index) = self.keyboards.iter().position(|(c, _)| c == &device) {
                    let keyboard = self.keyboards.remove(index).1;
                    self.pressed_keys.retain(|(d, _)| d != &device);
                    let _ = self
                        .sender
                        .blocking_send(Ok(KeyboardEvent::Unplugged(keyboard)));
                }
            }
            _ => {}
        }
    }

    fn handle_input(&mut self, lparam: LPARAM) {
        let Ok(input) = RawInput::from_message(lparam) else {
            return;
        };
        if !input.is_keyboard() {
            return;
        }
        let handle = input.device();
        let device = DeviceHandle::from(handle);

        let keyboard = match self
            .keyboards
            .iter()
            .find(|(c, _)| c == &device)
            .map(|(_, k)| Arc::clone(k))
        {
            Some(k) => k,
            None => {
                if let Some(k) = DeviceEnumerator::keyboard_from_handle(handle) {
                    let k = Arc::new(k);
                    self.keyboards.push((device, Arc::clone(&k)));
                    k
                } else {
                    return;
                }
            }
        };

        let raw_vkey = input.vkey();
        let vkey = VIRTUAL_KEY(raw_vkey);
        let is_e0 = input.is_extended();
        let is_up = input.is_key_up();

        let state = if is_up { KeyState::Up } else { KeyState::Down };

        let key_tuple = (device, raw_vkey);
        let repeat = if is_up {
            self.pressed_keys.remove(&key_tuple);
            false
        } else {
            !self.pressed_keys.insert(key_tuple)
        };

        let modifier = modifier_for_key(vkey, is_e0);
        let is_lock_key = matches!(vkey, VK_CAPITAL | VK_NUMLOCK | VK_SCROLL);

        let event_modifiers = match state {
            KeyState::Down => {
                if let Some(modifier) = modifier {
                    if is_lock_key && !repeat {
                        self.modifiers.toggle(modifier);
                    } else if !is_lock_key {
                        self.modifiers.insert(modifier);
                    }
                }
                self.modifiers
            }
            KeyState::Up => {
                if !is_lock_key {
                    if let Some(modifier) = modifier {
                        self.modifiers.remove(modifier);
                    }
                }
                self.modifiers
            }
        };

        let key_event = KeyEvent {
            state,
            key: raw_to_key(vkey),
            code: raw_to_code(vkey, is_e0, input.scancode()),
            location: raw_to_location(vkey, is_e0),
            modifiers: event_modifiers,
            repeat,
            is_composing: false,
        };

        let _ = self
            .sender
            .blocking_send(Ok(KeyboardEvent::Pressed(keyboard, key_event)));
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
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(Self::wnd_proc),
            hInstance: instance,
            lpszClassName: WINDOW_CLASS_NAME,
            ..Default::default()
        };
        if unsafe { RegisterClassExW(&class) } == 0
            && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(windows::core::Error::from_win32());
        }

        let state_ptr = Box::into_raw(Box::new(state));
        match unsafe {
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
        } {
            Ok(hwnd) => Ok(Self { hwnd }),
            Err(e) => {
                unsafe {
                    drop(Box::from_raw(state_ptr));
                }
                Err(e)
            }
        }
    }

    pub(crate) fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub(crate) fn destroy(&self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
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
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_NCCREATE => {
                unsafe {
                    let state = (*(lparam.0 as *const CREATESTRUCTW)).lpCreateParams;
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                }
                LRESULT(1)
            }
            WM_CLOSE => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_INPUT_DEVICE_CHANGE => {
                if let Some(state) =
                    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState).as_mut() }
                {
                    state.handle_device_change(wparam.0 as u32, HANDLE(lparam.0 as *mut c_void));
                }
                LRESULT(0)
            }
            WM_INPUT => {
                if let Some(state) =
                    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState).as_mut() }
                {
                    state.handle_input(lparam);
                }
                LRESULT(0)
            }
            WM_NCDESTROY => {
                unsafe {
                    let device = RAWINPUTDEVICE {
                        usUsagePage: 0x01,
                        usUsage: 0x06,
                        dwFlags: RIDEV_REMOVE,
                        hwndTarget: HWND(null_mut()),
                    };
                    let _ = RegisterRawInputDevices(
                        std::slice::from_ref(&device),
                        std::mem::size_of::<RAWINPUTDEVICE>() as u32,
                    );
                    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    if !ptr.is_null() {
                        drop(Box::from_raw(ptr));
                    }
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}
