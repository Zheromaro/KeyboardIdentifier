#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortID {
    pub physical_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardID {
    pub name: Option<String>,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub serial: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    pub keyboard_id: KeyboardID,
    pub port_id: PortID,
}

impl Keyboard {
    pub fn as_str(&self) -> String {
        format!("{:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardEvent {
    Plugged(Keyboard),
    Unplugged(Keyboard),
    Pressed(Keyboard),
}

pub trait KeyboardSource {
    fn new() -> impl Future<Output = Self> + Send;
    fn get_keyboards(&self) -> Vec<Keyboard>;
    fn receive_event(
        &mut self,
    ) -> impl Future<Output = Result<KeyboardEvent, std::io::Error>> + Send;
}
