#![allow(dead_code)]
use keyboard_identifier::keyboard_source::{
    Access, KeyEvent, Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID,
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
use tokio::sync::broadcast;
use tokio::time::timeout;

#[derive(Clone)]
pub struct MockKeyboardSource {
    devices: Arc<Mutex<Vec<Arc<Keyboard>>>>,
    consumed_devices: Arc<Mutex<Vec<Keyboard>>>,
    pressed_keys: Arc<Mutex<Vec<(Keyboard, Key, Code)>>>,
    next_id: Arc<AtomicUsize>,
    tx: broadcast::Sender<KeyboardEvent>,
}

impl MockKeyboardSource {
    pub fn plug_keyboard(&self) -> Arc<Keyboard> {
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
            access: Access::Shared,
        });

        self.devices.lock().unwrap().push(keyboard.clone());
        let _ = self.tx.send(KeyboardEvent::Plugged(keyboard.clone()));

        keyboard
    }

    pub fn unplug_keyboard(&self, keyboard: &Keyboard) {
        let mut devices = self.devices.lock().unwrap();
        if let Some(pos) = devices.iter().position(|dev| **dev == *keyboard) {
            let keyboard_arc = devices.remove(pos);

            // Clean up active states on unplug
            self.consumed_devices
                .lock()
                .unwrap()
                .retain(|k| k != keyboard);
            self.pressed_keys
                .lock()
                .unwrap()
                .retain(|(k, _, _)| k != keyboard);

            let _ = self.tx.send(KeyboardEvent::Unplugged(keyboard_arc));
        }
    }

    /// Simulates pressing key 'a' on the given keyboard.
    pub fn press_a(&self, keyboard: &Keyboard) {
        self.press_key(keyboard, Key::Character("a".into()), Code::KeyA);
    }

    /// Simulates releasing key 'a' on the given keyboard.
    pub fn release_a(&self, keyboard: &Keyboard) {
        self.release_key(keyboard, Key::Character("a".into()), Code::KeyA);
    }

    /// Simulates a key press (keydown) with an explicit logical key and physical code.
    pub fn press_key(&self, keyboard: &Keyboard, key: Key, code: Code) {
        let mut pressed = self.pressed_keys.lock().unwrap();
        if !pressed
            .iter()
            .any(|(k, k_key, k_code)| k == keyboard && k_key == &key && *k_code == code)
        {
            pressed.push((keyboard.clone(), key.clone(), code));
        }
        drop(pressed);

        let event = KeyEvent {
            state: KeyState::Down,
            key,
            code,
            ..Default::default()
        };
        self.send_key_action(keyboard, event);
    }

    /// Simulates a key release (keyup) with an explicit logical key and physical code.
    pub fn release_key(&self, keyboard: &Keyboard, key: Key, code: Code) {
        let mut pressed = self.pressed_keys.lock().unwrap();
        pressed.retain(|(k, k_key, k_code)| !(k == keyboard && k_key == &key && *k_code == code));
        drop(pressed);

        let event = KeyEvent {
            state: KeyState::Up,
            key,
            code,
            ..Default::default()
        };
        self.send_key_action(keyboard, event);
    }

    fn send_key_action(&self, keyboard: &Keyboard, event: KeyEvent) {
        let devices = self.devices.lock().unwrap();
        if let Some(dev) = devices.iter().find(|&dev| **dev == *keyboard) {
            let _ = self.tx.send(KeyboardEvent::KeyAction(dev.clone(), event));
        }
    }

    /// Returns a snapshot of all currently pressed keys.
    pub fn get_pressed_keys(&self) -> Vec<(Keyboard, Key, Code)> {
        self.pressed_keys.lock().unwrap().clone()
    }

    /// Checks if a specific key is currently held down on a keyboard.
    pub fn is_key_pressed(&self, keyboard: &Keyboard, key: &Key, code: Code) -> bool {
        self.pressed_keys
            .lock()
            .unwrap()
            .iter()
            .any(|(k, k_key, k_code)| k == keyboard && k_key == key && *k_code == code)
    }

    /// Helper for tests to assert if a keyboard was successfully consumed.
    pub fn is_consumed(&self, keyboard: &Keyboard) -> bool {
        self.consumed_devices.lock().unwrap().contains(keyboard)
    }
}

impl KeyboardSource for MockKeyboardSource {
    async fn new() -> io::Result<Self> {
        let (tx, _rx) = broadcast::channel(1024);
        Ok(Self {
            devices: Arc::new(Mutex::new(Vec::new())),
            consumed_devices: Arc::new(Mutex::new(Vec::new())),
            pressed_keys: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicUsize::new(0)),
            tx,
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

    fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.tx.subscribe()
    }

    async fn consume(&self, keyboard: &Keyboard) -> io::Result<()> {
        let mut consumed = self.consumed_devices.lock().unwrap();
        if !consumed.contains(keyboard) {
            consumed.push(keyboard.clone());
        }
        Ok(())
    }

    async fn release(&self, keyboard: &Keyboard) -> io::Result<()> {
        let mut consumed = self.consumed_devices.lock().unwrap();
        consumed.retain(|k| k != keyboard);
        Ok(())
    }
}

// ==== helpers ====
pub async fn expect_recv<T: Clone>(rx: &mut tokio::sync::broadcast::Receiver<T>) -> T {
    timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("Timed out waiting for event")
        .expect("Channel closed unexpectedly")
}

pub async fn expect_timeout<T: Clone>(rx: &mut tokio::sync::broadcast::Receiver<T>) {
    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(
        result.is_err(),
        "Expected timeout, but received an unexpected event"
    );
}
