pub trait InputEvent {
    fn is_key_event(&self) -> bool;
}

pub trait InputDevice: Send {
    type Event: InputEvent;

    fn equal(&self, other: &Self) -> bool;
    fn fetch_events(&mut self) -> Result<Vec<Self::Event>, std::io::Error>;
    //fn open();
}

pub trait DeviceSource {
    type Device: InputDevice;

    fn get_keyboards(&self) -> Vec<Self::Device>;
}
