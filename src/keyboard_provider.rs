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

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    Plugged(Keyboard),
    Unplugged(Keyboard),
    Pressed(Keyboard),
}

pub trait DeviceProvider {
    fn get_keyboards(&self) -> Vec<Keyboard>;
    fn next_event(&self) -> impl Future<Output = Result<ProviderEvent, std::io::Error>> + Send;
}
