use super::key_mapping::{
    evdev_to_code, evdev_to_key, evdev_to_key_state, evdev_to_location, modifier_for_key,
};
use crate::keyboard_source::{Keyboard, KeyboardEvent};
use evdev::{Device as EvdevDevice, KeyCode};
use keyboard_types::{KeyboardEvent as KeyEvent, Modifiers};
use std::{io, path::PathBuf, sync::Arc};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

const ENODEV: i32 = 19;

pub(crate) enum DeviceCommand {
    Consume(oneshot::Sender<io::Result<()>>),
    Release(oneshot::Sender<io::Result<()>>),
}

pub(crate) struct ManagedKeyboard {
    pub(crate) keyboard: Arc<Keyboard>,
    pub(crate) command_sender: mpsc::Sender<DeviceCommand>,
    pub(crate) event_task: JoinHandle<()>,
}

pub(crate) fn spawn_evdev(
    keyboards: &mut std::collections::HashMap<PathBuf, ManagedKeyboard>,
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

pub(crate) async fn evdev_loop(
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
                            tracing::error!(error = %error, devnode = ?devnode, "evdev error");
                        }
                        break;
                    }
                }
            }
        };

        if event.event_type() != evdev::EventType::KEY {
            continue;
        }

        let key_code = KeyCode::new(event.code());
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
                if let Some(m) = modifier {
                    if is_lock_key && !repeat {
                        modifiers.toggle(m);
                    } else if !is_lock_key {
                        modifiers.insert(m);
                    }
                }
                modifiers
            }
            keyboard_types::KeyState::Up => {
                if !is_lock_key && let Some(m) = modifier {
                    modifiers.remove(m);
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
    }

    // Notify the monitor loop that this device task has terminated
    let _ = device_exit_sender.send(devnode).await;
}
