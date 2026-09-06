use crate::keyboard_source::*;
use crate::registry::*;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

type Callback = Arc<dyn Fn(&Keyboard) + Send + Sync + 'static>;

pub struct KeyboardManager<P: KeyboardSource = NativeKeyboardSource> {
    provider: Option<P>,
    active_keyboards: Arc<RwLock<Vec<Keyboard>>>,
    on_pressed: Registry<Callback>,
    on_plugged: Registry<Callback>,
    on_unplugged: Registry<Callback>,
    shutdown: broadcast::Sender<()>,
}

impl<P: KeyboardSource + Send + 'static> KeyboardManager<P> {
    pub fn get_keyboards(&self) -> Vec<Keyboard> {
        match self.provider.as_ref() {
            Some(p) => {
                let keyboards = p.get_keyboards();

                if let Ok(mut active_keyboards) = self.active_keyboards.write() {
                    *active_keyboards = keyboards.clone();
                }

                keyboards
            }
            None => self.active_keyboards.read().unwrap().clone(),
        }
    }

    pub fn on_plugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        self.on_plugged.register(Arc::new(callback));
    }

    pub fn on_unplugged<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        self.on_unplugged.register(Arc::new(callback));
    }

    pub fn on_pressed<F>(&self, callback: F)
    where
        F: Fn(&Keyboard) + Send + Sync + 'static,
    {
        self.on_pressed.register(Arc::new(callback));
    }

    pub async fn listen(&mut self) {
        let Some(mut provider) = self.provider.take() else {
            eprintln!("listen() can only be called once");
            return;
        };

        let on_pressed = self.on_pressed.clone();
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

                    Ok(KeyboardEvent::Pressed(kb)) => {
                        on_pressed.for_each(|cb| cb(&kb));
                    }

                    Err(e) => {
                        eprintln!("Keyboard source error: {}", e);
                        break;
                    }
                }
            }
        });
    }
}

impl KeyboardManager {
    pub async fn new() -> std::io::Result<Self> {
        let provider = NativeKeyboardSource::new().await?;
        let initial_keyboards = provider.get_keyboards();
        let (shutdown, _) = broadcast::channel(1);

        Ok(Self {
            provider: Some(provider),
            active_keyboards: Arc::new(RwLock::new(initial_keyboards)),
            on_pressed: Registry::new(),
            on_plugged: Registry::new(),
            on_unplugged: Registry::new(),
            shutdown,
        })
    }
}

impl<P: KeyboardSource> From<P> for KeyboardManager<P> {
    fn from(provider: P) -> Self {
        let initial_keyboards = provider.get_keyboards();
        let (shutdown, _) = broadcast::channel(1);

        Self {
            provider: Some(provider),
            active_keyboards: Arc::new(RwLock::new(initial_keyboards)),
            on_pressed: Registry::new(),
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
