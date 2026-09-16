use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource, NativeKeyboardSource};
use crate::registry::Registry;
use keyboard_types::KeyboardEvent as KeyEvent;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tracing::{error, warn};

type DeviceCallback = Arc<dyn Fn(&Keyboard) + Send + Sync + 'static>;
type KeyActionCallback = Arc<dyn Fn(&Keyboard, &KeyEvent) + Send + Sync + 'static>;

/// The main manager for tracking keyboards and listening to their events.
///
/// `KeyboardManager` maintains a list of active keyboards and allows you to
/// register callbacks for specific events: key actions, device plugged,
/// and device unplugged.
///
/// # Type Parameters
///
/// * `P`: The underlying [`KeyboardSource`] implementation. Defaults to
///   the OS-native source (`NativeKeyboardSource`).
pub struct KeyboardManager<P: KeyboardSource = NativeKeyboardSource> {
    provider: Option<P>,
    active_keyboards: Arc<RwLock<Vec<Keyboard>>>,
    on_key_action: Registry<KeyActionCallback>,
    on_plugged: Registry<DeviceCallback>,
    on_unplugged: Registry<DeviceCallback>,
    shutdown: broadcast::Sender<()>,
}

impl<P: KeyboardSource + Send + 'static> KeyboardManager<P> {
    /// Returns a list of currently active keyboards.
    pub fn get_keyboards(&self) -> Vec<Keyboard> {
        match self.provider.as_ref() {
            Some(p) => {
                let keyboards = p.enumerate_keyboards();
                if let Ok(mut active_keyboards) = self.active_keyboards.write() {
                    *active_keyboards = keyboards.clone();
                }
                keyboards
            }
            None => self.active_keyboards.read().unwrap().clone(),
        }
    }

    /// Registers a callback to be executed when a keyboard is plugged in.
    ///
    /// Multiple callbacks can be registered. They will be executed sequentially
    /// when a `Plugged` event is received.
    pub fn on_plugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        self.on_plugged.register(Arc::new(callback));
    }

    /// Registers a callback to be executed when a keyboard is unplugged.
    ///
    /// Multiple callbacks can be registered. They will be executed sequentially
    /// when an `Unplugged` event is received.
    pub fn on_unplugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        self.on_unplugged.register(Arc::new(callback));
    }

    /// Registers a callback to be executed when any key action (press or release)
    /// occurs on any tracked keyboard.
    ///
    /// Multiple callbacks can be registered. They will be executed sequentially
    /// when a `KeyEvent` is received.
    ///
    /// This provides the full [`keyboard_types::KeyboardEvent`], allowing access to
    /// the logical key, physical code, modifiers, location, repeat state, and more.
    pub fn on_key_action<F>(&self, callback: F)
    where
        F: Fn(&Keyboard, &KeyEvent) + Send + Sync + 'static,
    {
        self.on_key_action.register(Arc::new(callback));
    }

    /// Starts the background event listening loop.
    ///
    /// This method spawns a Tokio task that continuously polls the underlying
    /// [`KeyboardSource`] for events. It will route events to the registered
    /// callbacks and maintain the internal list of active keyboards.
    ///
    /// # Note
    ///
    /// This method can only be called once. Subsequent calls will log a warning
    /// and return immediately. The loop will automatically terminate when the
    /// `KeyboardManager` is dropped.
    pub async fn listen(&mut self) {
        let Some(mut provider) = self.provider.take() else {
            warn!("listen() can only be called once");
            return;
        };

        let on_pressed = self.on_key_action.clone();
        let on_plugged = self.on_plugged.clone();
        let on_unplugged = self.on_unplugged.clone();
        let active_keyboards = self.active_keyboards.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    biased;
                    _ = shutdown.recv() => break,
                    res = provider.receive_event() => res,
                };

                match event {
                    Ok(KeyboardEvent::Plugged(kb)) => {
                        if let Ok(mut kbs) = active_keyboards.write()
                            && !kbs.contains(&*kb)
                        {
                            kbs.push((*kb).clone());
                        }
                        on_plugged.for_each(|cb| cb(&kb));
                    }
                    Ok(KeyboardEvent::Unplugged(kb)) => {
                        if let Ok(mut kbs) = active_keyboards.write() {
                            kbs.retain(|k| k != &*kb);
                        }
                        on_unplugged.for_each(|cb| cb(&kb));
                    }
                    Ok(KeyboardEvent::KeyAction(kb, key_action)) => {
                        on_pressed.for_each(|cb| cb(&kb, &key_action));
                    }
                    Err(e) => {
                        error!(error = %e, "keyboard source error");
                        break;
                    }
                }
            }
        });
    }
}

impl KeyboardManager {
    /// Creates a new `KeyboardManager` using the default OS-native keyboard source.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the underlying OS-specific keyboard source
    /// fails to initialize (e.g., due to permissions or missing system APIs).
    pub async fn new() -> std::io::Result<Self> {
        let provider = NativeKeyboardSource::new().await?;
        let initial_keyboards = provider.enumerate_keyboards();
        let (shutdown, _) = broadcast::channel(1);

        Ok(Self {
            provider: Some(provider),
            active_keyboards: Arc::new(RwLock::new(initial_keyboards)),
            on_key_action: Registry::new(),
            on_plugged: Registry::new(),
            on_unplugged: Registry::new(),
            shutdown,
        })
    }
}

impl<P: KeyboardSource> From<P> for KeyboardManager<P> {
    /// Creates a new `KeyboardManager` from a custom [`KeyboardSource`] provider.
    fn from(provider: P) -> Self {
        let initial_keyboards = provider.enumerate_keyboards();
        let (shutdown, _) = broadcast::channel(1);

        Self {
            provider: Some(provider),
            active_keyboards: Arc::new(RwLock::new(initial_keyboards)),
            on_key_action: Registry::new(),
            on_plugged: Registry::new(),
            on_unplugged: Registry::new(),
            shutdown,
        }
    }
}

impl<P: KeyboardSource> Drop for KeyboardManager<P> {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}
