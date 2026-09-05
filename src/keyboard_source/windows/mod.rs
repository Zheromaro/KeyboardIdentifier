mod enumerator;
mod errors;
mod handles;
mod hid;
mod input_thread;
mod keyboard_state;
mod path_parser;
mod raw_input;
mod setup_api;
mod window;
use super::{Keyboard, KeyboardEvent, KeyboardSource};
use input_thread::{InputThread, RawInputOwner};
use std::io;
use tokio::sync::{mpsc, oneshot};

pub struct WindowsKeyboardSource {
    events: mpsc::UnboundedReceiver<KeyboardEvent>,
    keyboards: Vec<Keyboard>,
    _owner: RawInputOwner,
    thread: Option<InputThread>,
}

impl KeyboardSource for WindowsKeyboardSource {
    async fn new() -> io::Result<Self> {
        let owner = RawInputOwner::acquire()?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (init_tx, init_rx) = oneshot::channel::<io::Result<Vec<Keyboard>>>();

        let thread = InputThread::spawn(event_tx, init_tx);

        let keyboards = match init_rx.await {
            Ok(Ok(keyboards)) => keyboards,

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
        };

        Ok(Self {
            events: event_rx,
            keyboards,
            _owner: owner,
            thread: Some(thread),
        })
    }

    fn get_keyboards(&self) -> Vec<Keyboard> {
        self.keyboards.clone()
    }

    async fn receive_event(&mut self) -> io::Result<KeyboardEvent> {
        let event = self.events.recv().await.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Windows keyboard input thread exited",
            )
        })?;

        self.update_keyboards(&event);

        Ok(event)
    }
}

impl WindowsKeyboardSource {
    fn update_keyboards(&mut self, event: &KeyboardEvent) {
        match event {
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
    }
}

impl Drop for WindowsKeyboardSource {
    fn drop(&mut self) {
        self.thread.take();
    }
}
