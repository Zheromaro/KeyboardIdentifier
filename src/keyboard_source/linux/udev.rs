use super::{
    Access, Command, KeyboardEvent, KeyboardTask,
    enumerate::{enumerate_keyboards, map_to_keyboard},
    evdev::spawn_evdev,
};
use std::{collections::HashMap, io, path::PathBuf, sync::Arc};
use tokio::{
    io::unix::AsyncFd,
    sync::{broadcast, mpsc},
};
use tracing::error;
use udev::{EventType, MonitorBuilder, MonitorSocket};

pub(crate) async fn udev_loop(
    monitor: AsyncFd<MonitorSocket>,
    event_tx: broadcast::Sender<KeyboardEvent>,
    mut cmd_rx: mpsc::Receiver<Command>,
) {
    let mut keyboards: HashMap<PathBuf, KeyboardTask> = HashMap::new();

    match enumerate_keyboards() {
        Ok(initial_keyboards) => {
            for (devnode, keyboard) in initial_keyboards {
                spawn_evdev(&mut keyboards, devnode, keyboard, &event_tx);
            }
        }
        Err(error) => {
            error!(error = %error, "Failed to enumerate initial keyboards");
        }
    }

    loop {
        tokio::select! {
            biased;
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Command::Shutdown => break,
                    Command::Consume(kb, reply) => {
                        if let Some((k_arc, _, tx)) = keyboards.values_mut().find(|(k, _, _)| **k == *kb) {
                            let mut new_kb = (**k_arc).clone();
                            new_kb.access = Access::Exclusive;
                            let new_arc = Arc::new(new_kb);
                            *k_arc = new_arc.clone();
                            let _ = tx.send(Command::Consume(new_arc, reply)).await;
                        } else {
                            let _ = reply.send(Err(io::Error::new(
                                io::ErrorKind::NotFound,
                                "keyboard not found",
                            )));
                        }
                    }
                    Command::Release(kb, reply) => {
                        if let Some((k_arc, _, tx)) = keyboards.values_mut().find(|(k, _, _)| **k == *kb) {
                            let mut new_kb = (**k_arc).clone();
                            new_kb.access = Access::Shared;
                            let new_arc = Arc::new(new_kb);
                            *k_arc = new_arc.clone();
                            let _ = tx.send(Command::Release(new_arc, reply)).await;
                        } else {
                            let _ = reply.send(Err(io::Error::new(
                                io::ErrorKind::NotFound,
                                "keyboard not found",
                            )));
                        }
                    }
                }
            }
            result = monitor.readable() => {
                let mut guard = match result {
                    Ok(guard) => guard,
                    Err(error) => {
                        error!(error = %error, "udev monitor read error");
                        break;
                    }
                };
                for event in guard.get_inner().iter() {
                    handle_udev_event(event, &mut keyboards, &event_tx).await;
                }
                guard.clear_ready();
            }
        }
    }
    shutdown_evdev_tasks(keyboards);
}

pub(crate) fn create_monitor() -> io::Result<MonitorSocket> {
    MonitorBuilder::new()?.match_subsystem("input")?.listen()
}

async fn handle_udev_event(
    event: udev::Event,
    keyboards: &mut HashMap<PathBuf, KeyboardTask>,
    sender: &broadcast::Sender<KeyboardEvent>,
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
            // broadcast::send returns Err only when there are no active receivers,
            // which is fine — we still want to track the device internally.
            let _ = sender.send(KeyboardEvent::Plugged(keyboard.clone()));
            spawn_evdev(keyboards, devnode, keyboard, sender);
        }
        EventType::Remove => {
            if let Some((keyboard, handle, _)) = keyboards.remove(&devnode) {
                handle.abort();
                let _ = sender.send(KeyboardEvent::Unplugged(keyboard));
            }
        }
        _ => {}
    }
}

pub(crate) fn shutdown_evdev_tasks(keyboards: HashMap<PathBuf, KeyboardTask>) {
    for (_, (_, handle, _)) in keyboards {
        handle.abort();
    }
}
