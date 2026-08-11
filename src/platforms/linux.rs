use crate::interface::{DeviceSource, InputDevice, InputEvent};
use evdev::{Device as EvdevDevice, EventType};
use std::path::PathBuf;
use udev::{Device as UdevDevice, Enumerator};

// Event
pub struct EvdevEvent(evdev::InputEvent);

impl InputEvent for EvdevEvent {
    fn is_key_event(&self) -> bool {
        self.0.event_type() == EventType::KEY
    }
}

// Device
#[derive(Debug, Clone)]
pub struct UdevInfo {
    syspath: PathBuf,
    devnum: Option<u64>,
    is_keyboard: bool,
}

impl UdevInfo {
    fn from_udev(udev: &UdevDevice) -> Self {
        Self {
            syspath: udev.syspath().to_path_buf(),
            devnum: udev.devnum(),
            is_keyboard: udev
                .property_value("ID_INPUT_KEYBOARD")
                .map(|v| v == "1")
                .unwrap_or(false),
        }
    }
}

pub struct LinuxInputDevice {
    udev: UdevInfo,
    evdev: EvdevDevice,
}

impl InputDevice for LinuxInputDevice {
    type Event = EvdevEvent;

    fn equal(&self, other: &Self) -> bool {
        if self.udev.syspath == other.udev.syspath {
            return true;
        }

        if let (Some(self_num), Some(other_num)) = (self.udev.devnum, other.udev.devnum) {
            if self_num == other_num {
                return true;
            }
        }

        false
    }

    fn fetch_events(&mut self) -> Result<Vec<EvdevEvent>, std::io::Error> {
        self.evdev
            .fetch_events()
            .map(|iter| iter.map(EvdevEvent).collect())
    }
}

// DeviceSource
pub struct LinuxDeviceSource;

impl DeviceSource for LinuxDeviceSource {
    type Device = LinuxInputDevice;

    fn get_keyboards(&self) -> Vec<Self::Device> {
        let Ok(mut enumerator) = Enumerator::new() else {
            return Vec::new();
        };

        enumerator.match_subsystem("input");
        let Ok(devices) = enumerator.scan_devices() else {
            return Vec::new();
        };

        devices
            .filter_map(|udev_dev| {
                let devnode = udev_dev.devnode()?;

                if udev_dev.property_value("ID_INPUT_KEYBOARD").is_none() {
                    return None;
                }

                let udev = UdevInfo::from_udev(&udev_dev);
                let evdev = EvdevDevice::open(devnode).ok()?;

                Some(LinuxInputDevice { udev, evdev })
            })
            .collect()
    }
}
