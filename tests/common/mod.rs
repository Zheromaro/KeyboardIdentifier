#![allow(dead_code)]
use keyboard_identifier::keyboard_provider::{DeviceProvider, Keyboard, KeyboardID, PortID};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone, Debug)]
pub struct MockDevice {
    pub id: KeyboardID,
    pub port: PortID,
    pub pressed: Arc<AtomicUsize>,
}

impl Keyboard for MockDevice {
    fn id(&self) -> KeyboardID {
        self.id.clone()
    }

    fn port(&self) -> PortID {
        self.port.clone()
    }

    async fn fetch_events(&mut self) -> Result<(), std::io::Error> {
        loop {
            if self.pressed.load(Ordering::SeqCst) > 0 {
                self.pressed.fetch_sub(1, Ordering::SeqCst);
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

pub struct MockDeviceSource {
    devices: Mutex<Vec<MockDevice>>,
    next_id: AtomicUsize,
    tx: UnboundedSender<(KeyboardID, PortID)>,
    rx: UnboundedReceiver<(KeyboardID, PortID)>,
}

impl MockDeviceSource {
    pub fn new() -> Self {
        let (tx, rx) = unbounded_channel();
        Self {
            devices: Mutex::new(Vec::new()),
            next_id: AtomicUsize::new(0),
            tx,
            rx,
        }
    }

    pub fn plug_keyboard(&self) -> MockDevice {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let device = MockDevice {
            id: KeyboardID {
                name: Some(format!("Mock Keyboard {id}")),
                vendor_id: Some("MOCK".into()),
                product_id: Some(format!("{id:04}")),
                serial: Some(format!("MOCK-SERIAL-{id}")),
            },
            port: PortID {
                physical_path: Some(format!("/mock/keyboard/{id}")),
            },
            pressed: Arc::new(AtomicUsize::new(0)),
        };

        // Store the device and notify the plugged listener
        self.devices.lock().unwrap().push(device.clone());
        let _ = self.tx.send((device.id.clone(), device.port.clone()));

        device
    }

    pub fn unplug_keyboard(&self, device: &MockDevice) {
        self.devices
            .lock()
            .unwrap()
            .retain(|dev| dev.id != device.id);
    }

    pub fn press(&self, device: &MockDevice) {
        if let Some(dev) = self
            .devices
            .lock()
            .unwrap()
            .iter_mut()
            .find(|dev| dev.id == device.id)
        {
            dev.pressed.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl Default for MockDeviceSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceProvider for MockDeviceSource {
    type Device = MockDevice;

    fn get_keyboards(&self) -> Vec<Self::Device> {
        self.devices.lock().unwrap().iter().cloned().collect()
    }

    fn get_ports(&self) -> Vec<PortID> {
        self.devices
            .lock()
            .unwrap()
            .iter()
            .map(|dev| dev.port.clone())
            .collect()
    }

    async fn plugged(&mut self) -> Result<(KeyboardID, PortID), std::io::Error> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "channel closed"))
    }
}
