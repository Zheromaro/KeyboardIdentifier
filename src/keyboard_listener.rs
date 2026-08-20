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
    ports: Registry<Port>,
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

    pub fn listen_to_keyboard<D: KeyboardDevice + Send + 'static>(&self, mut keyboard: D) {
        if !keyboard.is_plugged() {
            eprintln!(
                "keyboard Identifier Warning: listen_to_keyboard() called with unplugged keyboard: {}",
                keyboard.id().as_str()
            );
            return;
        }

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
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.recv() => {
                        keyboards.unregister(id);
                        break;
                    }
                    result = keyboard.fetch_events() => {
                        match result {
                            Ok(()) => {
                                on_pressed.for_each(|cb| (&**cb)());
                            }
                            Err(e) => {
                                eprintln!("keyboard Identifier Error: {e}");
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn listen_to_port<P: DeviceProvider + Send + 'static>(&self, port: Port, mut provider: P) {
        let ports = self.ports.clone();
        let id = match ports
            .register_unique(port.physical_path.as_ref().unwrap().clone(), port.clone())
        {
            Some(id) => id,
            None => {
                eprintln!(
                    "keyboard Identifier Warning: listen_to_port() already called with port: {}",
                    &port.physical_path.unwrap()
                );
                return;
            }
        };

        let on_plugged = self.on_plugged.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown.recv() => {
                        ports.unregister(id);
                        break;
                    }
                    result = provider.plugged_event() => {
                        match result {
                            Ok((_keyboard_id, plugged_port)) => {
                                if plugged_port == port {
                                    on_plugged.for_each(|cb| (&**cb)());
                                }
                            }
                            Err(e) => {
                                eprintln!("keyboard Identifier Error: {e}");
                                tokio::time::sleep(Duration::from_millis(100)).await;
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
