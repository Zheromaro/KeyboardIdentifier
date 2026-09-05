use std::{fmt, io};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortID {
    pub physical_path: Option<String>,
}

impl fmt::Display for PortID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PortID {{ physical_path: {} }}",
            self.physical_path.as_deref().unwrap_or("None")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardID {
    pub name: Option<String>,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub serial: Option<String>,
}

impl fmt::Display for KeyboardID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KeyboardID {{ name: {}, vendor_id: {}, product_id: {}, serial: {} }}",
            self.name.as_deref().unwrap_or("None"),
            self.vendor_id.as_deref().unwrap_or("None"),
            self.product_id.as_deref().unwrap_or("None"),
            self.serial.as_deref().unwrap_or("None"),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    pub keyboard_id: KeyboardID,
    pub port_id: PortID,
}

impl fmt::Display for Keyboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Keyboard {{ keyboard_id: {}, port_id: {} }}",
            self.keyboard_id, self.port_id
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardEvent {
    Plugged(Keyboard),
    Unplugged(Keyboard),
    Pressed(Keyboard),
}

impl fmt::Display for KeyboardEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyboardEvent::Plugged(kb) => write!(f, "Plugged({})", kb),
            KeyboardEvent::Unplugged(kb) => write!(f, "Unplugged({})", kb),
            KeyboardEvent::Pressed(kb) => write!(f, "Pressed({})", kb),
        }
    }
}

pub trait KeyboardSource: Sized {
    fn new() -> impl Future<Output = io::Result<Self>> + Send;
    fn get_keyboards(&self) -> Vec<Keyboard>;
    fn receive_event(
        &mut self,
    ) -> impl Future<Output = Result<KeyboardEvent, std::io::Error>> + Send;
}
