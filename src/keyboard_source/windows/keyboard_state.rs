use super::handles::DeviceHandle;

use crate::keyboard_source::Keyboard;

use std::collections::HashMap;

#[derive(Debug, Default)]
pub(crate) struct KeyboardState {
    devices: HashMap<DeviceHandle, Keyboard>,
}

impl KeyboardState {
    pub(crate) fn new(keyboards: impl IntoIterator<Item = (DeviceHandle, Keyboard)>) -> Self {
        Self {
            devices: keyboards.into_iter().collect(),
        }
    }

    pub(crate) fn insert(&mut self, handle: DeviceHandle, keyboard: Keyboard) -> bool {
        self.devices.insert(handle, keyboard).is_none()
    }

    pub(crate) fn remove(&mut self, handle: DeviceHandle) -> Option<Keyboard> {
        self.devices.remove(&handle)
    }

    pub(crate) fn get(&self, handle: DeviceHandle) -> Option<&Keyboard> {
        self.devices.get(&handle)
    }
}
