use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct RegistryState<T> {
    items: HashMap<u64, T>,
    unique_keys: HashMap<String, u64>,
    next_id: u64,
}

#[derive(Clone)]
pub struct Registry<T> {
    inner: Arc<Mutex<RegistryState<T>>>,
}

impl<T> Registry<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RegistryState {
                items: HashMap::new(),
                unique_keys: HashMap::new(),
                next_id: 0,
            })),
        }
    }

    pub fn register(&self, item: T) -> u64 {
        let mut state = self.inner.lock().unwrap();
        let id = state.next_id;

        state.next_id = state.next_id.wrapping_add(1);
        state.items.insert(id, item);

        id
    }

    pub fn register_unique(&self, key: impl Into<String>, item: T) -> Option<u64> {
        let mut state = self.inner.lock().unwrap();
        let key_str = key.into();

        if state.unique_keys.contains_key(&key_str) {
            return None;
        }

        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);

        state.items.insert(id, item);
        state.unique_keys.insert(key_str, id);

        Some(id)
    }

    pub fn unregister(&self, id: u64) -> Option<T> {
        let mut state = self.inner.lock().unwrap();

        let removed_item = state.items.remove(&id);

        if removed_item.is_some() {
            // Find and remove the associated unique key, if it had one
            state
                .unique_keys
                .retain(|_, &mut mapped_id| mapped_id != id);
        }

        removed_item
    }

    pub fn for_each<F>(&self, mut f: F)
    where
        T: Clone,
        F: FnMut(&T),
    {
        let items: Vec<T> = {
            let state = self.inner.lock().unwrap();
            state.items.values().cloned().collect()
        };
        for item in &items {
            f(item);
        }
    }
}
