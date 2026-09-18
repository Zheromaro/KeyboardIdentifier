mod enumerate;
mod evdev;
mod key_mapping;
mod udev;

use super::{Access, Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID};
use std::{io, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tracing::error;

pub(crate) type KeyboardTask = (Arc<Keyboard>, JoinHandle<()>, mpsc::Sender<Command>);

pub(crate) enum Command {
    Consume(Arc<Keyboard>, oneshot::Sender<io::Result<()>>),
    Release(Arc<Keyboard>, oneshot::Sender<io::Result<()>>),
    Shutdown,
}

pub struct LinuxKeyboardSource {
    broadcast: broadcast::Sender<KeyboardEvent>,
    sender: mpsc::Sender<Command>,
}

impl KeyboardSource for LinuxKeyboardSource {
    async fn new() -> io::Result<Self> {
        let monitor = tokio::io::unix::AsyncFd::new(udev::create_monitor()?)?;
        let (broadcast, _) = broadcast::channel(128);
        let (sender, rx) = mpsc::channel(32);
        tokio::spawn(udev::udev_loop(monitor, broadcast.clone(), rx));
        Ok(Self { broadcast, sender })
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        match enumerate::enumerate_keyboards() {
            Ok(keyboards) => keyboards
                .into_iter()
                .map(|(_, keyboard)| (*keyboard).clone())
                .collect(),
            Err(error) => {
                error!(
                    error = %error,
                    "Failed to enumerate Linux keyboards"
                );
                Vec::new()
            }
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.broadcast.subscribe()
    }

    async fn consume(&self, keyboard: &Keyboard) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::Consume(Arc::new(keyboard.clone()), tx))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::NotConnected, "udev loop stopped"))?;
        rx.await.unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "evdev task dead",
            ))
        })
    }

    async fn release(&self, keyboard: &Keyboard) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::Release(Arc::new(keyboard.clone()), tx))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::NotConnected, "udev loop stopped"))?;
        rx.await.unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "evdev task dead",
            ))
        })
    }
}

impl Drop for LinuxKeyboardSource {
    fn drop(&mut self) {
        let _ = self.sender.try_send(Command::Shutdown);
    }
}
