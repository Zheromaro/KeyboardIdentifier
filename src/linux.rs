use crate::components::{DeviceSource, InputDevice, InputEvent};
use evdev::{Device, EventType, KeyCode};
use std::path::PathBuf;

pub struct EvdevEvent(evdev::InputEvent);

impl InputEvent for EvdevEvent {
    fn is_key_event(&self) -> bool {
        self.0.event_type() == EventType::KEY
    }
}

impl InputDevice for Device {
    type Event = EvdevEvent;

    fn is_keyboard(&self) -> bool {
        self.supported_keys()
            .map(|keys| keys.contains(KeyCode::KEY_A) && keys.contains(KeyCode::KEY_ENTER))
            .unwrap_or(false)
    }

    fn name(&self) -> Option<String> {
        self.name().map(|s| s.to_owned())
    }

    fn fetch_events(&mut self) -> Result<Vec<EvdevEvent>, std::io::Error> {
        // Call the inherent evdev method explicitly to avoid recursion into the trait method
        evdev::Device::fetch_events(self).map(|iter| iter.map(EvdevEvent).collect())
    }
}

pub struct LinuxDeviceSource;

impl DeviceSource for LinuxDeviceSource {
    type Device = Device;

    fn enumerate(&self) -> Vec<(PathBuf, Device)> {
        evdev::enumerate().collect()
    }

    fn open(&self, path: &PathBuf) -> Result<Device, std::io::Error> {
        Device::open(path)
    }
}
