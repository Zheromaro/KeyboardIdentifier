mod device;
mod key_mapping;
mod window;
use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource};
use device::DeviceEnumerator;
pub use keyboard_types::KeyboardEvent as KeyEvent;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::error;
use window::{MessageOnlyWindow, ThreadCommand, WindowHandleSlot, WindowState};

pub(crate) fn win32_error(message: &'static str) -> io::Error {
    let code = unsafe { windows::Win32::Foundation::GetLastError().0 as i32 };
    if code == 0 {
        io::Error::other(message)
    } else {
        io::Error::from_raw_os_error(code)
    }
}

pub(crate) fn windows_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error)
}

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
    cmd_tx: mpsc::Sender<ThreadCommand>,
    thread: Option<JoinHandle<()>>,
}

impl InputThread {
    pub(crate) fn spawn(
        sender: broadcast::Sender<KeyboardEvent>,
        init_sender: oneshot::Sender<io::Result<()>>,
    ) -> Self {
        let hwnd = Arc::new(WindowHandleSlot::new());
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let thread_hwnd = Arc::clone(&hwnd);

        let thread = thread::spawn(move || {
            let result = Self::initialize(sender, cmd_rx, Arc::clone(&thread_hwnd));
            match result {
                Ok(window) => {
                    let _ = init_sender.send(Ok(()));
                    window.run_message_loop();
                }
                Err(err) => {
                    let _ = init_sender.send(Err(err));
                }
            }
            thread_hwnd.clear();
        });

        Self {
            hwnd,
            cmd_tx,
            thread: Some(thread),
        }
    }

    fn initialize(
        sender: broadcast::Sender<KeyboardEvent>,
        cmd_rx: mpsc::Receiver<ThreadCommand>,
        hwnd_slot: Arc<WindowHandleSlot>,
    ) -> io::Result<MessageOnlyWindow> {
        let instance = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .map(windows::Win32::Foundation::HINSTANCE::from)
                .map_err(windows_error)?
        };
        let devices = DeviceEnumerator::enumerate_keyboards()?;
        let window = MessageOnlyWindow::create(
            instance,
            WindowState::new(sender, devices, cmd_rx, instance),
        )
        .map_err(windows_error)?;

        hwnd_slot.store(window.hwnd());
        if let Err(err) = window.register_raw_input() {
            window.destroy();
            hwnd_slot.clear();
            return Err(err);
        }
        Ok(window)
    }

    pub(crate) async fn send_command(
        &self,
        make_cmd: impl FnOnce(oneshot::Sender<io::Result<()>>) -> ThreadCommand,
    ) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        let cmd = make_cmd(tx);

        self.cmd_tx
            .send(cmd)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Input thread exited"))?;

        self.hwnd.notify();

        rx.await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Input thread exited"))?
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

pub struct WindowsKeyboardSource {
    event_tx: broadcast::Sender<KeyboardEvent>,
    _owner: RawInputOwner,
    thread: Option<InputThread>,
}

impl KeyboardSource for WindowsKeyboardSource {
    async fn new() -> io::Result<Self> {
        let owner = RawInputOwner::acquire()?;
        let (event_tx, _) = broadcast::channel(128);
        let (init_tx, init_rx) = oneshot::channel::<io::Result<()>>();
        let thread = InputThread::spawn(event_tx.clone(), init_tx);

        match init_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                drop(thread);
                return Err(err);
            }
            Err(_) => {
                drop(thread);
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Windows keyboard input thread exited during initialization",
                ));
            }
        }

        Ok(Self {
            event_tx,
            _owner: owner,
            thread: Some(thread),
        })
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        match DeviceEnumerator::enumerate_keyboards() {
            Ok(devices) => devices.into_iter().map(|d| d.keyboard).collect(),
            Err(err) => {
                error!(error = %err, "Failed to enumerate Windows keyboards");
                Vec::new()
            }
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.event_tx.subscribe()
    }

    async fn consume(&self, keyboard: &Keyboard) -> io::Result<()> {
        if let Some(thread) = &self.thread {
            let kb = keyboard.clone();
            thread
                .send_command(|tx| ThreadCommand::Consume(kb, tx))
                .await
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Input thread not running",
            ))
        }
    }

    async fn release(&self, keyboard: &Keyboard) -> io::Result<()> {
        if let Some(thread) = &self.thread {
            let kb = keyboard.clone();
            thread
                .send_command(|tx| ThreadCommand::Release(kb, tx))
                .await
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Input thread not running",
            ))
        }
    }
}

impl Drop for WindowsKeyboardSource {
    fn drop(&mut self) {
        self.thread.take();
    }
}
