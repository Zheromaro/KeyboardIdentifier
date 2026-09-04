// windows/enumerator.rs
use super::errors::{handle_key, win32_error};
use super::path_parser::KeyboardPathParser;
use crate::keyboard_source::Keyboard;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW, SetupDiGetDevicePropertyW,
    SetupDiOpenDeviceInterfaceW,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_GetProductString, HidD_GetSerialNumberString,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_InstanceId, DEVPKEY_Device_LocationPaths, DEVPROP_TYPE_STRING,
    DEVPROP_TYPE_STRING_LIST, DEVPROPKEY, DEVPROPTYPE,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICENAME,
    RIM_TYPEKEYBOARD,
};
use windows::core::PCWSTR;

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

        let physical_path = physical_path(&path);

        let mut keyboard = KeyboardPathParser::parse(&path, physical_path);

        let (product, hid_serial) = get_hid_strings(&path);

        if keyboard.keyboard_id.name.is_none() {
            keyboard.keyboard_id.name = product;
        }

        keyboard.keyboard_id.serial = hid_serial.filter(|s| !s.is_empty());

        Some(keyboard)
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

            let result =
                GetRawInputDeviceList(None, &mut count, size_of::<RAWINPUTDEVICELIST>() as u32);

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
                size_of::<RAWINPUTDEVICELIST>() as u32,
            );

            if result == u32::MAX {
                return Err(win32_error("GetRawInputDeviceList(data) failed"));
            }

            devices.truncate(result as usize);

            Ok(devices)
        }
    }
}

fn physical_path(device_interface_path: &str) -> Option<String> {
    unsafe {
        let device_info_set = SetupDiGetClassDevsW(
            None,
            None,
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE | DIGCF_ALLCLASSES,
        )
        .ok()?;

        let result = physical_path_from_device_info_set(device_info_set, device_interface_path);
        let _ = SetupDiDestroyDeviceInfoList(device_info_set);

        // If SetupAPI fails (common for PS/2 keyboards), parse the instance ID from the path
        result.or_else(|| parse_instance_id_from_path(device_interface_path))
    }
}

/// Parses `\\?\HID#VID_vvvv&PID_pppp#instance#{guid}` into `HID\VID_vvvv&PID_pppp\instance`.
/// This is the Windows equivalent of Linux's physical path / ID_PATH.
fn parse_instance_id_from_path(path: &str) -> Option<String> {
    let path = path.strip_prefix(r"\\?\")?;
    let path = path.split("#{").next()?; // Remove the GUID suffix
    Some(path.replace('#', r"\"))
}

unsafe fn physical_path_from_device_info_set(
    device_info_set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    device_interface_path: &str,
) -> Option<String> {
    let wide_path: Vec<u16> = device_interface_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut interface_data = SP_DEVICE_INTERFACE_DATA {
        cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
        ..Default::default()
    };

    SetupDiOpenDeviceInterfaceW(
        device_info_set,
        PCWSTR(wide_path.as_ptr()),
        0,
        Some(&mut interface_data),
    )
    .ok()?;

    // --- First call: get required buffer size ---
    let mut required_size = 0u32;
    let _ = SetupDiGetDeviceInterfaceDetailW(
        device_info_set,
        &interface_data,
        None,
        0,
        Some(&mut required_size),
        None,
    );

    if required_size == 0 {
        return None;
    }

    // --- Allocate detail data and get SP_DEVINFO_DATA ---
    let mut detail_buffer = vec![0u8; required_size as usize];
    let detail_data = detail_buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
    (*detail_data).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;

    let mut dev_info_data = SP_DEVINFO_DATA {
        cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };

    SetupDiGetDeviceInterfaceDetailW(
        device_info_set,
        &interface_data,
        Some(detail_data),
        required_size,
        None,
        Some(&mut dev_info_data),
    )
    .ok()?;

    // Try LocationPaths first (e.g. "{PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(1)}")
    if let Some(path) = query_string_list_property(
        device_info_set,
        &dev_info_data,
        &DEVPKEY_Device_LocationPaths,
    ) {
        return Some(path);
    }

    // Fallback to InstanceId (e.g. "HID\VID_046D&PID_C52B\6&2C7A4C79&0&0000")
    query_string_property(device_info_set, &dev_info_data, &DEVPKEY_Device_InstanceId)
}

unsafe fn query_string_list_property(
    device_info_set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    dev_info_data: &SP_DEVINFO_DATA,
    property_key: &DEVPROPKEY,
) -> Option<String> {
    let mut property_type = DEVPROPTYPE(0);
    let mut property_size = 0u32;

    let _ = SetupDiGetDevicePropertyW(
        device_info_set,
        dev_info_data,
        property_key,
        &mut property_type,
        None,
        Some(&mut property_size),
        0,
    );

    if property_size == 0 || property_type != DEVPROP_TYPE_STRING_LIST {
        return None;
    }

    let mut buffer = vec![0u8; property_size as usize];

    SetupDiGetDevicePropertyW(
        device_info_set,
        dev_info_data,
        property_key,
        &mut property_type,
        Some(&mut buffer),
        Some(&mut property_size),
        0,
    )
    .ok()?;

    let values = std::slice::from_raw_parts(
        buffer.as_ptr() as *const u16,
        buffer.len() / size_of::<u16>(),
    );

    let first = values.split(|&value| value == 0).next()?;

    if first.is_empty() {
        return None;
    }

    String::from_utf16(first).ok()
}

unsafe fn query_string_property(
    device_info_set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    dev_info_data: &SP_DEVINFO_DATA,
    property_key: &DEVPROPKEY,
) -> Option<String> {
    let mut property_type = DEVPROPTYPE(0);
    let mut property_size = 0u32;

    let _ = SetupDiGetDevicePropertyW(
        device_info_set,
        dev_info_data,
        property_key,
        &mut property_type,
        None,
        Some(&mut property_size),
        0,
    );

    if property_size == 0 || property_type != DEVPROP_TYPE_STRING {
        return None;
    }

    let mut buffer = vec![0u8; property_size as usize];

    SetupDiGetDevicePropertyW(
        device_info_set,
        dev_info_data,
        property_key,
        &mut property_type,
        Some(&mut buffer),
        Some(&mut property_size),
        0,
    )
    .ok()?;

    let values = std::slice::from_raw_parts(
        buffer.as_ptr() as *const u16,
        buffer.len() / size_of::<u16>(),
    );

    let len = values.iter().position(|&c| c == 0).unwrap_or(values.len());

    if len == 0 {
        return None;
    }

    String::from_utf16(&values[..len]).ok()
}

/// Opens the HID device path and queries the product and serial strings.
fn get_hid_strings(device_path: &str) -> (Option<String>, Option<String>) {
    let path: Vec<u16> = device_path.encode_utf16().chain(Some(0)).collect();

    unsafe {
        let handle = match CreateFileW(
            PCWSTR(path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            HANDLE(std::ptr::null_mut()),
        ) {
            Ok(h) => h,
            Err(_) => return (None, None),
        };

        if handle.is_invalid() {
            let _ = CloseHandle(handle);
            return (None, None);
        }

        let product = {
            let mut buf = [0u16; 256];

            let ok =
                HidD_GetProductString(handle, buf.as_mut_ptr() as *mut _, (buf.len() * 2) as u32);

            if ok.as_bool() {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());

                String::from_utf16(&buf[..len]).ok()
            } else {
                None
            }
        };

        let serial = {
            let mut buf = [0u16; 256];

            let ok = HidD_GetSerialNumberString(
                handle,
                buf.as_mut_ptr() as *mut _,
                (buf.len() * 2) as u32,
            );

            if ok.as_bool() {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());

                String::from_utf16(&buf[..len])
                    .ok()
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        };

        let _ = CloseHandle(handle);

        (product, serial)
    }
}
