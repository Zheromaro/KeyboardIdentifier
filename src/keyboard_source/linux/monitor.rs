use super::SourceCommand;
use super::device::{ManagedKeyboard, spawn_evdev};
use crate::keyboard_source::{Keyboard, KeyboardEvent};
use std::{collections::HashMap, io, path::PathBuf, sync::Arc};
use tokio::{
    io::unix::AsyncFd,
    sync::{broadcast, mpsc},
};
use udev::{Device as UdevDevice, Enumerator, EventType, MonitorBuilder, MonitorSocket};

pub(crate) fn create_monitor() -> io::Result<MonitorSocket> {
    MonitorBuilder::new()?.match_subsystem("input")?.listen()
}

pub(crate) fn enumerate_keyboards() -> Result<Vec<(PathBuf, Arc<Keyboard>)>, io::Error> {
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

pub(crate) fn map_to_keyboard(udev_dev: &UdevDevice) -> Option<Keyboard> {
    let devnode = udev_dev.devnode()?;
    let evdev = match evdev::Device::open(devnode) {
        Ok(device) => device,
        Err(error) => {
            tracing::error!(error = %error, devnode = ?devnode, "failed to open evdev device");
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
        keyboard_id: crate::keyboard_source::KeyboardID {
            name,
            vendor_id: Some(format!("{:04x}", input_id.vendor())),
            product_id: Some(format!("{:04x}", input_id.product())),
            serial,
        },
        port_id: crate::keyboard_source::PortID { physical_path },
    })
}

pub(crate) async fn udev_loop(
    monitor: AsyncFd<MonitorSocket>,
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    mut command_receiver: mpsc::Receiver<SourceCommand>,
    mut shutdown: broadcast::Receiver<()>,
) {
    let mut keyboards: HashMap<PathBuf, ManagedKeyboard> = HashMap::new();
    let (device_exit_sender, mut device_exit_receiver) = mpsc::channel(32);

    if let Ok(initial_keyboards) = enumerate_keyboards() {
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

    loop {
        tokio::select! {
            biased;

            _ = shutdown.recv() => break,

            Some(exited_devnode) = device_exit_receiver.recv() => {
                keyboards.remove(&exited_devnode);
            }

            command = command_receiver.recv() => {
                match command {
                    Some(cmd) => handle_source_command(cmd, &keyboards).await,
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
                    handle_udev_event(event, &mut keyboards, &sender, device_exit_sender.clone()).await;
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
    use super::device::DeviceCommand;

    let (keyboard, response_sender, is_consume) = match command {
        SourceCommand::Consume {
            keyboard,
            response_sender,
        } => (keyboard, response_sender, true),
        SourceCommand::Release {
            keyboard,
            response_sender,
        } => (keyboard, response_sender, false),
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

    let device_command = if is_consume {
        DeviceCommand::Consume(response_sender)
    } else {
        DeviceCommand::Release(response_sender)
    };

    if let Err(err) = command_sender.send(device_command).await {
        let response_sender = match err.0 {
            DeviceCommand::Consume(s) | DeviceCommand::Release(s) => s,
        };

        let _ = response_sender.send(Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "keyboard event task stopped",
        )));
    }
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

fn shutdown_evdev_tasks(keyboards: HashMap<PathBuf, ManagedKeyboard>) {
    for (_, managed) in keyboards {
        managed.event_task.abort();
    }
}
