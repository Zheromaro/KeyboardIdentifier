use crate::keyboard_source::{Keyboard, KeyboardID, PortID};
use interception::{Device as InterceptionDevice, Interception};
use std::{io, mem::size_of};
use windows::Win32::{
    Devices::{
        DeviceAndDriverInstallation::{
            DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
            SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
            SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
            SetupDiGetDevicePropertyW,
        },
        HumanInterfaceDevice::{HidD_GetProductString, HidD_GetSerialNumberString},
        Properties::{
            DEVPKEY_Device_InstanceId, DEVPKEY_Device_LocationPaths, DEVPROP_TYPE_STRING,
            DEVPROP_TYPE_STRING_LIST, DEVPROPKEY, DEVPROPTYPE,
        },
    },
    Foundation::{CloseHandle, HANDLE},
    Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
    UI::Input::{RIM_TYPEKEYBOARD, GetRawInputDeviceInfoW, GetRawInputDeviceList,
        RAWINPUTDEVICELIST, RIDI_DEVICENAME},
};
use windows::core::PCWSTR;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DeviceHandle(pub(crate) InterceptionDevice);

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredKeyboard {
    pub(crate) handle: DeviceHandle,
    pub(crate) keyboard: Keyboard,
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) struct DeviceEnumerator;

impl DeviceEnumerator {
    /// Enumerates keyboards known to the Windows Raw Input subsystem and matches
    /// them to Interception device slots using VID/PID/serial information.
    ///
    /// Raw Input is used only for device metadata; keyboard events themselves
    /// are handled by Interception.
    pub(crate) fn enumerate_keyboards(
        interception: &Interception,
    ) -> io::Result<Vec<DiscoveredKeyboard>> {
        let raw_keyboards = Self::enumerate_raw_keyboards()?;
        let mut result = Vec::new();

        for device in 1..=20 {
            if !interception::is_keyboard(device) {
                continue;
            }

            let Some(hardware_id) = Self::hardware_id(interception, device) else {
                continue;
            };

            let keyboard = raw_keyboards
                .iter()
                .find(|keyboard| Self::matches_hardware_id(&keyboard.keyboard, &hardware_id))
                .map(|keyboard| keyboard.keyboard.clone())
                .unwrap_or_else(|| Self::keyboard_from_hardware_id(&hardware_id));

            result.push(DiscoveredKeyboard {
                handle: DeviceHandle(device),
                keyboard,
            });
        }

        Ok(result)
    }

    pub(crate) fn hardware_id(
        interception: &Interception,
        device: InterceptionDevice,
    ) -> Option<String> {
        let mut buffer = vec![0u8; 512];
        let length = interception.get_hardware_id(device, &mut buffer) as usize;
        if length == 0 || length > buffer.len() {
            return None;
        }

        Some(String::from_utf8_lossy(&buffer[..length])
            .trim_end_matches('\0')
            .to_owned())
    }

    fn enumerate_raw_keyboards() -> io::Result<Vec<DiscoveredKeyboardMetadata>> {
        let mut count = 0u32;
        if unsafe {
            GetRawInputDeviceList(None, &mut count, size_of::<RAWINPUTDEVICELIST>() as u32)
        } == u32::MAX
        {
            return Err(super::win32_error("GetRawInputDeviceList(size) failed"));
        }

        if count == 0 {
            return Ok(Vec::new());
        }

        let mut devices = vec![RAWINPUTDEVICELIST::default(); count as usize];
        if unsafe {
            GetRawInputDeviceList(
                Some(devices.as_mut_ptr()),
                &mut count,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            )
        } == u32::MAX
        {
            return Err(super::win32_error("GetRawInputDeviceList(data) failed"));
        }

        Ok(devices
            .into_iter()
            .filter(|device| device.dwType == RIM_TYPEKEYBOARD)
            .filter_map(|device| {
                let keyboard = Self::keyboard_from_raw_handle(device.hDevice)?;
                Some(DiscoveredKeyboardMetadata { keyboard })
            })
            .collect())
    }

    fn keyboard_from_raw_handle(handle: HANDLE) -> Option<Keyboard> {
        let path = Self::device_name(handle).ok()?;
        let hardware_id = path.split('#').nth(1).unwrap_or_default();

        let mut keyboard = Keyboard {
            keyboard_id: KeyboardID {
                name: None,
                vendor_id: Self::extract_hex(hardware_id, "VID_"),
                product_id: Self::extract_hex(hardware_id, "PID_"),
                serial: None,
            },
            port_id: PortID {
                physical_path: Self::physical_path(&path),
            },
        };

        let (product, serial) = Self::hid_strings(&path);
        keyboard.keyboard_id.name = product;
        keyboard.keyboard_id.serial = serial;
        Some(keyboard)
    }

    fn keyboard_from_hardware_id(hardware_id: &str) -> Keyboard {
        Keyboard {
            keyboard_id: KeyboardID {
                name: None,
                vendor_id: Self::extract_hex(hardware_id, "VID_"),
                product_id: Self::extract_hex(hardware_id, "PID_"),
                serial: Self::extract_serial(hardware_id),
            },
            port_id: PortID {
                physical_path: Some(hardware_id.to_owned()),
            },
        }
    }

    fn matches_hardware_id(keyboard: &Keyboard, hardware_id: &str) -> bool {
        let vendor_matches = keyboard
            .keyboard_id
            .vendor_id
            .as_deref()
            .is_some_and(|vid| Self::extract_hex(hardware_id, "VID_").as_deref() == Some(vid));
        let product_matches = keyboard
            .keyboard_id
            .product_id
            .as_deref()
            .is_some_and(|pid| Self::extract_hex(hardware_id, "PID_").as_deref() == Some(pid));

        if !vendor_matches || !product_matches {
            return false;
        }

        match keyboard.keyboard_id.serial.as_deref() {
            Some(serial) if !serial.is_empty() => hardware_id
                .to_ascii_uppercase()
                .contains(&serial.to_ascii_uppercase()),
            _ => true,
        }
    }

    fn extract_serial(hardware_id: &str) -> Option<String> {
        let mut parts = hardware_id.split('\\');
        let _ = parts.next();
        let _ = parts.next();
        let instance = parts.next()?.trim();
        (!instance.is_empty()).then(|| instance.to_owned())
    }

    fn device_name(handle: HANDLE) -> io::Result<String> {
        let mut size = 0u32;
        if unsafe { GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, None, &mut size) }
            == u32::MAX
        {
            return Err(super::win32_error("GetRawInputDeviceInfoW(size) failed"));
        }

        let mut buffer = vec![0u16; size as usize + 1];
        let mut actual_size = buffer.len() as u32;
        if unsafe {
            GetRawInputDeviceInfoW(
                handle,
                RIDI_DEVICENAME,
                Some(buffer.as_mut_ptr().cast()),
                &mut actual_size,
            )
        } == u32::MAX
        {
            return Err(super::win32_error("GetRawInputDeviceInfoW(data) failed"));
        }

        Ok(String::from_utf16_lossy(&buffer[..actual_size.min(buffer.len() as u32) as usize])
            .trim_end_matches('\0')
            .to_owned())
    }

    fn extract_hex(value: &str, prefix: &str) -> Option<String> {
        let upper = value.to_ascii_uppercase();
        let start = upper.find(prefix)?;
        let value = &value[start + prefix.len()..];
        let end = value
            .find(|character: char| !character.is_ascii_hexdigit())
            .unwrap_or(value.len());

        if end == 0 {
            None
        } else {
            u16::from_str_radix(&value[..end], 16)
                .ok()
                .map(|id| format!("{id:04x}"))
        }
    }

    fn hid_strings(device_path: &str) -> (Option<String>, Option<String>) {
        let path: Vec<u16> = device_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                Default::default(),
                HANDLE(std::ptr::null_mut()),
            )
        };

        let Ok(handle) = handle else {
            return (None, None);
        };
        if handle.is_invalid() {
            return (None, None);
        }

        let handle = OwnedHandle(handle);
        let mut product = [0u16; 256];
        let mut serial = [0u16; 256];

        let product = unsafe { HidD_GetProductString(handle.0, product.as_mut_ptr().cast(), 512) }
            .as_bool()
            .then(|| String::from_utf16_lossy(&product).trim_end_matches('\0').to_owned());

        let serial = unsafe {
            HidD_GetSerialNumberString(handle.0, serial.as_mut_ptr().cast(), 512)
        }
        .as_bool()
        .then(|| String::from_utf16_lossy(&serial).trim_end_matches('\0').to_owned())
        .filter(|value| !value.is_empty());

        (product, serial)
    }

    fn physical_path(target_path: &str) -> Option<String> {
        let guid = windows::core::GUID::from_values(
            0x4D1E55B2,
            0xF16F,
            0x11CF,
            [0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30],
        );

        let handle = unsafe {
            SetupDiGetClassDevsW(
                Some(&guid),
                None,
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }
        .ok()?;

        let mut index = 0u32;
        let result = loop {
            let mut iface_data = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };

            if unsafe { SetupDiEnumDeviceInterfaces(handle, None, &guid, index, &mut iface_data) }
                .is_err()
            {
                break None;
            }

            let mut required_size = 0u32;
            unsafe {
                let _ = SetupDiGetDeviceInterfaceDetailW(
                    handle,
                    &iface_data,
                    None,
                    0,
                    Some(&mut required_size),
                    None,
                );
            }

            let mut detail_buf = vec![0u8; required_size as usize];
            let detail_data = detail_buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            unsafe {
                (*detail_data).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            }

            let mut dev_info = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };

            if unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    handle,
                    &iface_data,
                    Some(detail_data),
                    required_size,
                    None,
                    Some(&mut dev_info),
                )
            }
            .is_ok()
            {
                let path = unsafe {
                    PCWSTR((*detail_data).DevicePath.as_ptr())
                        .to_string()
                        .unwrap_or_default()
                };

                if path.eq_ignore_ascii_case(target_path) {
                    if let Some(location) = Self::query_prop(
                        handle,
                        &dev_info,
                        &DEVPKEY_Device_LocationPaths,
                        DEVPROP_TYPE_STRING_LIST,
                    ) {
                        break Some(location);
                    }

                    break Self::query_prop(
                        handle,
                        &dev_info,
                        &DEVPKEY_Device_InstanceId,
                        DEVPROP_TYPE_STRING,
                    );
                }
            }

            index += 1;
        };

        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(handle);
        }

        result
    }

    fn query_prop(
        handle: HDEVINFO,
        dev_info: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
        expected_type: DEVPROPTYPE,
    ) -> Option<String> {
        let mut property_type = DEVPROPTYPE(0);
        let mut size = 0u32;
        unsafe {
            let _ = SetupDiGetDevicePropertyW(
                handle,
                dev_info,
                key,
                &mut property_type,
                None,
                Some(&mut size),
                0,
            );
        }

        if size == 0 || property_type != expected_type {
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        unsafe {
            SetupDiGetDevicePropertyW(
                handle,
                dev_info,
                key,
                &mut property_type,
                Some(&mut buffer),
                Some(&mut size),
                0,
            )
        }
        .ok()?;

        let values: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|chunk| u16::from_ne_bytes([chunk[0], chunk[1]]))
            .collect();
        let len = values.iter().position(|&value| value == 0).unwrap_or(values.len());
        (len != 0).then(|| String::from_utf16(&values[..len]).ok()).flatten()
    }
}

#[derive(Debug, Clone)]
struct DiscoveredKeyboardMetadata {
    keyboard: Keyboard,
}
