use super::{Access, Keyboard, KeyboardID, PortID};
use evdev::Device as EvdevDevice;
use std::{io, path::PathBuf, sync::Arc};
use tracing::error;
use udev::{Device as UdevDevice, Enumerator};

pub(crate) fn enumerate_keyboards() -> Result<Vec<(PathBuf, Arc<Keyboard>)>, io::Error> {
    let mut enumerator = Enumerator::new()?;
    enumerator.match_subsystem("input")?;

    Ok(enumerator
        .scan_devices()?
        .filter(|device| device.property_value("ID_INPUT_KEYBOARD").is_some())
        .filter_map(|device| {
            let devnode = device.devnode().map(PathBuf::from)?;
            let keyboard = Arc::new(map_to_keyboard(&device)?);
            Some((devnode, keyboard))
        })
        .collect())
}

pub(crate) fn map_to_keyboard(udev_dev: &UdevDevice) -> Option<Keyboard> {
    let devnode = udev_dev.devnode()?;
    let evdev = match EvdevDevice::open(devnode) {
        Ok(device) => device,
        Err(error) => {
            error!(
                error = %error,
                devnode = ?devnode,
                "failed open evdev device error",
            );
            return None;
        }
    };

    let input_id = evdev.input_id();

    let name = udev_dev
        .property_value("ID_MODEL_FROM_DATABASE")
        .or_else(|| udev_dev.property_value("ID_MODEL"))
        .and_then(|value| value.to_str().map(str::to_owned))
        .or_else(|| evdev.name().map(str::to_owned));

    let serial = udev_dev
        .property_value("ID_SERIAL_SHORT")
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("noserial"))
        .map(str::to_owned);

    let physical_path = udev_dev
        .property_value("ID_PATH")
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .or_else(|| udev_dev.syspath().to_str().map(str::to_owned));

    Some(Keyboard {
        keyboard_id: KeyboardID {
            name,
            vendor_id: Some(format!("{:04x}", input_id.vendor())),
            product_id: Some(format!("{:04x}", input_id.product())),
            serial,
        },
        port_id: PortID { physical_path },
        access: Access::Shared,
    })
}
