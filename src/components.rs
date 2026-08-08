use std::path::PathBuf;

pub trait InputEvent {
    fn is_key_event(&self) -> bool;
}

pub trait InputDevice: Send {
    type Event: InputEvent;

    fn is_keyboard(&self) -> bool;
    fn name(&self) -> Option<String>;
    fn fetch_events(&mut self) -> Result<Vec<Self::Event>, std::io::Error>;
}

pub trait DeviceSource {
    type Device: InputDevice;

    fn enumerate(&self) -> Vec<(PathBuf, Self::Device)>;
    fn open(&self, path: &PathBuf) -> Result<Self::Device, std::io::Error>;
}
