use super::{
    Command, Keyboard, KeyboardEvent, KeyboardTask,
    key_mapping::{
        evdev_to_code, evdev_to_key, evdev_to_key_state, evdev_to_location, modifier_for_key,
    },
};
use evdev::{Device as EvdevDevice, KeyCode};
use keyboard_types::{KeyboardEvent as KeyEvent, Modifiers};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, mpsc};
use tracing::error;

const ENODEV: i32 = 19;

pub(crate) fn spawn_evdev(
    keyboards: &mut HashMap<PathBuf, KeyboardTask>,
    devnode: PathBuf,
    keyboard: Arc<Keyboard>,
    sender: &broadcast::Sender<KeyboardEvent>,
) {
    if keyboards.contains_key(&devnode) {
        return;
    }
    let sender = sender.clone();
    let devnode_for_task = devnode.clone();
    let (ctrl_tx, ctrl_rx) = mpsc::channel(4);
    let keyboard_for_task = Arc::clone(&keyboard);

    let handle = tokio::spawn(async move {
        evdev_loop(devnode_for_task, keyboard_for_task, sender, ctrl_rx).await;
    });

    keyboards.insert(devnode, (keyboard, handle, ctrl_tx));
}

async fn evdev_loop(
    devnode: PathBuf,
    mut keyboard: Arc<Keyboard>,
    sender: broadcast::Sender<KeyboardEvent>,
    mut ctrl_rx: mpsc::Receiver<Command>,
) {
    let mut stream = match EvdevDevice::open(&devnode).and_then(|device| device.into_event_stream())
    {
        Ok(stream) => stream,
        Err(error) => {
            error!(
                error = %error,
                devnode = ?devnode,
                "Failed to open evdev device",
            );
            return;
        }
    };

    let mut modifiers = Modifiers::empty();

    loop {
        tokio::select! {
            biased;
            cmd = ctrl_rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Command::Consume(new_kb, reply) => {
                        keyboard = new_kb;
                        let _ = reply.send(stream.device_mut().grab());
                    }
                    Command::Release(new_kb, reply) => {
                        keyboard = new_kb;
                        let _ = reply.send(stream.device_mut().ungrab());
                    }
                    Command::Shutdown => break,
                }
            }
            event_res = stream.next_event() => {
                let event = match event_res {
                    Ok(event) => event,
                    Err(error) => {
                        if error.raw_os_error() != Some(ENODEV) {
                            error!(
                                error = %error,
                                devnode = ?devnode,
                                "evdev read error",
                            );
                        }
                        break;
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
                        if let Some(modifier) = modifier {
                            if is_lock_key && !repeat {
                                modifiers.toggle(modifier);
                            } else if !is_lock_key {
                                modifiers.insert(modifier);
                            }
                        }
                        modifiers
                    }
                    keyboard_types::KeyState::Up => {
                        if !is_lock_key && let Some(modifier) = modifier {
                            modifiers.remove(modifier);
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

                // No active receivers is not an error — just means nobody is listening.
                if sender
                    .send(KeyboardEvent::KeyAction(keyboard.clone(), key_event))
                    .is_err()
                {
                    // All receivers dropped; keep running so we maintain device state.
                }
            }
        }
    }
}
