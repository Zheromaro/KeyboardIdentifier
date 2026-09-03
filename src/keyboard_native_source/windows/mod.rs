mod enumerator;
mod errors;
mod path_parser;
mod raw_input;
mod window;
use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource};
use enumerator::DeviceEnumerator;
use errors::windows_error;
use std::ffi::c_void;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use tokio::sync::{mpsc, oneshot};
use window::{MessageOnlyWindow, WindowState};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    RAWINPUTDEVICE, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, PostMessageW, WM_CLOSE};

static RAW_INPUT_OWNER: AtomicBool = AtomicBool::new(false);

pub struct WindowsKeyboardSource {
    rx: mpsc::UnboundedReceiver<KeyboardEvent>,
    keyboards: Vec<Keyboard>,
    hwnd: Arc<AtomicIsize>,
    thread: Option<std::thread::JoinHandle<()>>,
    init_error: Option<io::Error>,
}

impl KeyboardSource for WindowsKeyboardSource {
    async fn new() -> Self {
        if RAW_INPUT_OWNER
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            let (_tx, rx) = mpsc::unbounded_channel();

            return Self {
                rx,
                keyboards: Vec::new(),
                hwnd: Arc::new(AtomicIsize::new(0)),
                thread: None,
                init_error: Some(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "another WindowsKeyboardSource is already active",
                )),
            };
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let (init_tx, init_rx) = oneshot::channel::<Result<Vec<Keyboard>, io::Error>>();

        let hwnd_slot = Arc::new(AtomicIsize::new(0));
        let hwnd_slot_thread = Arc::clone(&hwnd_slot);

        let thread = std::thread::spawn(move || {
            let result = (|| -> Result<Vec<Keyboard>, io::Error> {
                let instance = unsafe {
                    GetModuleHandleW(None)
                        .map(HINSTANCE::from)
                        .map_err(windows_error)?
                };

                let initial_map = DeviceEnumerator::enumerate_keyboards()?;
                let initial_keyboards = initial_map.values().cloned().collect();

                let state = Box::new(WindowState::new(tx, initial_map));
                let state_ptr = Box::into_raw(state);

                let window = match MessageOnlyWindow::create(instance, state_ptr) {
                    Ok(window) => window,
                    Err(error) => {
                        unsafe {
                            drop(Box::from_raw(state_ptr));
                        }
                        return Err(windows_error(error));
                    }
                };

                let hwnd_bits = window.hwnd.0 as isize;
                hwnd_slot_thread.store(hwnd_bits, Ordering::Release);

                let raw_input = RAWINPUTDEVICE {
                    usUsagePage: 0x01,
                    usUsage: 0x06,
                    dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                    hwndTarget: window.hwnd,
                };

                if let Err(error) = unsafe {
                    RegisterRawInputDevices(
                        std::slice::from_ref(&raw_input),
                        std::mem::size_of::<RAWINPUTDEVICE>() as u32,
                    )
                } {
                    unsafe {
                        let _ = DestroyWindow(window.hwnd);
                    }

                    hwnd_slot_thread.store(0, Ordering::Release);
                    return Err(windows_error(error));
                }

                Ok(initial_keyboards)
            })();

            match result {
                Ok(keyboards) => {
                    let _ = init_tx.send(Ok(keyboards));
                    let hwnd_bits = hwnd_slot_thread.load(Ordering::Acquire);

                    if hwnd_bits != 0 {
                        let window = MessageOnlyWindow {
                            hwnd: HWND(hwnd_bits as *mut c_void),
                        };
                        window.run_message_loop();
                    }
                }
                Err(error) => {
                    let _ = init_tx.send(Err(error));
                }
            }

            hwnd_slot_thread.store(0, Ordering::Release);
        });

        match init_rx.await {
            Ok(Ok(keyboards)) => Self {
                rx,
                keyboards,
                hwnd: hwnd_slot,
                thread: Some(thread),
                init_error: None,
            },
            Ok(Err(error)) => {
                let _ = thread.join();
                RAW_INPUT_OWNER.store(false, Ordering::Release);

                Self {
                    rx,
                    keyboards: Vec::new(),
                    hwnd: hwnd_slot,
                    thread: None,
                    init_error: Some(error),
                }
            }
            Err(_) => {
                let _ = thread.join();
                RAW_INPUT_OWNER.store(false, Ordering::Release);

                Self {
                    rx,
                    keyboards: Vec::new(),
                    hwnd: hwnd_slot,
                    thread: None,
                    init_error: Some(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Windows keyboard input thread exited during initialization",
                    )),
                }
            }
        }
    }

    fn get_keyboards(&self) -> Vec<Keyboard> {
        self.keyboards.clone()
    }

    async fn receive_event(&mut self) -> Result<KeyboardEvent, io::Error> {
        if let Some(error) = self.init_error.take() {
            return Err(error);
        }

        match self.rx.recv().await {
            Some(event) => {
                match &event {
                    KeyboardEvent::Plugged(keyboard) => {
                        if !self.keyboards.contains(keyboard) {
                            self.keyboards.push(keyboard.clone());
                        }
                    }
                    KeyboardEvent::Unplugged(keyboard) => {
                        self.keyboards.retain(|current| {
                            current.port_id.physical_path != keyboard.port_id.physical_path
                        });
                    }
                    KeyboardEvent::Pressed(_) => {}
                }

                Ok(event)
            }
            None => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Windows keyboard input thread exited",
            )),
        }
    }
}

impl Drop for WindowsKeyboardSource {
    fn drop(&mut self) {
        let hwnd_bits = self.hwnd.swap(0, Ordering::AcqRel);

        if hwnd_bits != 0 {
            let hwnd = HWND(hwnd_bits as *mut c_void);

            unsafe {
                let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }

        RAW_INPUT_OWNER.store(false, Ordering::Release);
    }
}
