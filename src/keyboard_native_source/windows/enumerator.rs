use super::errors::{handle_key, win32_error};
use super::path_parser::KeyboardPathParser;
use crate::keyboard_source::Keyboard;

use std::collections::HashMap;
use std::ffi::c_void;
use std::io;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICENAME,
    RIM_TYPEKEYBOARD,
};

pub(crate) struct DeviceEnumerator;

impl DeviceEnumerator {
    pub(crate) fn enumerate_keyboards() -> io::Result<HashMap<isize, Keyboard>> {
        let devices = Self::list_devices()?;
        let mut keyboards = HashMap::new();

        for device in devices {
            if device.dwType != RIM_TYPEKEYBOARD {
                continue;
            }

            if let Some(keyboard) = Self::keyboard_from_handle(device.hDevice) {
                keyboards.insert(handle_key(device.hDevice), keyboard);
            }
        }

        Ok(keyboards)
    }

    pub(crate) fn keyboard_from_handle(handle: HANDLE) -> Option<Keyboard> {
        let path = Self::device_name(handle).ok()?;
        Some(KeyboardPathParser::parse(&path))
    }

    fn device_name(handle: HANDLE) -> io::Result<String> {
        unsafe {
            let mut size = 0u32;

            let result = GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, None, &mut size);

            if result == u32::MAX {
                return Err(win32_error("GetRawInputDeviceInfoW(size) failed"));
            }

            if size == 0 {
                return Err(io::Error::other(
                    "GetRawInputDeviceInfoW returned an empty device name",
                ));
            }

            let mut buffer = vec![0u16; size as usize + 1];
            let mut actual_size = buffer.len() as u32;

            let result = GetRawInputDeviceInfoW(
                handle,
                RIDI_DEVICENAME,
                Some(buffer.as_mut_ptr() as *mut c_void),
                &mut actual_size,
            );

            if result == u32::MAX {
                return Err(win32_error("GetRawInputDeviceInfoW(data) failed"));
            }

            let len = actual_size.min(buffer.len() as u32) as usize;

            Ok(String::from_utf16_lossy(&buffer[..len])
                .trim_end_matches('\0')
                .to_owned())
        }
    }

    fn list_devices() -> io::Result<Vec<RAWINPUTDEVICELIST>> {
        unsafe {
            let mut count = 0u32;

            let result = GetRawInputDeviceList(
                None,
                &mut count,
                std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
            );

            if result == u32::MAX {
                return Err(win32_error("GetRawInputDeviceList(size) failed"));
            }

            if count == 0 {
                return Ok(Vec::new());
            }

            let mut devices = vec![RAWINPUTDEVICELIST::default(); count as usize];

            let result = GetRawInputDeviceList(
                Some(devices.as_mut_ptr()),
                &mut count,
                std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
            );

            if result == u32::MAX {
                return Err(win32_error("GetRawInputDeviceList(data) failed"));
            }

            devices.truncate(result as usize);
            Ok(devices)
        }
    }
}
