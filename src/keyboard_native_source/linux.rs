use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID};
use evdev::Device as EvdevDevice;
use std::{collections::HashMap, io, path::PathBuf};
use tokio::{
    io::unix::AsyncFd,
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use udev::{Device as UdevDevice, Enumerator, EventType, MonitorBuilder, MonitorSocket};

const ENODEV: i32 = 19;
const PRESSED: i32 = 1;

pub struct LinuxKeyboardSource {
    receiver: mpsc::Receiver<Result<KeyboardEvent, io::Error>>,
    shutdown: broadcast::Sender<()>,
    task: JoinHandle<()>,
}

impl KeyboardSource for LinuxKeyboardSource {
    async fn new() -> Self {
        let (sender, receiver) = mpsc::channel(128);
        let (shutdown, _) = broadcast::channel(1);

        let task = tokio::spawn(udev_loop(sender, shutdown.subscribe()));

        Self {
            receiver,
            shutdown,
            task,
        }
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

    fn get_keyboards(&self) -> Vec<Keyboard> {
        match enumerate_keyboards() {
            Ok(keyboards) => keyboards
                .into_iter()
                .map(|(_, keyboard)| keyboard)
                .collect(),

            Err(error) => {
                eprintln!("Failed to enumerate Linux keyboards: {error}");
                Vec::new()
            }
        }
    }
}

impl Drop for LinuxKeyboardSource {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());

        self.task.abort();
    }
}

async fn udev_loop(
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    mut shutdown: broadcast::Receiver<()>,
) {
    let monitor = match create_monitor().and_then(AsyncFd::new) {
        Ok(monitor) => monitor,
        Err(error) => {
            let _ = sender.send(Err(error)).await;
            return;
        }
    };

    let mut keyboards: HashMap<PathBuf, (Keyboard, JoinHandle<()>)> = HashMap::new();

    match enumerate_keyboards() {
        Ok(initial_keyboards) => {
            for (devnode, keyboard) in initial_keyboards {
                spawn_evdev(&mut keyboards, devnode, keyboard, &sender);
            }
        }

        Err(error) => {
            if sender.send(Err(error)).await.is_err() {
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

            result = monitor.readable() => {
                let mut guard = match result {
                    Ok(guard) => guard,

                    Err(error) => {
                        let _ = sender.send(Err(error)).await;
                        break;
                    }
                };

                for event in guard.get_inner().iter() {
                    if !handle_udev_event(
                        event,
                        &mut keyboards,
                        &sender,
                    ).await {
                        break;
                    }
                }

                guard.clear_ready();
            }
        }
    }

    for (_, handle) in keyboards.into_values() {
        handle.abort();
    }
}

fn create_monitor() -> io::Result<MonitorSocket> {
    MonitorBuilder::new()?.match_subsystem("input")?.listen()
}

async fn handle_udev_event(
    event: udev::Event,
    keyboards: &mut HashMap<PathBuf, (Keyboard, JoinHandle<()>)>,
    sender: &mpsc::Sender<Result<KeyboardEvent, io::Error>>,
) -> bool {
    let dev = event.device();

    let Some(devnode) = dev.devnode().map(PathBuf::from) else {
        return true;
    };

    match event.event_type() {
        EventType::Add => {
            if dev.property_value("ID_INPUT_KEYBOARD").is_none() {
                return true;
            }

            let Some(keyboard) = map_to_keyboard(&dev) else {
                return true;
            };

            if keyboards.contains_key(&devnode) {
                return true;
            }

            spawn_evdev(keyboards, devnode, keyboard.clone(), sender);

            sender
                .send(Ok(KeyboardEvent::Plugged(keyboard)))
                .await
                .is_ok()
        }

        EventType::Remove => {
            if let Some((keyboard, handle)) = keyboards.remove(&devnode) {
                handle.abort();

                return sender
                    .send(Ok(KeyboardEvent::Unplugged(keyboard)))
                    .await
                    .is_ok();
            }

            true
        }

        _ => true,
    }
}

fn spawn_evdev(
    keyboards: &mut HashMap<PathBuf, (Keyboard, JoinHandle<()>)>,
    devnode: PathBuf,
    keyboard: Keyboard,
    sender: &mpsc::Sender<Result<KeyboardEvent, io::Error>>,
) {
    if keyboards.contains_key(&devnode) {
        return;
    }

    let sender = sender.clone();
    let keyboard_for_task = keyboard.clone();
    let devnode_for_task = devnode.clone();

    let handle = tokio::spawn(async move {
        evdev_loop(devnode_for_task, keyboard_for_task, sender).await;
    });

    keyboards.insert(devnode, (keyboard, handle));
}

async fn evdev_loop(
    devnode: PathBuf,
    keyboard: Keyboard,
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
) {
    let mut stream = match EvdevDevice::open(&devnode).and_then(|device| device.into_event_stream())
    {
        Ok(stream) => stream,

        Err(error) => {
            let _ = sender.send(Err(error)).await;
            return;
        }
    };

    loop {
        let event = match stream.next_event().await {
            Ok(event) => event,

            Err(error) => {
                if error.raw_os_error() != Some(ENODEV) {
                    eprintln!("evdev error for {devnode:?}: {error}");
                }

                break;
            }
        };

        if event.event_type() != evdev::EventType::KEY || event.value() != PRESSED {
            continue;
        }

        if sender
            .send(Ok(KeyboardEvent::Pressed(keyboard.clone())))
            .await
            .is_err()
        {
            break;
        }
    }
}

fn enumerate_keyboards() -> Result<Vec<(PathBuf, Keyboard)>, io::Error> {
    let mut enumerator = Enumerator::new()?;

    enumerator.match_subsystem("input")?;

    Ok(enumerator
        .scan_devices()?
        .filter(|device| device.property_value("ID_INPUT_KEYBOARD").is_some())
        .filter_map(|device| {
            let devnode = device.devnode().map(PathBuf::from)?;

            let keyboard = map_to_keyboard(&device)?;

            Some((devnode, keyboard))
        })
        .collect())
}

fn map_to_keyboard(udev_dev: &UdevDevice) -> Option<Keyboard> {
    let devnode = udev_dev.devnode()?;

    let evdev = match EvdevDevice::open(devnode) {
        Ok(device) => device,

        Err(error) => {
            eprintln!("Failed to open evdev device at {devnode:?}: {error}");

            return None;
        }
    };

    let input_id = evdev.input_id();

    let name = udev_dev
        .property_value("ID_MODEL_FROM_DATABASE")
        .or_else(|| udev_dev.property_value("ID_MODEL"))
        .and_then(|value| value.to_str().map(String::from))
        .or_else(|| evdev.name().map(String::from));

    let serial = udev_dev
        .property_value("ID_SERIAL_SHORT")
        .or_else(|| udev_dev.property_value("ID_SERIAL"))
        .and_then(|value| {
            let serial = value.to_str()?;

            if serial.is_empty() || serial.eq_ignore_ascii_case("noserial") {
                None
            } else {
                Some(serial.to_string())
            }
        });

    Some(Keyboard {
        keyboard_id: KeyboardID {
            name,

            vendor_id: Some(format!("{:04x}", input_id.vendor())),

            product_id: Some(format!("{:04x}", input_id.product())),

            serial,
        },

        port_id: PortID {
            physical_path: udev_dev.syspath().to_str().map(String::from),
        },
    })
}
