use super::enumerator::DeviceEnumerator;
use super::errors::windows_error;
use super::handles::WindowHandleSlot;
use super::window::{MessageOnlyWindow, WindowState};

use crate::keyboard_source::{Keyboard, KeyboardEvent};

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use tokio::sync::{mpsc, oneshot};

static RAW_INPUT_OWNER: AtomicBool = AtomicBool::new(false);

pub(crate) struct RawInputOwner;

impl RawInputOwner {
    pub(crate) fn acquire() -> io::Result<Self> {
        if RAW_INPUT_OWNER
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "another WindowsKeyboardSource is already active",
            ));
        }

        Ok(Self)
    }
}

impl Drop for RawInputOwner {
    fn drop(&mut self) {
        RAW_INPUT_OWNER.store(false, Ordering::Release);
    }
}

pub(crate) struct InputThread {
    hwnd: Arc<WindowHandleSlot>,
    thread: Option<JoinHandle<()>>,
}

impl InputThread {
    pub(crate) fn spawn(
        sender: mpsc::UnboundedSender<KeyboardEvent>,
        init_sender: oneshot::Sender<io::Result<Vec<Keyboard>>>,
    ) -> Self {
        let hwnd = Arc::new(WindowHandleSlot::new());
        let thread_hwnd = Arc::clone(&hwnd);

        let thread = thread::spawn(move || {
            run_input_thread(sender, thread_hwnd, init_sender);
        });

        Self {
            hwnd,
            thread: Some(thread),
        }
    }
}

impl Drop for InputThread {
    fn drop(&mut self) {
        self.hwnd.close();

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_input_thread(
    sender: mpsc::UnboundedSender<KeyboardEvent>,
    hwnd_slot: Arc<WindowHandleSlot>,
    init_sender: oneshot::Sender<io::Result<Vec<Keyboard>>>,
) {
    let result = initialize(sender, Arc::clone(&hwnd_slot));

    let (window, keyboards) = match result {
        Ok(value) => value,

        Err(error) => {
            let _ = init_sender.send(Err(error));
            return;
        }
    };

    // Initialization must be reported before entering the message loop.
    let _ = init_sender.send(Ok(keyboards));

    window.run_message_loop();

    hwnd_slot.clear();
}

fn initialize(
    sender: mpsc::UnboundedSender<KeyboardEvent>,
    hwnd_slot: Arc<WindowHandleSlot>,
) -> io::Result<(MessageOnlyWindow, Vec<Keyboard>)> {
    let instance = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .map(windows::Win32::Foundation::HINSTANCE::from)
            .map_err(windows_error)?
    };

    let devices = DeviceEnumerator::enumerate_keyboards()?;

    let keyboards = devices
        .iter()
        .map(|device| device.keyboard.clone())
        .collect::<Vec<_>>();

    let state = WindowState::new(sender, devices);

    let window = MessageOnlyWindow::create(instance, state).map_err(windows_error)?;

    hwnd_slot.store(window.hwnd());

    if let Err(error) = window.register_raw_input() {
        window.destroy();
        hwnd_slot.clear();

        return Err(error);
    }

    Ok((window, keyboards))
}
