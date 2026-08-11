#![allow(dead_code)]
use crate::interface::{DeviceSource, InputDevice, InputEvent};
use std::{
    cell::{Cell, RefCell},
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

    fn equal(&self, other: &Self) -> bool {
        self.name == other.name
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
    devices: RefCell<Vec<MockDevice>>,
    next_id: Cell<usize>,
}

impl MockDeviceSource {
    pub fn new() -> Self {
        Self {
            devices: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
        }
    }

    pub fn plug_keyboard(&self) -> MockDevice {
        let id = self.next_id.get();
        self.next_id.set(id + 1);

        let dev = MockDevice {
            name: format!("Mock Keyboard {}", id),
            keyboard: true,
            buffer: Arc::new(Mutex::new(vec![])),
        };
        self.devices.borrow_mut().push(dev.clone());
        dev
    }

    pub fn unplug_keyboard(&self, device: &MockDevice) {
        let mut devices = self.devices.borrow_mut();

        if let Some(index) = devices.iter().position(|dev| device.equal(dev)) {
            devices.remove(index);
        }
    }

    pub fn press(&self, device: &MockDevice) {
        device
            .buffer
            .lock()
            .unwrap()
            .push(MockEvent { is_key: true });
    }
}

impl DeviceSource for MockDeviceSource {
    type Device = MockDevice;

    fn get_keyboards(&self) -> Vec<Self::Device> {
        self.devices
            .borrow()
            .iter()
            .map(|dev| dev.clone())
            .collect()
    }
}
