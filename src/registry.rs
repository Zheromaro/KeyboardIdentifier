use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct RegistryState<T> {
    items: HashMap<u64, T>,
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
