#![allow(dead_code)]
use crate::components::{DeviceSource, InputDevice, InputEvent};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

// ==== MockEvent ====
#[derive(Clone)]
pub struct MockEvent {
    pub is_key: bool,
}

impl InputEvent for MockEvent {
    fn is_key_event(&self) -> bool {
        self.is_key
    }
}

// ==== MockDevice ====
#[derive(Clone)]
pub struct MockDevice {
    pub name: String,
    pub keyboard: bool,
    pub buffer: Arc<Mutex<Vec<MockEvent>>>,
}

impl InputDevice for MockDevice {
    type Event = MockEvent;

    fn is_keyboard(&self) -> bool {
        self.keyboard
    }

    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    fn fetch_events(&mut self) -> Result<Vec<MockEvent>, std::io::Error> {
        let mut buf = self.buffer.lock().unwrap();
        if buf.is_empty() {
            // Release lock before sleeping so press() can write
            drop(buf);
            std::thread::sleep(std::time::Duration::from_millis(10));
            Ok(vec![])
        } else {
            Ok(std::mem::take(&mut *buf))
        }
    }
}

// ==== MockDeviceSource ====
pub struct MockDeviceSource {
    devices: RefCell<HashMap<PathBuf, MockDevice>>,
}

impl MockDeviceSource {
    pub fn new() -> Self {
        Self {
            devices: RefCell::new(HashMap::new()),
        }
    }

    pub fn plug_keyboard(&self) -> PathBuf {
        let index = self.devices.borrow().len();
        let path = PathBuf::from(format!("/dev/input/mock{}", index));
        let dev = MockDevice {
            name: format!("Mock Keyboard {}", index),
            keyboard: true,
            buffer: Arc::new(Mutex::new(vec![])),
        };
        self.devices.borrow_mut().insert(path.clone(), dev);
        path
    }

    pub fn unplug_keyboard(&self) {
        let path = { self.devices.borrow().keys().last().cloned() };

        if let Some(path) = path {
            self.devices.borrow_mut().remove(&path);
        }
    }

    pub fn press(&self, path: &PathBuf) {
        if let Some(dev) = self.devices.borrow().get(path) {
            dev.buffer.lock().unwrap().push(MockEvent { is_key: true });
        }
    }
}

impl DeviceSource for MockDeviceSource {
    type Device = MockDevice;

    fn enumerate(&self) -> Vec<(PathBuf, MockDevice)> {
        self.devices
            .borrow()
            .iter()
            .map(|(p, d)| (p.clone(), d.clone()))
            .collect()
    }

    fn open(&self, path: &PathBuf) -> Result<MockDevice, std::io::Error> {
        self.devices
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such device"))
    }
}
