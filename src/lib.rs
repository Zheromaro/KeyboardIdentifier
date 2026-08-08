pub mod components;
use components::*;
use std::{io, path::PathBuf, thread, time::Duration};

fn listen_to_keyboard<D: InputDevice>(job: impl Fn(), mut device: D) {
    loop {
        match device.fetch_events() {
            Ok(events) => {
                for event in events {
                    if event.is_key_event() {
                        job();
                    }
                }
            }
            Err(e) => {
                let is_unplugged = || {
                    matches!(e.raw_os_error(), Some(19) | Some(5))
                        || e.kind() == io::ErrorKind::NotFound
                };

                if is_unplugged() {
                    eprintln!("[UNPLUGED]");
                    break;
                } else {
                    eprintln!("Read error: {} (raw: {:?})", e, e.raw_os_error());
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
}

// public API
pub fn get_keyboard_list<S: DeviceSource>(source: &S) -> Vec<PathBuf> {
    source
        .enumerate()
        .into_iter()
        .filter(|(_, dev)| dev.is_keyboard())
        .map(|(path, _)| path)
        .collect()
}

pub fn on_keyboard_pressed<S: DeviceSource>(
    job: impl Fn() + Send + 'static,
    source: &S,
    keyboard_path: &PathBuf,
) where
    S::Device: Send + 'static,
{
    let device = match source.open(keyboard_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("keyboard_identifier Error: {e}, shutting down the listener");
            return;
        }
    };
    std::thread::spawn(move || {
        listen_to_keyboard(job, device);
    });
}
