use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource, NativeKeyboardSource};
use keyboard_types::KeyboardEvent as KeyEvent;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tracing::warn;

type DeviceCallback = Arc<dyn Fn(&Keyboard) + Send + Sync + 'static>;
type KeyActionCallback = Arc<dyn Fn(&Keyboard, &KeyEvent) + Send + Sync + 'static>;

/// The main manager for tracking keyboards and listening to their events.
///
/// `KeyboardManager` maintains a list of active keyboards and allows you to
/// subscribe to a stream of events including key actions, device plugged,
/// and device unplugged.
///
/// Callback-based methods (`on_key_action`, `on_plugged`, and `on_unplugged`)
/// are provided for basics usage.
///
/// # Type Parameters
///
/// * `P`: The underlying [`KeyboardSource`] implementation. Defaults to
///   the OS-native source (`NativeKeyboardSource`).
pub struct KeyboardManager<P: KeyboardSource = NativeKeyboardSource> {
    provider: P,

    active_keyboards: Arc<RwLock<Vec<Keyboard>>>,
    consumed_keyboards: Arc<RwLock<Vec<Keyboard>>>,

    on_key_action: Arc<RwLock<Vec<KeyActionCallback>>>,
    on_plugged: Arc<RwLock<Vec<DeviceCallback>>>,
    on_unplugged: Arc<RwLock<Vec<DeviceCallback>>>,

    events: broadcast::Sender<KeyboardEvent>,
    shutdown: broadcast::Sender<()>,
}

impl<P: KeyboardSource + Send + 'static> KeyboardManager<P> {
    /// Returns a list of currently active keyboards.
    pub fn get_keyboards(&self) -> Vec<Keyboard> {
        let keyboards = self.provider.enumerate_keyboards();

        if let Ok(mut active_keyboards) = self.active_keyboards.write() {
            *active_keyboards = keyboards.clone();
        }

        keyboards
    }

    /// Consumes (grabs) a specific keyboard device, preventing its input from
    /// reaching other applications or the operating system.
    pub async fn consume(&self, keyboard: &Keyboard) -> std::io::Result<()> {
        self.provider.consume(keyboard).await?;

        if let Ok(mut consumed) = self.consumed_keyboards.write()
            && !consumed.contains(keyboard)
        {
            consumed.push(keyboard.clone());
        }

        Ok(())
    }

    /// Releases a previously consumed keyboard device, restoring its standard behavior.
    pub async fn release(&self, keyboard: &Keyboard) -> std::io::Result<()> {
        self.provider.release(keyboard).await?;

        if let Ok(mut consumed) = self.consumed_keyboards.write() {
            consumed.retain(|k| k != keyboard);
        }

        Ok(())
    }

    /// Returns a list of keyboards that are currently consumed by this manager.
    pub fn get_consumed(&self) -> Vec<Keyboard> {
        self.consumed_keyboards
            .read()
            .map(|k| k.clone())
            .unwrap_or_default()
    }

    /// Releases all currently consumed keyboard devices.
    pub async fn release_all(&self) -> std::io::Result<()> {
        let keyboards = self.get_consumed();
        for kb in keyboards {
            self.release(&kb).await?;
        }
        Ok(())
    }

    /// Subscribes to the stream of keyboard events.
    ///
    /// Each subscriber receives its own [`broadcast::Receiver`].
    pub fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.events.subscribe()
    }

    /// Registers a callback to be executed when a keyboard is plugged in.
    pub fn on_plugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        if let Ok(mut callbacks) = self.on_plugged.write() {
            callbacks.push(Arc::new(callback));
        }
    }

    /// Registers a callback to be executed when a keyboard is unplugged.
    pub fn on_unplugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        if let Ok(mut callbacks) = self.on_unplugged.write() {
            callbacks.push(Arc::new(callback));
        }
    }

    /// Registers a callback to be executed when any key action occurs.
    pub fn on_key_action<F>(&self, callback: F)
    where
        F: Fn(&Keyboard, &KeyEvent) + Send + Sync + 'static,
    {
        if let Ok(mut callbacks) = self.on_key_action.write() {
            callbacks.push(Arc::new(callback));
        }
    }

    /// Starts the background event listening loop.
    pub async fn listen(&self) {
        let mut events = self.provider.subscribe();

        let active_keyboards = self.active_keyboards.clone();
        let consumed_keyboards = self.consumed_keyboards.clone();

        let on_key_action = self.on_key_action.clone();
        let on_plugged = self.on_plugged.clone();
        let on_unplugged = self.on_unplugged.clone();

        let events_tx = self.events.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;

                    _ = shutdown.recv() => {
                        break;
                    }

                    result = events.recv() => {
                        match result {
                            Ok(event) => {
                                match &event {
                                    KeyboardEvent::Plugged(kb) => {
                                        if let Ok(mut keyboards) = active_keyboards.write()
                                            && !keyboards.contains(&**kb)
                                        {
                                            keyboards.push((**kb).clone());
                                        }

                                        let callbacks = on_plugged
                                            .read()
                                            .map(|callbacks| callbacks.clone())
                                            .unwrap_or_default();

                                        for callback in callbacks {
                                            callback(&**kb);
                                        }
                                    }

                                    KeyboardEvent::Unplugged(kb) => {
                                        if let Ok(mut keyboards) = active_keyboards.write() {
                                            keyboards.retain(|k| k != &**kb);
                                        }

                                        if let Ok(mut consumed) = consumed_keyboards.write() {
                                            consumed.retain(|k| k != &**kb);
                                        }

                                        let callbacks = on_unplugged
                                            .read()
                                            .map(|callbacks| callbacks.clone())
                                            .unwrap_or_default();

                                        for callback in callbacks {
                                            callback(&**kb);
                                        }
                                    }

                                    KeyboardEvent::KeyAction(kb, key_action) => {
                                        let callbacks = on_key_action
                                            .read()
                                            .map(|callbacks| callbacks.clone())
                                            .unwrap_or_default();

                                        for callback in callbacks {
                                            callback(&**kb, key_action);
                                        }
                                    }
                                }

                                let _ = events_tx.send(event);
                            }

                            Err(broadcast::error::RecvError::Lagged(count)) => {
                                warn!(count, "keyboard event subscriber lagged");
                            }

                            Err(broadcast::error::RecvError::Closed) => {
                                break;
                            }
                        }
                    }
                }
            }
        });
    }
}

impl KeyboardManager {
    /// Creates a new `KeyboardManager` using the default OS-native keyboard source.
    pub async fn new() -> std::io::Result<Self> {
        let provider = NativeKeyboardSource::new().await?;
        let initial_keyboards = provider.enumerate_keyboards();

        let (events_tx, _) = broadcast::channel(1024);
        let (shutdown, _) = broadcast::channel(1);

        Ok(Self {
            provider,
            active_keyboards: Arc::new(RwLock::new(initial_keyboards)),
            consumed_keyboards: Arc::new(RwLock::new(Vec::new())),

            on_key_action: Arc::new(RwLock::new(Vec::new())),
            on_plugged: Arc::new(RwLock::new(Vec::new())),
            on_unplugged: Arc::new(RwLock::new(Vec::new())),

            events: events_tx,
            shutdown,
        })
    }
}

impl<P: KeyboardSource> From<P> for KeyboardManager<P> {
    /// Creates a new `KeyboardManager` from a custom [`KeyboardSource`] provider.
    fn from(provider: P) -> Self {
        let initial_keyboards = provider.enumerate_keyboards();

        let (events_tx, _) = broadcast::channel(1024);
        let (shutdown, _) = broadcast::channel(1);

        Self {
            provider,
            active_keyboards: Arc::new(RwLock::new(initial_keyboards)),
            consumed_keyboards: Arc::new(RwLock::new(Vec::new())),

            on_key_action: Arc::new(RwLock::new(Vec::new())),
            on_plugged: Arc::new(RwLock::new(Vec::new())),
            on_unplugged: Arc::new(RwLock::new(Vec::new())),

            events: events_tx,
            shutdown,
        }
    }
}

impl<P: KeyboardSource> Drop for KeyboardManager<P> {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}
