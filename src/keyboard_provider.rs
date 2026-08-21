#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortID {
    pub physical_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardID {
    pub name: Option<String>,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub serial: Option<String>,
}

impl KeyboardID {
    pub fn as_str(&self) -> String {
        format!("{:?}", self)
    }
}

impl PortID {
    pub fn as_str(&self) -> String {
        format!("{:?}", self)
    }
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    Plugged {
        keyboard_id: KeyboardID,
        port: PortID,
    },
    Unplugged {
        keyboard_id: KeyboardID,
        port: PortID,
    },
    Pressed {
        keyboard_id: KeyboardID,
        port: PortID,
    },
}

pub trait Port {
    fn id(&self) -> PortID;
    fn keyboard_id(&self) -> KeyboardID;

    //async
    fn next_event(&mut self) -> impl Future<Output = Result<ProviderEvent, std::io::Error>> + Send;
}

pub trait Keyboard {
    fn id(&self) -> KeyboardID;
    fn port_id(&self) -> PortID;

    //async
    fn next_event(&mut self) -> impl Future<Output = Result<ProviderEvent, std::io::Error>> + Send;
}

pub trait DeviceProvider {
    type Device: Keyboard;

    fn get_keyboards(&self) -> Vec<Self::Device>;
    fn get_ports(&self) -> Vec<PortID>;
}
