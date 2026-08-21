use crate::keyboard_provider::*;
use crate::registry::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

pub type Callback = Arc<dyn Fn() + Send + Sync + 'static>;

pub struct KeyboardListener {
    on_pressed: Registry<Callback>,
    on_plugged: Registry<Callback>,
    on_unplugged: Registry<Callback>,
    keyboards: Registry<KeyboardID>,
    ports: Registry<PortID>,
    shutdown: broadcast::Sender<()>,
}

impl Default for KeyboardListener {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardListener {
    pub fn new() -> Self {
        let (shutdown, _) = broadcast::channel(1);
        Self {
            on_pressed: Registry::new(),
            on_plugged: Registry::new(),
            on_unplugged: Registry::new(),
            keyboards: Registry::new(),
            ports: Registry::new(),
            shutdown,
        }
    }

    pub fn listen_to_keyboard<D: Keyboard + Send + 'static>(&self, mut keyboard: D) {
        let keyboard_id = keyboard.id();
        let keyboards = self.keyboards.clone();

        let id = match keyboards.register_unique(keyboard.id().as_str(), keyboard_id) {
            Some(id) => id,
            None => {
                eprintln!(
                    "keyboard Identifier Warning: listen_to_keyboard() already called with keyboard: {}",
                    keyboard.id().as_str()
                );
                return;
            }
        };

        let on_pressed = self.on_pressed.clone();
        let on_plugged = self.on_plugged.clone();
        let on_unplugged = self.on_unplugged.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.recv() => {
                        keyboards.unregister(id);
                        break;
                    }
                    result = keyboard.next_event() => {
                        match result {
                            Ok(ProviderEvent::Plugged{ keyboard_id, port }) => {
                                on_plugged.for_each(|cb| (&**cb)());
                            }

                            Ok(ProviderEvent::Unplugged{ keyboard_id, port }) => {
                                on_unplugged.for_each(|cb| (&**cb)());

                                keyboards.unregister(id);
                                break;
                            }

                            Ok(ProviderEvent::Pressed{ keyboard_id, port }) => {
                                on_pressed.for_each(|cb| (&**cb)());
                            }

                            Err(e) => {
                                eprintln!("keyboard Identifier Error: {e}");

                                tokio::time::sleep(
                                    Duration::from_millis(100)
                                ).await;
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn listen_to_port<P: Port + Send + 'static>(&self, mut port: P) {
        let port_id = port.id();
        let ports = self.ports.clone();

        let id = match ports.register_unique(port.id().as_str(), port_id) {
            Some(id) => id,
            None => {
                eprintln!(
                    "keyboard Identifier Warning: listen_to_keyboard() already called with keyboard: {}",
                    port.id().as_str()
                );
                return;
            }
        };

        let on_pressed = self.on_pressed.clone();
        let on_plugged = self.on_plugged.clone();
        let on_unplugged = self.on_unplugged.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.recv() => {
                        ports.unregister(id);
                        break;
                    }
                    result = port.next_event() => {
                        match result {
                            Ok(ProviderEvent::Plugged{ keyboard_id, port }) => {
                                on_plugged.for_each(|cb| (&**cb)());
                            }
                            Ok(ProviderEvent::Unplugged{ keyboard_id, port }) => {
                                on_unplugged.for_each(|cb| (&**cb)());
                                ports.unregister(id);
                                break;
                            }
                            Ok(ProviderEvent::Pressed{ keyboard_id, port }) => {
                                on_pressed.for_each(|cb| (&**cb)());
                            }
                            Err(e) => {
                                eprintln!("keyboard Identifier Error: {e}");

                                tokio::time::sleep(
                                    Duration::from_millis(100)
                                ).await;
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn on_plugged<F: Fn() + Send + Sync + 'static>(&self, callback: F) {
        self.on_plugged.register(Arc::new(callback));
    }

    pub fn on_unplugged<F: Fn() + Send + Sync + 'static>(&self, callback: F) {
        self.on_unplugged.register(Arc::new(callback));
    }

    pub fn on_pressed<F: Fn() + Send + Sync + 'static>(&self, callback: F) {
        self.on_pressed.register(Arc::new(callback));
    }
}

impl Drop for KeyboardListener {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}
