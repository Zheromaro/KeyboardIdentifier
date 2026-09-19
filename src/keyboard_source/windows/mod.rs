mod device;
mod key_mapping;
mod win32_props;

use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource};
use device::{DeviceEnumerator, InterceptionState};
use interception::{FilterKeyState, Interception, Stroke};
pub use keyboard_types::KeyboardEvent as KeyEvent;
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    thread::{self, JoinHandle},
};
use tokio::sync::mpsc;
use tracing::error;

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
                "Another interception instance is already active",
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

pub struct WindowsKeyboardSource {
    events: mpsc::Receiver<Result<KeyboardEvent, io::Error>>,
    _owner: InterceptionOwner,
    thread: Option<JoinHandle<()>>,
}

impl KeyboardSource for WindowsKeyboardSource {
    async fn new() -> io::Result<Self> {
        let owner = InterceptionOwner::acquire()?;
        let (event_tx, event_rx) = mpsc::channel(128);

        let thread = thread::spawn(move || {
            let context = match Interception::new() {
                Some(ctx) => ctx,
                None => {
                    let _ = event_tx.blocking_send(Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Failed to initialize Interception context. Is the driver installed?",
                    )));
                    return;
                }
            };

            // Filter only keyboard events
            context.set_filter(context.is_keyboard(), FilterKeyState::All.into());

            let mut state = InterceptionState::new(event_tx, &context);

            // Blocking loop: Wait for intercepted hardware events
            while let Some((device, stroke)) = context.wait() {
                if context.is_keyboard(device) {
                    state.handle_input(&context, device, &stroke);
                }

                // CRITICAL: We must forward the stroke back to the OS.
                // If we don't, the user's keyboard input is swallowed entirely.
                context.send(device, &[stroke]);
            }
        });

        Ok(Self {
            events: event_rx,
            _owner: owner,
            thread: Some(thread),
        })
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        let context = match Interception::new() {
            Some(ctx) => ctx,
            None => {
                error!("Failed to initialize Interception context for enumeration.");
                return Vec::new();
            }
        };
        DeviceEnumerator::enumerate_keyboards(&context)
            .into_iter()
            .map(|d| d.keyboard)
            .collect()
    }

    async fn receive_event(&mut self) -> io::Result<KeyboardEvent> {
        match self.events.recv().await {
            Some(result) => result,
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Interception keyboard input thread exited",
            )),
        }
    }
}

impl Drop for WindowsKeyboardSource {
    fn drop(&mut self) {
        // Interception blocking loops are difficult to cleanly terminate
        // without sending a dummy keystroke. We allow the thread to detach.
        self.thread.take();
    }
}
