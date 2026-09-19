use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource, NativeKeyboardSource};
use keyboard_types::KeyboardEvent as KeyEvent;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::broadcast;
use tracing::warn;

type DeviceCallback = Arc<dyn Fn(&Keyboard) + Send + Sync + 'static>;
type KeyActionCallback = Arc<dyn Fn(&Keyboard, &KeyEvent) + Send + Sync + 'static>;

/// Internal shared state for the KeyboardManager.
struct Inner<P> {
    provider: P,
    active_keyboards: RwLock<Vec<Arc<Keyboard>>>,
    consumed_keyboards: RwLock<Vec<Arc<Keyboard>>>,
    on_key_action: RwLock<Vec<KeyActionCallback>>,
    on_plugged: RwLock<Vec<DeviceCallback>>,
    on_unplugged: RwLock<Vec<DeviceCallback>>,
    events: broadcast::Sender<KeyboardEvent>,
    shutdown: broadcast::Sender<()>,
    listening: AtomicBool,
}

/// The main manager for tracking keyboards and listening to their events.
pub struct KeyboardManager<P: KeyboardSource = NativeKeyboardSource> {
    inner: Arc<Inner<P>>,
}

impl<P: KeyboardSource> Clone for KeyboardManager<P> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<P: KeyboardSource + Send + 'static> KeyboardManager<P> {
    /// Returns a list of currently active keyboards.
    ///
    /// Returns `Arc<Keyboard>` to allow zero-cost cloning and to align
    /// with the `KeyboardEvent` enum.
    pub fn get_keyboards(&self) -> Vec<Arc<Keyboard>> {
        let keyboards = self.inner.provider.enumerate_keyboards();
        let arcs: Vec<Arc<Keyboard>> = keyboards.into_iter().map(Arc::new).collect();

        if let Ok(mut active) = self.inner.active_keyboards.write() {
            *active = arcs.clone();
        }
        arcs
    }

    /// Consumes (grabs) a specific keyboard device.
    pub async fn consume(&self, keyboard: &Keyboard) -> std::io::Result<()> {
        self.inner.provider.consume(keyboard).await?;

        if let Ok(mut consumed) = self.inner.consumed_keyboards.write() {
            // Check if it's already consumed
            if !consumed.iter().any(|k| k.as_ref() == keyboard) {
                // Optimization: Try to find the existing Arc in active_keyboards
                // to avoid allocating new Strings for the KeyboardID/PortID.
                let arc_kb = self
                    .inner
                    .active_keyboards
                    .read()
                    .ok()
                    .and_then(|active| active.iter().find(|k| k.as_ref() == keyboard).cloned())
                    .unwrap_or_else(|| Arc::new(keyboard.clone()));

                consumed.push(arc_kb);
            }
        }
        Ok(())
    }

    /// Releases a previously consumed keyboard device.
    pub async fn release(&self, keyboard: &Keyboard) -> std::io::Result<()> {
        self.inner.provider.release(keyboard).await?;
        if let Ok(mut consumed) = self.inner.consumed_keyboards.write() {
            consumed.retain(|k| k.as_ref() != keyboard);
        }
        Ok(())
    }

    /// Returns a list of keyboards that are currently consumed by this manager.
    pub fn get_consumed(&self) -> Vec<Arc<Keyboard>> {
        self.inner
            .consumed_keyboards
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
    pub fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.inner.events.subscribe()
    }

    /// Registers a callback to be executed when a keyboard is plugged in.
    pub fn on_plugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        if let Ok(mut callbacks) = self.inner.on_plugged.write() {
            callbacks.push(Arc::new(callback));
        }
    }

    /// Registers a callback to be executed when a keyboard is unplugged.
    pub fn on_unplugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        if let Ok(mut callbacks) = self.inner.on_unplugged.write() {
            callbacks.push(Arc::new(callback));
        }
    }

    /// Registers a callback to be executed when any key action occurs.
    pub fn on_key_action<F>(&self, callback: F)
    where
        F: Fn(&Keyboard, &KeyEvent) + Send + Sync + 'static,
    {
        if let Ok(mut callbacks) = self.inner.on_key_action.write() {
            callbacks.push(Arc::new(callback));
        }
    }

    /// Starts the background event listening loop.
    ///
    /// Calling this method more than once has no effect.
    pub async fn listen(&self) {
        // Prevent multiple listener tasks from being created.
        if self.inner.listening.swap(true, Ordering::AcqRel) {
            return;
        }

        let inner = self.inner.clone();

        // Subscribe BEFORE spawning the task.
        //
        // This is important because listen() must not return with a race
        // window where an event can be emitted before the receiver exists.
        let mut events = inner.provider.subscribe();
        let mut shutdown = inner.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;

                    _ = shutdown.recv() => break,

                    result = events.recv() => {
                        match result {
                            Ok(event) => {
                                match &event {
                                    KeyboardEvent::Plugged(kb) => {
                                        if let Ok(mut keyboards) =
                                            inner.active_keyboards.write()
                                        {
                                            if !keyboards.contains(kb) {
                                                keyboards.push(Arc::clone(kb));
                                            }
                                        }

                                        let callbacks = inner
                                            .on_plugged
                                            .read()
                                            .map(|callbacks| callbacks.clone())
                                            .unwrap_or_default();

                                        for cb in callbacks {
                                            cb(kb);
                                        }
                                    }

                                    KeyboardEvent::Unplugged(kb) => {
                                        if let Ok(mut keyboards) =
                                            inner.active_keyboards.write()
                                        {
                                            keyboards.retain(|k| k != kb);
                                        }

                                        if let Ok(mut consumed) =
                                            inner.consumed_keyboards.write()
                                        {
                                            consumed.retain(|k| k != kb);
                                        }

                                        let callbacks = inner
                                            .on_unplugged
                                            .read()
                                            .map(|callbacks| callbacks.clone())
                                            .unwrap_or_default();

                                        for cb in callbacks {
                                            cb(kb);
                                        }
                                    }

                                    KeyboardEvent::KeyAction(kb, key_action) => {
                                        let callbacks = inner
                                            .on_key_action
                                            .read()
                                            .map(|callbacks| callbacks.clone())
                                            .unwrap_or_default();

                                        for cb in callbacks {
                                            cb(kb, key_action);
                                        }
                                    }
                                }

                                let _ = inner.events.send(event);
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

            // Allow a future call to listen() after the previous listener exits.
            inner.listening.store(false, Ordering::Release);
        });
    }
}

impl KeyboardManager {
    /// Creates a new `KeyboardManager` using the default OS-native keyboard source.
    pub async fn new() -> std::io::Result<Self> {
        let provider = NativeKeyboardSource::new().await?;
        Ok(Self::from(provider))
    }
}

impl<P: KeyboardSource> From<P> for KeyboardManager<P> {
    /// Creates a new `KeyboardManager` from a custom [`KeyboardSource`] provider.
    fn from(provider: P) -> Self {
        // Wrap initial keyboards in Arc immediately
        let initial_keyboards: Vec<Arc<Keyboard>> = provider
            .enumerate_keyboards()
            .into_iter()
            .map(Arc::new)
            .collect();

        let (events_tx, _) = broadcast::channel(1024);
        let (shutdown, _) = broadcast::channel(1);

        Self {
            inner: Arc::new(Inner {
                provider,
                active_keyboards: RwLock::new(initial_keyboards),
                consumed_keyboards: RwLock::new(Vec::new()),
                on_key_action: RwLock::new(Vec::new()),
                on_plugged: RwLock::new(Vec::new()),
                on_unplugged: RwLock::new(Vec::new()),
                events: events_tx,
                shutdown,
                listening: AtomicBool::new(false),
            }),
        }
    }
}

impl<P: KeyboardSource> Drop for KeyboardManager<P> {
    fn drop(&mut self) {
        let _ = self.inner.shutdown.send(());
    }
}
