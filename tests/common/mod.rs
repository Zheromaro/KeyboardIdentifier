#![allow(dead_code)]
use keyboard_identifier::keyboard_source::{
    KeyEvent, Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID,
};
use keyboard_types::{Code, Key, KeyState};
use std::time::Duration;
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::timeout;

#[derive(Clone)]
pub struct MockDeviceSource {
    devices: Arc<Mutex<Vec<Arc<Keyboard>>>>,
    next_id: Arc<AtomicUsize>,
    tx: UnboundedSender<KeyboardEvent>,
    rx: Arc<AsyncMutex<UnboundedReceiver<KeyboardEvent>>>,
}

impl MockDeviceSource {
    pub fn plug_keyboard(&self) -> Keyboard {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let keyboard = Arc::new(Keyboard {
            keyboard_id: KeyboardID {
                name: Some(format!("Mock Keyboard {id}")),
                vendor_id: Some("MOCK".into()),
                product_id: Some(format!("{id:04}")),
                serial: Some(format!("MOCK-SERIAL-{id}")),
            },
            port_id: PortID {
                physical_path: Some(format!("/mock/keyboard/{id}")),
            },
        });

        self.devices.lock().unwrap().push(keyboard.clone());
        let _ = self.tx.send(KeyboardEvent::Plugged(keyboard.clone()));

        (*keyboard).clone()
    }

    pub fn unplug_keyboard(&self, keyboard: &Keyboard) {
        let mut devices = self.devices.lock().unwrap();
        if let Some(pos) = devices.iter().position(|dev| **dev == *keyboard) {
            let keyboard_arc = devices.remove(pos);
            let _ = self.tx.send(KeyboardEvent::Unplugged(keyboard_arc));
        }
    }

    /// Simulates a key press (keydown) on the given keyboard.
    pub fn press(&self, keyboard: &Keyboard) {
        self.press_key(keyboard, Key::Character("a".into()), Code::KeyA);
    }

    /// Simulates a key release (keyup) on the given keyboard.
    pub fn release(&self, keyboard: &Keyboard) {
        let mut event = key_event(Key::Character("a".into()), Code::KeyA);
        event.state = KeyState::Up;
        self.send_pressed(keyboard, event);
    }

    /// Simulates a key press with an explicit logical key and physical code.
    pub fn press_key(&self, keyboard: &Keyboard, key: Key, code: Code) {
        self.send_pressed(keyboard, key_event(key, code));
    }

    fn send_pressed(&self, keyboard: &Keyboard, event: KeyEvent) {
        let devices = self.devices.lock().unwrap();
        if let Some(dev) = devices.iter().find(|dev| ***dev == *keyboard) {
            let _ = self.tx.send(KeyboardEvent::KeyAction(dev.clone(), event));
        }
    }
}

fn key_event(key: Key, code: Code) -> KeyEvent {
    KeyEvent {
        state: KeyState::Down,
        key,
        code,
        ..Default::default()
    }
}

impl KeyboardSource for MockDeviceSource {
    async fn new() -> io::Result<Self> {
        let (tx, rx) = unbounded_channel();
        Ok(Self {
            devices: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicUsize::new(0)),
            tx,
            rx: Arc::new(AsyncMutex::new(rx)),
        })
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        self.devices
            .lock()
            .unwrap()
            .iter()
            .map(|k| (**k).clone())
            .collect()
    }

    async fn receive_event(&mut self) -> Result<KeyboardEvent, std::io::Error> {
        self.rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| std::io::Error::other("channel closed"))
    }
}

// ==== helpers ====
pub async fn expect_recv<T>(rx: &mut tokio::sync::mpsc::UnboundedReceiver<T>) -> T {
    timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("Timed out waiting for event")
        .expect("Channel closed unexpectedly")
}

pub async fn expect_timeout<T>(rx: &mut tokio::sync::mpsc::UnboundedReceiver<T>) {
    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(
        result.is_err(),
        "Expected timeout, but received an unexpected event"
    );
}
