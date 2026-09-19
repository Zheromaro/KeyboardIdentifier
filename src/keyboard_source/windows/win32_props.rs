use std::mem::size_of;
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
};
use windows::core::PCWSTR;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) fn hid_strings(device_path: &str) -> (Option<String>, Option<String>) {
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

    let mut p_buf = [0u16; 256];
    let mut s_buf = [0u16; 256];
    let product = unsafe { HidD_GetProductString(handle.0, p_buf.as_mut_ptr().cast(), 512) }
        .as_bool()
        .then(|| {
            String::from_utf16_lossy(&p_buf)
                .trim_end_matches('\0')
                .to_owned()
        });
    let serial = unsafe { HidD_GetSerialNumberString(handle.0, s_buf.as_mut_ptr().cast(), 512) }
        .as_bool()
        .then(|| {
            String::from_utf16_lossy(&s_buf)
                .trim_end_matches('\0')
                .to_owned()
        })
        .filter(|s| !s.is_empty());
    (product, serial)
}

pub(crate) fn physical_path(target_path: &str) -> Option<String> {
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

        let mut req_size = 0u32;
        unsafe {
            let _ = SetupDiGetDeviceInterfaceDetailW(
                handle,
                &iface_data,
                None,
                0,
                Some(&mut req_size),
                None,
            );
        }

        let mut detail_buf = vec![0u8; req_size as usize];
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
                req_size,
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
                if let Some(loc) = query_prop(
                    handle,
                    &dev_info,
                    &DEVPKEY_Device_LocationPaths,
                    DEVPROP_TYPE_STRING_LIST,
                ) {
                    break Some(loc);
                }
                break query_prop(
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
    result.or_else(|| {
        let p = target_path.strip_prefix(r"\\?\")?.split("#{").next()?;
        Some(p.replace('#', r"\"))
    })
}

fn query_prop(
    handle: HDEVINFO,
    dev_info: &SP_DEVINFO_DATA,
    key: &DEVPROPKEY,
    expected_type: DEVPROPTYPE,
) -> Option<String> {
    let mut p_type = DEVPROPTYPE(0);
    let mut p_size = 0u32;
    unsafe {
        let _ = SetupDiGetDevicePropertyW(
            handle,
            dev_info,
            key,
            &mut p_type,
            None,
            Some(&mut p_size),
            0,
        );
    }
    if p_size == 0 || p_type != expected_type {
        return None;
    }

    let mut buf = vec![0u8; p_size as usize];
    unsafe {
        SetupDiGetDevicePropertyW(
            handle,
            dev_info,
            key,
            &mut p_type,
            Some(&mut buf),
            Some(&mut p_size),
            0,
        )
    }
    .ok()?;

    let values: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_ne_bytes([c[0], c[1]]))
        .collect();
    let len = values.iter().position(|&v| v == 0).unwrap_or(values.len());
    if len == 0 {
        None
    } else {
        String::from_utf16(&values[..len]).ok()
    }
}
