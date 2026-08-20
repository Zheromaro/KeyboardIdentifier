#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Port {
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
        format!(
            "{:?}, {:?}, {:?}, {:?}",
            self.name, self.vendor_id, self.product_id, self.serial
        )
    }
}

pub trait KeyboardDevice {
    fn is_plugged(&self) -> bool;
    fn id(&self) -> KeyboardID;
    fn port(&self) -> Port;

    //async
    fn fetch_events(&mut self) -> impl Future<Output = Result<(), std::io::Error>> + Send;
}

pub trait DeviceProvider {
    type Device: KeyboardDevice;

    fn get_keyboards(&self) -> Vec<Self::Device>;
    fn get_ports(&self) -> Vec<Port>;

    //async
    fn plugged_event(
        &mut self,
    ) -> impl Future<Output = Result<(KeyboardID, Port), std::io::Error>> + Send;
    fn unplugged_event(
        &mut self,
    ) -> impl Future<Output = Result<(KeyboardID, Port), std::io::Error>> + Send;
}
