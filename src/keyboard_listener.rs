use crate::interface::*;
use std::sync::Arc;
use std::{io, thread, time::Duration};

type Job = Arc<dyn Fn() + Send + Sync + 'static>;

pub struct KeyboardListener<D: InputDevice + Send + 'static> {
    device: D,
    on_pressed: Vec<Job>,
    on_plugged: Vec<Job>,
    on_unplugged: Vec<Job>,
}

impl<D: InputDevice + Send + 'static> KeyboardListener<D> {
    pub fn new(device: D) -> Self {
        Self {
            device,
            on_pressed: Vec::new(),
            on_plugged: Vec::new(),
            on_unplugged: Vec::new(),
        }
    }

    pub fn on_pressed<F: Fn() + Send + Sync + 'static>(&mut self, job: F) {
        self.on_pressed.push(Arc::new(job));
    }

    pub fn on_plugged<F: Fn() + Send + Sync + 'static>(&mut self, job: F) {
        self.on_plugged.push(Arc::new(job));
    }

    pub fn on_unplugged<F: Fn() + Send + Sync + 'static>(&mut self, job: F) {
        self.on_unplugged.push(Arc::new(job));
    }

    // TODO: pub fn drop_listener() {}

    pub fn start_listener(mut self) {
        // TODO: self.device.open();

        for job in &self.on_plugged {
            job();
        }

        thread::spawn(move || {
            loop {
                match self.device.fetch_events() {
                    Ok(events) => {
                        for event in events {
                            if event.is_key_event() {
                                for job in &self.on_pressed {
                                    job();
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let is_unplugged = matches!(e.raw_os_error(), Some(19) | Some(5))
                            || e.kind() == io::ErrorKind::NotFound;

                        if is_unplugged {
                            for job in &self.on_unplugged {
                                job();
                            }
                            break;
                        } else {
                            eprintln!("Read error: {} (raw: {:?})", e, e.raw_os_error());
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            }
        });
    }
}
