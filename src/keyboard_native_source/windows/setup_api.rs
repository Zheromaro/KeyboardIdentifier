use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    SetupDiGetDevicePropertyW,
};

use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_InstanceId, DEVPKEY_Device_LocationPaths, DEVPROP_TYPE_STRING,
    DEVPROP_TYPE_STRING_LIST, DEVPROPKEY, DEVPROPTYPE,
};

use windows::core::PCWSTR;

const GUID_DEVINTERFACE_HID: windows::core::GUID = windows::core::GUID::from_values(
    0x4D1E55B2,
    0xF16F,
    0x11CF,
    [0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30],
);

pub(crate) fn physical_path(device_interface_path: &str) -> Option<String> {
    let info_set = DeviceInfoSet::new()?;

    physical_path_by_enumeration(info_set.get(), device_interface_path)
        .or_else(|| parse_instance_id_from_path(device_interface_path))
}

struct DeviceInfoSet(HDEVINFO);

impl DeviceInfoSet {
    fn new() -> Option<Self> {
        let handle = unsafe {
            SetupDiGetClassDevsW(
                Some(&GUID_DEVINTERFACE_HID),
                None,
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }
        .ok()?;

        Some(Self(handle))
    }

    fn get(&self) -> HDEVINFO {
        self.0
    }
}

impl Drop for DeviceInfoSet {
    fn drop(&mut self) {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

fn physical_path_by_enumeration(device_info_set: HDEVINFO, target_path: &str) -> Option<String> {
    let mut index = 0u32;

    loop {
        let mut interface_data = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };

        let result = unsafe {
            SetupDiEnumDeviceInterfaces(
                device_info_set,
                None,
                &GUID_DEVINTERFACE_HID,
                index,
                &mut interface_data,
            )
        };

        if result.is_err() {
            break;
        }

        let mut required_size = 0u32;

        unsafe {
            let _ = SetupDiGetDeviceInterfaceDetailW(
                device_info_set,
                &interface_data,
                None,
                0,
                Some(&mut required_size),
                None,
            );
        }

        if required_size == 0 {
            index += 1;
            continue;
        }

        let mut detail_buffer = vec![0u8; required_size as usize];

        let detail_data = detail_buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;

        // SAFETY:
        // `detail_buffer` has exactly the size requested by SetupAPI.
        // The buffer is owned by this function and remains alive until
        // `SetupDiGetDeviceInterfaceDetailW` returns.
        unsafe {
            (*detail_data).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }

        let mut dev_info_data = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };

        let result = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                device_info_set,
                &interface_data,
                Some(detail_data),
                required_size,
                None,
                Some(&mut dev_info_data),
            )
        };

        if result.is_err() {
            index += 1;
            continue;
        }

        let path = unsafe {
            PCWSTR((*detail_data).DevicePath.as_ptr())
                .to_string()
                .unwrap_or_default()
        };

        if path.eq_ignore_ascii_case(target_path) {
            if let Some(location) = query_string_list_property(
                device_info_set,
                &dev_info_data,
                &DEVPKEY_Device_LocationPaths,
            ) {
                return Some(location);
            }

            return query_string_property(
                device_info_set,
                &dev_info_data,
                &DEVPKEY_Device_InstanceId,
            );
        }

        index += 1;
    }

    None
}

fn query_string_list_property(
    device_info_set: HDEVINFO,
    dev_info_data: &SP_DEVINFO_DATA,
    property_key: &DEVPROPKEY,
) -> Option<String> {
    let mut property_type = DEVPROPTYPE(0);
    let mut property_size = 0u32;

    unsafe {
        let _ = SetupDiGetDevicePropertyW(
            device_info_set,
            dev_info_data,
            property_key,
            &mut property_type,
            None,
            Some(&mut property_size),
            0,
        );
    }

    if property_size == 0 || property_type != DEVPROP_TYPE_STRING_LIST {
        return None;
    }

    let mut buffer = vec![0u8; property_size as usize];

    unsafe {
        SetupDiGetDevicePropertyW(
            device_info_set,
            dev_info_data,
            property_key,
            &mut property_type,
            Some(&mut buffer),
            Some(&mut property_size),
            0,
        )
    }
    .ok()?;

    let values = utf16_values(&buffer);

    let first = values.split(|&value| value == 0).next()?;

    if first.is_empty() {
        return None;
    }

    String::from_utf16(first).ok()
}

fn query_string_property(
    device_info_set: HDEVINFO,
    dev_info_data: &SP_DEVINFO_DATA,
    property_key: &DEVPROPKEY,
) -> Option<String> {
    let mut property_type = DEVPROPTYPE(0);
    let mut property_size = 0u32;

    unsafe {
        let _ = SetupDiGetDevicePropertyW(
            device_info_set,
            dev_info_data,
            property_key,
            &mut property_type,
            None,
            Some(&mut property_size),
            0,
        );
    }

    if property_size == 0 || property_type != DEVPROP_TYPE_STRING {
        return None;
    }

    let mut buffer = vec![0u8; property_size as usize];

    unsafe {
        SetupDiGetDevicePropertyW(
            device_info_set,
            dev_info_data,
            property_key,
            &mut property_type,
            Some(&mut buffer),
            Some(&mut property_size),
            0,
        )
    }
    .ok()?;

    let values = utf16_values(&buffer);

    let len = values
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(values.len());

    if len == 0 {
        return None;
    }

    String::from_utf16(&values[..len]).ok()
}

fn utf16_values(buffer: &[u8]) -> Vec<u16> {
    buffer
        .chunks_exact(2)
        .map(|chunk| u16::from_ne_bytes([chunk[0], chunk[1]]))
        .collect()
}

fn parse_instance_id_from_path(path: &str) -> Option<String> {
    let path = path.strip_prefix(r"\\?\")?;
    let path = path.split("#{").next()?;

    Some(path.replace('#', r"\"))
}
