mod device;
mod key_mapping;
mod monitor;

use super::{Keyboard, KeyboardEvent, KeyboardSource};
use std::io;
use tokio::sync::{broadcast, mpsc, oneshot};

/// Commands sent from the public API to the udev monitor loop.
pub(crate) enum SourceCommand {
    Consume {
        keyboard: Keyboard,
        response_sender: oneshot::Sender<io::Result<()>>,
    },
    Release {
        keyboard: Keyboard,
        response_sender: oneshot::Sender<io::Result<()>>,
    },
}

pub struct LinuxKeyboardSource {
    receiver: mpsc::Receiver<Result<KeyboardEvent, io::Error>>,
    command_sender: mpsc::Sender<SourceCommand>,
    shutdown: broadcast::Sender<()>,
}

impl KeyboardSource for LinuxKeyboardSource {
    async fn new() -> io::Result<Self> {
        let monitor = monitor::create_monitor()?;
        let async_monitor = tokio::io::unix::AsyncFd::new(monitor)?;

        let (sender, receiver) = mpsc::channel(128);
        let (command_sender, command_receiver) = mpsc::channel(32);
        let (shutdown, _) = broadcast::channel(1);

        tokio::spawn(monitor::udev_loop(
            async_monitor,
            sender,
            command_receiver,
            shutdown.subscribe(),
        ));

        Ok(Self {
            receiver,
            command_sender,
            shutdown,
        })
    }

    async fn receive_event(&mut self) -> Result<KeyboardEvent, io::Error> {
        match self.receiver.recv().await {
            Some(result) => result,
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Linux device event channel closed",
            )),
        }
    }

    async fn consume(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        let (response_sender, response_receiver) = oneshot::channel();

        self.command_sender
            .send(SourceCommand::Consume {
                keyboard: keyboard.clone(),
                response_sender,
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "keyboard source stopped"))?;

        response_receiver
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "keyboard task stopped"))?
    }

    async fn release(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        let (response_sender, response_receiver) = oneshot::channel();

        self.command_sender
            .send(SourceCommand::Release {
                keyboard: keyboard.clone(),
                response_sender,
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "keyboard source stopped"))?;

        response_receiver
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "keyboard task stopped"))?
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        match monitor::enumerate_keyboards() {
            Ok(keyboards) => keyboards
                .into_iter()
                .map(|(_, keyboard)| (*keyboard).clone())
                .collect(),
            Err(error) => {
                tracing::error!(error = %error, "Failed to enumerate Linux keyboards");
                Vec::new()
            }
        }
    }
}

impl Drop for LinuxKeyboardSource {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}
