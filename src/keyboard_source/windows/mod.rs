mod device;
mod key_mapping;
mod window;

use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource};
use std::{
    io,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{mpsc, oneshot};
use window::InputThread;

pub(crate) fn win32_error(message: &'static str) -> io::Error {
    let code = unsafe { windows::Win32::Foundation::GetLastError().0 as i32 };
    if code == 0 {
        io::Error::other(message)
    } else {
        io::Error::from_raw_os_error(code)
    }
}

static INTERCEPTION_OWNER: AtomicBool = AtomicBool::new(false);

pub(crate) struct InterceptionOwner;

impl InterceptionOwner {
    pub(crate) fn acquire() -> io::Result<Self> {
        if INTERCEPTION_OWNER
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

impl Drop for InterceptionOwner {
    fn drop(&mut self) {
        INTERCEPTION_OWNER.store(false, Ordering::Release);
    }
}

pub(crate) enum InterceptionCommand {
    Consume {
        keyboard: Keyboard,
        reply: oneshot::Sender<io::Result<()>>,
    },
    Release {
        keyboard: Keyboard,
        reply: oneshot::Sender<io::Result<()>>,
    },
    Stop,
}

pub struct WindowsKeyboardSource {
    events: mpsc::Receiver<Result<KeyboardEvent, io::Error>>,
    _owner: InterceptionOwner,
    thread: Option<InputThread>,
    keyboards: Arc<RwLock<Vec<Keyboard>>>,
}

impl KeyboardSource for WindowsKeyboardSource {
    async fn new() -> io::Result<Self> {
        let owner = InterceptionOwner::acquire()?;
        let (event_tx, event_rx) = mpsc::channel(256);
        let (init_tx, init_rx) = oneshot::channel::<io::Result<()>>();
        let keyboards = Arc::new(RwLock::new(Vec::new()));
        let thread = InputThread::spawn(event_tx, Arc::clone(&keyboards), init_tx);

        match init_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                drop(thread);
                return Err(error);
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
            events: event_rx,
            _owner: owner,
            thread: Some(thread),
            keyboards,
        })
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        // FIXED: Recover from poisoned lock instead of silently returning empty vector
        self.keyboards
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    async fn receive_event(&mut self) -> io::Result<KeyboardEvent> {
        match self.events.recv().await {
            Some(result) => result,
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Windows keyboard input thread exited",
            )),
        }
    }

    async fn consume(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        let thread = self.thread.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Windows keyboard input thread exited",
            )
        })?;
        let (reply_tx, reply_rx) = oneshot::channel();
        thread.send_command(InterceptionCommand::Consume {
            keyboard: keyboard.clone(),
            reply: reply_tx,
        })?;
        reply_rx.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Windows keyboard input thread exited",
            )
        })?
    }

    async fn release(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        let thread = self.thread.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Windows keyboard input thread exited",
            )
        })?;
        let (reply_tx, reply_rx) = oneshot::channel();
        thread.send_command(InterceptionCommand::Release {
            keyboard: keyboard.clone(),
            reply: reply_tx,
        })?;
        reply_rx.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Windows keyboard input thread exited",
            )
        })?
    }
}

impl Drop for WindowsKeyboardSource {
    fn drop(&mut self) {
        self.thread.take();
    }
}
