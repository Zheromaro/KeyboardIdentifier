mod key_mapping;

use super::{Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID};
use evdev::{Device as EvdevDevice, KeyCode};
use key_mapping::*;
use keyboard_types::{KeyboardEvent as KeyEvent, Modifiers};
use std::{collections::HashMap, io, path::PathBuf, sync::Arc};
use tokio::{
    io::unix::AsyncFd,
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tracing::error;
use udev::{Device as UdevDevice, Enumerator, EventType, MonitorBuilder, MonitorSocket};

const ENODEV: i32 = 19;

pub struct LinuxKeyboardSource {
    receiver: mpsc::Receiver<Result<KeyboardEvent, io::Error>>,
    command_sender: mpsc::Sender<SourceCommand>,
    shutdown: broadcast::Sender<()>,
}

impl KeyboardSource for LinuxKeyboardSource {
    async fn new() -> io::Result<Self> {
        let monitor = AsyncFd::new(create_monitor()?)?;

        let (sender, receiver) = mpsc::channel(128);
        let (command_sender, command_receiver) = mpsc::channel(32);
        let (shutdown, _) = broadcast::channel(1);

        tokio::spawn(udev_loop(
            monitor,
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
        match enumerate_keyboards() {
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
}

impl Drop for LinuxKeyboardSource {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}

enum SourceCommand {
    Consume {
        keyboard: Keyboard,
        response_sender: oneshot::Sender<io::Result<()>>,
    },
    Release {
        keyboard: Keyboard,
        response_sender: oneshot::Sender<io::Result<()>>,
    },
}

enum DeviceCommandKind {
    Consume,
    Release,
}

impl DeviceCommandKind {
    fn into_command(self, response_sender: oneshot::Sender<io::Result<()>>) -> DeviceCommand {
        match self {
            Self::Consume => DeviceCommand::Consume(response_sender),
            Self::Release => DeviceCommand::Release(response_sender),
        }
    }
}

enum DeviceCommand {
    Consume(oneshot::Sender<io::Result<()>>),
    Release(oneshot::Sender<io::Result<()>>),
}

impl DeviceCommand {
    fn into_response_sender(self) -> oneshot::Sender<io::Result<()>> {
        match self {
            Self::Consume(response_sender) | Self::Release(response_sender) => response_sender,
        }
    }
}

struct ManagedKeyboard {
    keyboard: Arc<Keyboard>,
    command_sender: mpsc::Sender<DeviceCommand>,
    event_task: JoinHandle<()>,
}

async fn udev_loop(
    monitor: AsyncFd<MonitorSocket>,
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    mut command_receiver: mpsc::Receiver<SourceCommand>,
    mut shutdown: broadcast::Receiver<()>,
) {
    let mut keyboards: HashMap<PathBuf, ManagedKeyboard> = HashMap::new();

    // Channel to receive notifications when an evdev task exits prematurely
    let (device_exit_sender, mut device_exit_receiver) = mpsc::channel(32);

    match enumerate_keyboards() {
        Ok(initial_keyboards) => {
            for (devnode, keyboard) in initial_keyboards {
                spawn_evdev(
                    &mut keyboards,
                    devnode,
                    keyboard,
                    &sender,
                    device_exit_sender.clone(),
                );
            }
        }

        Err(error) => {
            if sender.send(Err(error)).await.is_err() {
                shutdown_evdev_tasks(keyboards);
                return;
            }
        }
    }

    loop {
        tokio::select! {
            biased;

            _ = shutdown.recv() => {
                break;
            }

            // Handle premature exit of an evdev task to prevent "ghost" devices
            Some(exited_devnode) = device_exit_receiver.recv() => {
                keyboards.remove(&exited_devnode);
            }

            command = command_receiver.recv() => {
                match command {
                    Some(command) => {
                        handle_source_command(command, &keyboards).await;
                    }

                    None => break,
                }
            }

            result = monitor.readable() => {
                let mut guard = match result {
                    Ok(guard) => guard,

                    Err(error) => {
                        let _ = sender.send(Err(error)).await;
                        break;
                    }
                };

                for event in guard.get_inner().iter() {
                    handle_udev_event(
                        event,
                        &mut keyboards,
                        &sender,
                        device_exit_sender.clone(),
                    )
                    .await;
                }

                guard.clear_ready();
            }
        }
    }

    shutdown_evdev_tasks(keyboards);
}

async fn handle_source_command(
    command: SourceCommand,
    keyboards: &HashMap<PathBuf, ManagedKeyboard>,
) {
    let (keyboard, response_sender, command_kind) = match command {
        SourceCommand::Consume {
            keyboard,
            response_sender,
        } => (keyboard, response_sender, DeviceCommandKind::Consume),

        SourceCommand::Release {
            keyboard,
            response_sender,
        } => (keyboard, response_sender, DeviceCommandKind::Release),
    };

    let Some(command_sender) = keyboards
        .values()
        .find(|managed| managed.keyboard.as_ref() == &keyboard)
        .map(|managed| managed.command_sender.clone())
    else {
        let _ = response_sender.send(Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("keyboard not found: {keyboard}"),
        )));

        return;
    };

    let device_command = command_kind.into_command(response_sender);

    if let Err(error) = command_sender.send(device_command).await {
        let response_sender = error.0.into_response_sender();

        let _ = response_sender.send(Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "keyboard event task stopped",
        )));
    }
}

fn shutdown_evdev_tasks(keyboards: HashMap<PathBuf, ManagedKeyboard>) {
    for (_, managed) in keyboards {
        managed.event_task.abort();
    }
}

fn create_monitor() -> io::Result<MonitorSocket> {
    MonitorBuilder::new()?.match_subsystem("input")?.listen()
}

async fn handle_udev_event(
    event: udev::Event,
    keyboards: &mut HashMap<PathBuf, ManagedKeyboard>,
    sender: &mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    device_exit_sender: mpsc::Sender<PathBuf>,
) {
    let dev = event.device();

    let Some(devnode) = dev.devnode().map(PathBuf::from) else {
        return;
    };

    match event.event_type() {
        EventType::Add => {
            if dev.property_value("ID_INPUT_KEYBOARD").is_none() {
                return;
            }

            let Some(keyboard) = map_to_keyboard(&dev).map(Arc::new) else {
                return;
            };

            if keyboards.contains_key(&devnode) {
                return;
            }

            if sender
                .send(Ok(KeyboardEvent::Plugged(keyboard.clone())))
                .await
                .is_err()
            {
                return;
            }

            spawn_evdev(keyboards, devnode, keyboard, sender, device_exit_sender);
        }

        EventType::Remove => {
            if let Some(managed) = keyboards.remove(&devnode) {
                managed.event_task.abort();

                let _ = sender
                    .send(Ok(KeyboardEvent::Unplugged(managed.keyboard)))
                    .await;
            }
        }

        _ => {}
    }
}

fn spawn_evdev(
    keyboards: &mut HashMap<PathBuf, ManagedKeyboard>,
    devnode: PathBuf,
    keyboard: Arc<Keyboard>,
    sender: &mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    device_exit_sender: mpsc::Sender<PathBuf>,
) {
    if keyboards.contains_key(&devnode) {
        return;
    }

    let sender = sender.clone();
    let keyboard_for_task = keyboard.clone();
    let devnode_for_task = devnode.clone();
    let (command_sender, command_receiver) = mpsc::channel(8);

    let event_task = tokio::spawn(async move {
        evdev_loop(
            devnode_for_task,
            keyboard_for_task,
            sender,
            command_receiver,
            device_exit_sender,
        )
        .await;
    });

    keyboards.insert(
        devnode,
        ManagedKeyboard {
            keyboard,
            command_sender,
            event_task,
        },
    );
}

async fn evdev_loop(
    devnode: PathBuf,
    keyboard: Arc<Keyboard>,
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    mut command_receiver: mpsc::Receiver<DeviceCommand>,
    device_exit_sender: mpsc::Sender<PathBuf>,
) {
    let mut stream = match EvdevDevice::open(&devnode).and_then(|device| device.into_event_stream())
    {
        Ok(stream) => stream,

        Err(error) => {
            let _ = sender.send(Err(error)).await;
            let _ = device_exit_sender.send(devnode.clone()).await;
            return;
        }
    };

    let mut modifiers = Modifiers::empty();

    loop {
        let event = tokio::select! {
            biased;

            command = command_receiver.recv() => {
                match command {
                    Some(DeviceCommand::Consume(response_sender)) => {
                        let _ = response_sender.send(stream.device_mut().grab());
                        continue;
                    }

                    Some(DeviceCommand::Release(response_sender)) => {
                        let _ = response_sender.send(stream.device_mut().ungrab());
                        continue;
                    }

                    None => break,
                }
            }

            result = stream.next_event() => {
                match result {
                    Ok(event) => event,

                    Err(error) => {
                        if error.raw_os_error() != Some(ENODEV) {
                            error!(
                                error = %error,
                                devnode = ?devnode,
                                "evdev error",
                            );
                        }

                        break;
                    }
                }
            }
        };

        if event.event_type() != evdev::EventType::KEY {
            continue;
        }

        let key_code = evdev::KeyCode::new(event.code());

        let Some((state, repeat)) = evdev_to_key_state(event.value()) else {
            continue;
        };

        let modifier = modifier_for_key(key_code);

        let is_lock_key = matches!(
            key_code,
            KeyCode::KEY_CAPSLOCK | KeyCode::KEY_NUMLOCK | KeyCode::KEY_SCROLLLOCK
        );

        let event_modifiers = match state {
            keyboard_types::KeyState::Down => {
                if let Some(modifier) = modifier {
                    if is_lock_key && !repeat {
                        modifiers.toggle(modifier); // Toggle only on initial press
                    } else if !is_lock_key {
                        modifiers.insert(modifier);
                    }
                }
                modifiers
            }
            keyboard_types::KeyState::Up => {
                if !is_lock_key {
                    if let Some(modifier) = modifier {
                        modifiers.remove(modifier);
                    }
                }
                modifiers
            }
        };

        let key_event = KeyEvent {
            state,
            key: evdev_to_key(key_code),
            code: evdev_to_code(key_code),
            location: evdev_to_location(key_code),
            modifiers: event_modifiers,
            repeat,
            is_composing: false,
        };

        if sender
            .send(Ok(KeyboardEvent::KeyAction(keyboard.clone(), key_event)))
            .await
            .is_err()
        {
            break;
        }

        // REMOVED: The redundant modifier removal block that broke CapsLock/NumLock
    }

    // Notify the main loop that this device task has terminated so it can be cleaned up
    let _ = device_exit_sender.send(devnode).await;
}

fn enumerate_keyboards() -> Result<Vec<(PathBuf, Arc<Keyboard>)>, io::Error> {
    let mut enumerator = Enumerator::new()?;

    enumerator.match_subsystem("input")?;

    Ok(enumerator
        .scan_devices()?
        .filter(|device| device.property_value("ID_INPUT_KEYBOARD").is_some())
        .filter_map(|device| {
            let devnode = device.devnode().map(PathBuf::from)?;

            let keyboard = Arc::new(map_to_keyboard(&device)?);

            Some((devnode, keyboard))
        })
        .collect())
}

fn map_to_keyboard(udev_dev: &UdevDevice) -> Option<Keyboard> {
    let devnode = udev_dev.devnode()?;

    let evdev = match EvdevDevice::open(devnode) {
        Ok(device) => device,

        Err(error) => {
            error!(
                error = %error,
                devnode = ?devnode,
                "failed open evdev device error",
            );

            return None;
        }
    };

    let input_id = evdev.input_id();

    let name = udev_dev
        .property_value("ID_MODEL_FROM_DATABASE")
        .or_else(|| udev_dev.property_value("ID_MODEL"))
        .and_then(|value| value.to_str().map(str::to_owned))
        .or_else(|| evdev.name().map(str::to_owned));

    let serial = udev_dev
        .property_value("ID_SERIAL_SHORT")
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("noserial"))
        .map(str::to_owned);

    let physical_path = udev_dev
        .property_value("ID_PATH")
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .or_else(|| udev_dev.syspath().to_str().map(str::to_owned));

    Some(Keyboard {
        keyboard_id: KeyboardID {
            name,
            vendor_id: Some(format!("{:04x}", input_id.vendor())),
            product_id: Some(format!("{:04x}", input_id.product())),
            serial,
        },

        port_id: PortID { physical_path },
    })
}
