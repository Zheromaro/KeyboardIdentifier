use crate::keyboard_provider::{DeviceProvider, Keyboard, KeyboardID, PortID, ProviderEvent};
use evdev::Device as EvdevDevice;
use std::future::Future;
use udev::{Device as UdevDevice, Enumerator};

// Import your new interface structs here:
// use crate::keyboard::{Keyboard, KeyboardID, PortID, ProviderEvent, DeviceProvider};

pub struct LinuxDeviceProvider {
    // You will likely need to store a channel receiver or similar state here
    // to multiplex udev hotplug events and evdev key events asynchronously.
}

impl LinuxDeviceProvider {
    /// Helper method to build a `Keyboard` struct from a udev device.
    fn map_to_keyboard(udev_dev: &UdevDevice) -> Option<Keyboard> {
        let devnode = udev_dev.devnode()?;
        let evdev = EvdevDevice::open(devnode).ok()?;

        // Map syspath to physical_path
        let port_id = PortID {
            physical_path: udev_dev.syspath().to_str().map(String::from),
        };

        // Map evdev and udev attributes to KeyboardID
        let input_id = evdev.input_id();
        let keyboard_id = KeyboardID {
            name: evdev.name().map(String::from),
            vendor_id: Some(format!("{:04x}", input_id.vendor())),
            product_id: Some(format!("{:04x}", input_id.product())),
            serial: udev_dev
                .property_value("ID_SERIAL_SHORT")
                .and_then(|v| v.to_str().map(String::from)),
        };

        Some(Keyboard {
            keyboard_id,
            port_id,
        })
    }
}

impl DeviceProvider for LinuxDeviceProvider {
    fn get_keyboards(&self) -> Vec<Keyboard> {
        let Ok(mut enumerator) = Enumerator::new() else {
            return Vec::new();
        };

        let _ = enumerator.match_subsystem("input");
        let Ok(devices) = enumerator.scan_devices() else {
            return Vec::new();
        };

        devices
            .filter_map(|udev_dev| {
                // Filter out non-keyboards using udev properties
                if udev_dev.property_value("ID_INPUT_KEYBOARD").is_none() {
                    return None;
                }

                Self::map_to_keyboard(&udev_dev)
            })
            .collect()
    }

    fn next_event(&self) -> impl Future<Output = Result<ProviderEvent, std::io::Error>> + Send {
        async {
            // TODO: Implement an asynchronous event loop here.
            // This requires multiplexing `tokio-udev` (for Plugged/Unplugged)
            // and `evdev` asynchronous streams (for Pressed events).
            std::future::pending().await
        }
    }
}
