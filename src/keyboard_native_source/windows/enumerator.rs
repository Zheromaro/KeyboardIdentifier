use super::{handles::DeviceHandle, hid, path_parser::KeyboardPathParser, raw_input, setup_api};
use crate::keyboard_source::Keyboard;
use std::io;
use windows::Win32::Foundation::HANDLE;

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredKeyboard {
    pub(crate) handle: DeviceHandle,
    pub(crate) keyboard: Keyboard,
}

pub(crate) struct DeviceEnumerator;

impl DeviceEnumerator {
    pub(crate) fn enumerate_keyboards() -> io::Result<Vec<DiscoveredKeyboard>> {
        let mut keyboards = Vec::new();

        for handle in raw_input::list_keyboards()? {
            let Some(keyboard) = Self::keyboard_from_handle(handle) else {
                continue;
            };

            keyboards.push(DiscoveredKeyboard {
                handle: handle.into(),
                keyboard,
            });
        }

        Ok(keyboards)
    }

    pub(crate) fn keyboard_from_handle(handle: HANDLE) -> Option<Keyboard> {
        let path = raw_input::device_name(handle).ok()?;

        let physical_path = setup_api::physical_path(&path);

        let mut keyboard = KeyboardPathParser::parse(&path, physical_path);

        let (product, serial) = hid::strings(&path);

        if keyboard.keyboard_id.name.is_none() {
            keyboard.keyboard_id.name = product;
        }

        keyboard.keyboard_id.serial = serial;

        Some(keyboard)
    }
}
