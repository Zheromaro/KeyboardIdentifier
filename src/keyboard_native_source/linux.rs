use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID};
use evdev::Device as EvdevDevice;
use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use tokio::io::unix::AsyncFd;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use udev::{Device as UdevDevice, Enumerator, EventType, MonitorBuilder, MonitorSocket};

struct SendMonitorSocket(MonitorSocket);

unsafe impl Send for SendMonitorSocket {}
unsafe impl Sync for SendMonitorSocket {}

impl AsRawFd for SendMonitorSocket {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_raw_fd()
    }
}

pub struct LinuxKeyboardSource {
    receiver: mpsc::Receiver<Result<KeyboardEvent, std::io::Error>>,
}

impl KeyboardSource for LinuxKeyboardSource {
    async fn new() -> Self {
        let (sender, receiver) = mpsc::channel(128);
        tokio::spawn(udev_loop(sender));
        Self { receiver }
    }

    async fn receive_event(&mut self) -> Result<KeyboardEvent, std::io::Error> {
        self.receiver.recv().await.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Linux device event channel closed",
            )
        })?
    }

    fn get_keyboards(&self) -> Vec<Keyboard> {
        enumerate_keyboards()
            .map(|v| v.into_iter().map(|(_, kb)| kb).collect())
            .unwrap_or_default()
    }
}

async fn udev_loop(sender: mpsc::Sender<Result<KeyboardEvent, std::io::Error>>) {
    let monitor_result = MonitorBuilder::new()
        .and_then(|b| b.match_subsystem("input"))
        .and_then(|b| b.listen())
        .map(SendMonitorSocket);

    let monitor = match monitor_result {
        Ok(m) => m,
        Err(e) => {
            let _ = sender.send(Err(e)).await;
            return;
        }
    };

    let mut keyboards: HashMap<PathBuf, (Keyboard, JoinHandle<()>)> = HashMap::new();

    if let Ok(initial) = enumerate_keyboards() {
        for (devnode, keyboard) in initial {
            spawn_evdev(&mut keyboards, devnode, keyboard, &sender);
        }
    }

    let async_monitor = match AsyncFd::new(monitor) {
        Ok(m) => m,
        Err(e) => {
            let _ = sender.send(Err(e)).await;
            return;
        }
    };

    'udev: loop {
        let mut guard = match async_monitor.readable().await {
            Ok(guard) => guard,
            Err(e) => {
                let _ = sender.try_send(Err(e));
                break 'udev;
            }
        };

        for event in guard.get_inner().0.iter() {
            let dev = event.device();

            if dev.property_value("ID_INPUT_KEYBOARD").is_none() {
                continue;
            }

            let Some(devnode) = dev.devnode().map(PathBuf::from) else {
                continue;
            };

            match event.event_type() {
                EventType::Add => {
                    if let Some(keyboard) = map_to_keyboard(&dev) {
                        spawn_evdev(&mut keyboards, devnode, keyboard.clone(), &sender);

                        if sender
                            .try_send(Ok(KeyboardEvent::Plugged(keyboard)))
                            .is_err()
                        {
                            if sender.is_closed() {
                                break 'udev;
                            }
                        }
                    }
                }

                EventType::Remove => {
                    if let Some((keyboard, handle)) = keyboards.remove(&devnode) {
                        handle.abort();
                        if sender
                            .try_send(Ok(KeyboardEvent::Unplugged(keyboard)))
                            .is_err()
                        {
                            if sender.is_closed() {
                                break 'udev;
                            }
                        }
                    }
                }

                _ => {}
            }
        }

        guard.clear_ready();
    }

    for (_, handle) in keyboards.into_values() {
        handle.abort();
    }
}

fn spawn_evdev(
    keyboards: &mut HashMap<PathBuf, (Keyboard, JoinHandle<()>)>,
    devnode: PathBuf,
    keyboard: Keyboard,
    sender: &mpsc::Sender<Result<KeyboardEvent, std::io::Error>>,
) {
    if keyboards.contains_key(&devnode) {
        return;
    }

    let tx = sender.clone();
    let kb = keyboard.clone();
    let devnode_clone = devnode.clone();
    let handle = tokio::spawn(async move {
        evdev_loop(devnode_clone, kb, tx).await;
    });

    keyboards.insert(devnode, (keyboard, handle));
}

async fn evdev_loop(
    devnode: PathBuf,
    keyboard: Keyboard,
    sender: mpsc::Sender<Result<KeyboardEvent, std::io::Error>>,
) {
    let mut stream = match EvdevDevice::open(&devnode).and_then(|d| d.into_event_stream()) {
        Ok(s) => s,
        Err(e) => {
            let _ = sender.send(Err(e)).await;
            return;
        }
    };

    loop {
        let event = match stream.next_event().await {
            Ok(e) => e,
            Err(e) => {
                if e.raw_os_error() != Some(19) {
                    eprintln!("evdev error for {devnode:?}: {e}");
                }
                break;
            }
        };

        if event.event_type() != evdev::EventType::KEY || event.value() != 1 {
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

fn enumerate_keyboards() -> Result<Vec<(PathBuf, Keyboard)>, std::io::Error> {
    let mut enumerator = Enumerator::new()?;
    enumerator.match_subsystem("input")?;

    Ok(enumerator
        .scan_devices()?
        .filter(|dev| dev.property_value("ID_INPUT_KEYBOARD").is_some())
        .filter_map(|dev| {
            let devnode = dev.devnode().map(PathBuf::from)?;
            let keyboard = map_to_keyboard(&dev)?;
            Some((devnode, keyboard))
        })
        .collect())
}

fn map_to_keyboard(udev_dev: &UdevDevice) -> Option<Keyboard> {
    let devnode = udev_dev.devnode()?;

    let evdev = match EvdevDevice::open(devnode) {
        Ok(dev) => dev,
        Err(e) => {
            eprintln!("Failed to open evdev device at {devnode:?}: {e}");
            return None;
        }
    };

    let input_id = evdev.input_id();

    let name = udev_dev
        .property_value("ID_MODEL_FROM_DATABASE")
        .or_else(|| udev_dev.property_value("ID_MODEL"))
        .and_then(|v| v.to_str().map(String::from))
        .or_else(|| evdev.name().map(String::from));

    Some(Keyboard {
        keyboard_id: KeyboardID {
            name,
            vendor_id: Some(format!("{:04x}", input_id.vendor())),
            product_id: Some(format!("{:04x}", input_id.product())),
            serial: udev_dev
                .property_value("ID_SERIAL_SHORT")
                .or_else(|| udev_dev.property_value("ID_SERIAL"))
                .and_then(|v| {
                    let s = v.to_str()?;
                    // Treat udev's "noserial" placeholder the same as a missing serial
                    if s.eq_ignore_ascii_case("noserial") || s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                }),
        },
        port_id: PortID {
            physical_path: udev_dev.syspath().to_str().map(String::from),
        },
    })
}
