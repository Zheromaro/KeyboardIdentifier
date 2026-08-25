use crate::keyboard_provider::*;
use crate::registry::*;
use std::sync::Arc;
use tokio::sync::broadcast;

pub type Callback = Arc<dyn Fn(&Keyboard) + Send + Sync + 'static>;

pub struct KeyboardListener {
    on_pressed: Registry<Callback>,
    on_plugged: Registry<Callback>,
    on_unplugged: Registry<Callback>,
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
            shutdown,
        }
    }

    pub fn on_plugged<F: Fn(&Keyboard) + Send + Sync + 'static>(&self, callback: F) {
        self.on_plugged.register(Arc::new(callback));
    }

    pub fn on_unplugged<F: Fn(&Keyboard) + Send + Sync + 'static>(&self, callback: F) {
        self.on_unplugged.register(Arc::new(callback));
    }

    pub fn on_pressed<F: Fn(&Keyboard) + Send + Sync + 'static>(&self, callback: F) {
        self.on_pressed.register(Arc::new(callback));
    }

    pub fn listen<D: DeviceProvider + Send + 'static>(&self, mut provider: D) {
        let on_pressed = self.on_pressed.clone();
        let on_plugged = self.on_plugged.clone();
        let on_unplugged = self.on_unplugged.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased; // Prioritize shutdown signals
                    _ = shutdown.recv() => break,
                    Ok(event) = provider.next_event() => {
                        match event {
                            ProviderEvent::Plugged(kb) => {
                                on_plugged.for_each(|cb| cb(&kb));
                            }
                            ProviderEvent::Unplugged(kb) => {
                                on_unplugged.for_each(|cb| cb(&kb));
                            }
                            ProviderEvent::Pressed(kb) => {
                                on_pressed.for_each(|cb| cb(&kb));
                            }                        }
                    }
                }
            }
        });
    }
}

impl Drop for KeyboardListener {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}
