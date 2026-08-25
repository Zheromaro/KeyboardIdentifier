use crate::keyboard_provider::{DeviceProvider, Keyboard, KeyboardID, PortID, ProviderEvent};
use evdev::Device as EvdevDevice;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;
use udev::{Device as UdevDevice, Enumerator, EventType, MonitorBuilder};

pub struct LinuxDeviceProvider {
    receiver: mpsc::Receiver<Result<ProviderEvent, std::io::Error>>,
}

impl DeviceProvider for LinuxDeviceProvider {
    async fn new() -> Self {
        let (sender, receiver) = mpsc::channel(128);
        tokio::task::spawn_blocking(move || udev_loop(sender));
        Self { receiver }
    }

    async fn next_event(&mut self) -> Result<ProviderEvent, std::io::Error> {
        self.receiver.recv().await.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Linux device event channel closed",
            )
        })?
    }

    fn get_keyboards(&self) -> Vec<Keyboard> {
        let Ok(mut enumerator) = Enumerator::new() else {
            return Vec::new();
        };
        let Ok(_) = enumerator.match_subsystem("input") else {
            return Vec::new();
        };
        let Ok(devices) = enumerator.scan_devices() else {
            return Vec::new();
        };

        devices
            .filter_map(|dev| {
                if dev.property_value("ID_INPUT_KEYBOARD").is_none() {
                    return None;
                }
                map_to_keyboard(&dev)
            })
            .collect()
    }
}

fn udev_loop(sender: mpsc::Sender<Result<ProviderEvent, std::io::Error>>) {
    let monitor = match MonitorBuilder::new()
        .and_then(|b| b.match_subsystem("input"))
        .and_then(|b| b.listen())
    {
        Ok(m) => m,
        Err(e) => {
            let _ = sender.blocking_send(Err(e));
            return;
        }
    };

    let mut keyboards: HashMap<PathBuf, (Keyboard, tokio::task::JoinHandle<()>)> = HashMap::new();

    for event in monitor.iter() {
        let dev = event.device();

        if dev.property_value("ID_INPUT_KEYBOARD").is_none() {
            continue;
        }

        let Some(devnode) = dev.devnode().map(PathBuf::from) else {
            continue;
        };

        match event.event_type() {
            EventType::Add => {
                let Some(keyboard) = map_to_keyboard(&dev) else {
                    continue;
                };

                let event_sender = sender.clone();
                let evdev_keyboard = keyboard.clone();
                let evdev_devnode = devnode.clone();
                let handle = tokio::spawn(async move {
                    evdev_loop(evdev_devnode, evdev_keyboard, event_sender).await;
                });

                keyboards.insert(devnode.clone(), (keyboard.clone(), handle));

                if sender
                    .blocking_send(Ok(ProviderEvent::Plugged(keyboard)))
                    .is_err()
                {
                    break;
                }
            }

            EventType::Remove => {
                if let Some((keyboard, handle)) = keyboards.remove(&devnode) {
                    handle.abort();
                    if sender
                        .blocking_send(Ok(ProviderEvent::Unplugged(keyboard)))
                        .is_err()
                    {
                        break;
                    }
                }
            }

            _ => {}
        }
    }

    for (_, handle) in keyboards.into_values() {
        handle.abort();
    }
}

async fn evdev_loop(
    devnode: PathBuf,
    keyboard: Keyboard,
    sender: mpsc::Sender<Result<ProviderEvent, std::io::Error>>,
) {
    let device = match EvdevDevice::open(&devnode) {
        Ok(d) => d,
        Err(e) => {
            let _ = sender.send(Err(e)).await;
            return;
        }
    };

    let mut stream = match device.into_event_stream() {
        Ok(s) => s,
        Err(e) => {
            let _ = sender.send(Err(e)).await;
            return;
        }
    };

    while let Ok(event) = stream.next_event().await {
        if event.event_type() != evdev::EventType::KEY || event.value() != 1 {
            continue;
        }

        if sender
            .send(Ok(ProviderEvent::Pressed(keyboard.clone())))
            .await
            .is_err()
        {
            break;
        }
    }
}

fn map_to_keyboard(udev_dev: &UdevDevice) -> Option<Keyboard> {
    let devnode = udev_dev.devnode()?;
    let evdev = EvdevDevice::open(devnode).ok()?;

    let input_id = evdev.input_id();

    Some(Keyboard {
        keyboard_id: KeyboardID {
            name: evdev.name().map(String::from),
            vendor_id: Some(format!("{:04x}", input_id.vendor())),
            product_id: Some(format!("{:04x}", input_id.product())),
            serial: udev_dev
                .property_value("ID_SERIAL_SHORT")
                .and_then(|v| v.to_str().map(String::from)),
        },
        port_id: PortID {
            physical_path: udev_dev.syspath().to_str().map(String::from),
        },
    })
}
