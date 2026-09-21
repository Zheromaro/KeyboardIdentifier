use super::{
    KeyEvent,
    device::{DeviceEnumerator, DeviceHandle, DiscoveredKeyboard, RawInput},
    key_mapping::*,
};
use crate::keyboard_source::{Access, Keyboard, KeyboardEvent};
use keyboard_types::{KeyState, Modifiers};
use std::{
    collections::HashSet,
    ffi::c_void,
    io,
    ptr::null_mut,
    sync::{
        Arc, LazyLock, RwLock,
        atomic::{AtomicIsize, Ordering},
    },
};
use tokio::sync::{broadcast, mpsc, oneshot};
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
            CREATESTRUCTW, CW_USEDEFAULT, CallNextHookEx, CreateWindowExW, DefWindowProcW,
            DestroyWindow, DispatchMessageW, GIDC_ARRIVAL, GIDC_REMOVAL, GWLP_USERDATA,
            GetMessageW, GetWindowLongPtrW, HHOOK, HWND_MESSAGE, MSG, PostMessageW,
            PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, SetWindowsHookExW,
            TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WINDOW_EX_STYLE, WINDOW_STYLE,
            WM_CLOSE, WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_NCCREATE, WM_NCDESTROY, WM_USER,
            WNDCLASSEXW,
        },
    },
};
use windows::core::w;

const WINDOW_CLASS_NAME: windows::core::PCWSTR = w!("KeyboardIdentifierRawInputWindow");
pub(crate) const WM_USER_COMMAND: u32 = WM_USER + 100;

static CONSUMED_HANDLES: LazyLock<RwLock<HashSet<DeviceHandle>>> =
    LazyLock::new(|| RwLock::new(HashSet::new()));
static LAST_RAW_HANDLE: RwLock<Option<DeviceHandle>> = RwLock::new(None);

pub(crate) enum ThreadCommand {
    Consume(Keyboard, oneshot::Sender<io::Result<()>>),
    Release(Keyboard, oneshot::Sender<io::Result<()>>),
}

#[derive(Debug)]
pub(crate) struct WindowHandleSlot {
    pub(crate) value: AtomicIsize,
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

    pub(crate) fn notify(&self) {
        let value = self.value.load(Ordering::Acquire);
        if value != 0 {
            unsafe {
                let _ = PostMessageW(
                    HWND(value as *mut c_void),
                    WM_USER_COMMAND,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }
}

pub(crate) struct WindowState {
    sender: broadcast::Sender<KeyboardEvent>,
    keyboards: Vec<(DeviceHandle, Arc<Keyboard>)>,
    consumed_devices: HashSet<DeviceHandle>,
    modifiers: Modifiers,
    key_action_keys: HashSet<(DeviceHandle, u16)>,
    cmd_rx: mpsc::Receiver<ThreadCommand>,
    instance: HINSTANCE,
    hook_handle: Option<HHOOK>,
}

impl WindowState {
    pub(crate) fn new(
        sender: broadcast::Sender<KeyboardEvent>,
        keyboards: Vec<DiscoveredKeyboard>,
        cmd_rx: mpsc::Receiver<ThreadCommand>,
        instance: HINSTANCE,
    ) -> Self {
        Self {
            sender,
            keyboards: keyboards
                .into_iter()
                .map(|d| (d.handle, Arc::new(d.keyboard)))
                .collect(),
            consumed_devices: HashSet::new(),
            modifiers: Modifiers::empty(),
            key_action_keys: HashSet::new(),
            cmd_rx,
            instance,
            hook_handle: None,
        }
    }

    fn find_device_handle(&self, target: &Keyboard) -> Option<DeviceHandle> {
        self.keyboards.iter().find_map(|(handle, kb)| {
            if kb.keyboard_id == target.keyboard_id && kb.port_id == target.port_id {
                Some(*handle)
            } else {
                None
            }
        })
    }

    fn process_commands(&mut self) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            match cmd {
                ThreadCommand::Consume(target_kb, reply) => {
                    let result = self.handle_consume(&target_kb);
                    let _ = reply.send(result);
                }
                ThreadCommand::Release(target_kb, reply) => {
                    let result = self.handle_release(&target_kb);
                    let _ = reply.send(result);
                }
            }
        }
    }

    fn handle_consume(&mut self, target_kb: &Keyboard) -> io::Result<()> {
        let Some(device) = self.find_device_handle(target_kb) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Specified keyboard was not found",
            ));
        };

        if self.consumed_devices.insert(device) {
            if let Ok(mut global_set) = CONSUMED_HANDLES.write() {
                global_set.insert(device);
            }

            if let Some(pos) = self.keyboards.iter().position(|(d, _)| *d == device) {
                let mut updated_kb = (*self.keyboards[pos].1).clone();
                updated_kb.access = Access::Exclusive;
                self.keyboards[pos].1 = Arc::new(updated_kb);
            }

            if self.hook_handle.is_none() {
                let hook = unsafe {
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(Self::ll_keyboard_proc),
                        self.instance,
                        0,
                    )
                }
                .map_err(io::Error::other)?;
                self.hook_handle = Some(hook);
            }
        }

        Ok(())
    }

    fn handle_release(&mut self, target_kb: &Keyboard) -> io::Result<()> {
        let Some(device) = self.find_device_handle(target_kb) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Specified keyboard was not found",
            ));
        };

        if self.consumed_devices.remove(&device) {
            if let Ok(mut global_set) = CONSUMED_HANDLES.write() {
                global_set.remove(&device);
            }

            if let Some(pos) = self.keyboards.iter().position(|(d, _)| *d == device) {
                let mut updated_kb = (*self.keyboards[pos].1).clone();
                updated_kb.access = Access::Shared;
                self.keyboards[pos].1 = Arc::new(updated_kb);
            }

            if self.consumed_devices.is_empty() {
                if let Some(hook) = self.hook_handle.take() {
                    unsafe {
                        let _ = UnhookWindowsHookEx(hook);
                    }
                }
            }
        }

        Ok(())
    }

    unsafe extern "system" fn ll_keyboard_proc(
        ncode: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if ncode >= 0 {
            let is_consumed = if let Ok(last) = LAST_RAW_HANDLE.read() {
                if let Some(dev) = *last {
                    if let Ok(consumed) = CONSUMED_HANDLES.read() {
                        consumed.contains(&dev)
                    } else {
                        false
                    }
                } else {
                    if let Ok(consumed) = CONSUMED_HANDLES.read() {
                        !consumed.is_empty()
                    } else {
                        false
                    }
                }
            } else {
                false
            };

            if is_consumed {
                return LRESULT(1);
            }
        }
        unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
    }

    fn handle_device_change(&mut self, action: u32, handle: HANDLE) {
        let device = DeviceHandle::from(handle);
        match action {
            GIDC_ARRIVAL => {
                if !self.keyboards.iter().any(|(c, _)| c == &device) {
                    if let Some(keyboard) = DeviceEnumerator::keyboard_from_handle(handle) {
                        let keyboard = Arc::new(keyboard);
                        self.keyboards.push((device, Arc::clone(&keyboard)));
                        let _ = self.sender.send(KeyboardEvent::Plugged(keyboard));
                    }
                }
            }
            GIDC_REMOVAL => {
                if let Some(index) = self.keyboards.iter().position(|(c, _)| c == &device) {
                    let keyboard = self.keyboards.remove(index).1;
                    self.key_action_keys.retain(|(d, _)| d != &device);
                    self.consumed_devices.remove(&device);
                    if let Ok(mut global_set) = CONSUMED_HANDLES.write() {
                        global_set.remove(&device);
                    }
                    let _ = self.sender.send(KeyboardEvent::Unplugged(keyboard));
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

        if let Ok(mut last) = LAST_RAW_HANDLE.write() {
            *last = Some(device);
        }

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
            self.key_action_keys.remove(&key_tuple);
            false
        } else {
            !self.key_action_keys.insert(key_tuple)
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
            .send(KeyboardEvent::KeyAction(keyboard, key_event));
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
            WM_USER_COMMAND => {
                if let Some(state) =
                    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState).as_mut() }
                {
                    state.process_commands();
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
                        let mut state = Box::from_raw(ptr);
                        if let Some(hook) = state.hook_handle.take() {
                            let _ = UnhookWindowsHookEx(hook);
                        }
                        if let Ok(mut set) = CONSUMED_HANDLES.write() {
                            set.clear();
                        }
                    }
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}
