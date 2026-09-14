use super::win32_error;
use crate::keyboard_source::{Keyboard, KeyboardID, PortID};
use std::{ffi::c_void, io, mem::size_of};
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
    Foundation::{CloseHandle, HANDLE, LPARAM},
    Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
    UI::{
        Input::{
            GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, HRAWINPUT, RAWINPUT,
            RAWINPUTDEVICELIST, RAWINPUTHEADER, RID_INPUT, RIDI_DEVICENAME, RIM_TYPEKEYBOARD,
        },
        WindowsAndMessaging::{WM_KEYUP, WM_SYSKEYUP},
    },
};
use windows::core::PCWSTR;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DeviceHandle(isize);

impl From<HANDLE> for DeviceHandle {
    fn from(handle: HANDLE) -> Self {
        Self(handle.0 as isize)
    }
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredKeyboard {
    pub(crate) handle: DeviceHandle,
    pub(crate) keyboard: Keyboard,
}

pub(crate) struct DeviceEnumerator;

impl DeviceEnumerator {
    pub(crate) fn enumerate_keyboards() -> io::Result<Vec<DiscoveredKeyboard>> {
        let mut count = 0u32;
        if unsafe {
            GetRawInputDeviceList(None, &mut count, size_of::<RAWINPUTDEVICELIST>() as u32)
        } == u32::MAX
        {
            return Err(win32_error("GetRawInputDeviceList(size) failed"));
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
            return Err(win32_error("GetRawInputDeviceList(data) failed"));
        }

        Ok(devices
            .into_iter()
            .filter(|d| d.dwType == RIM_TYPEKEYBOARD)
            .filter_map(|d| {
                Self::keyboard_from_handle(d.hDevice).map(|k| DiscoveredKeyboard {
                    handle: d.hDevice.into(),
                    keyboard: k,
                })
            })
            .collect())
    }

    pub(crate) fn keyboard_from_handle(handle: HANDLE) -> Option<Keyboard> {
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
        if keyboard.keyboard_id.name.is_none() {
            keyboard.keyboard_id.name = product;
        }
        keyboard.keyboard_id.serial = serial;

        Some(keyboard)
    }

    fn device_name(handle: HANDLE) -> io::Result<String> {
        let mut size = 0u32;
        if unsafe { GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, None, &mut size) } == u32::MAX {
            return Err(win32_error("GetRawInputDeviceInfoW(size) failed"));
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
            return Err(win32_error("GetRawInputDeviceInfoW(data) failed"));
        }
        Ok(
            String::from_utf16_lossy(&buffer[..actual_size.min(buffer.len() as u32) as usize])
                .trim_end_matches('\0')
                .to_owned(),
        )
    }

    fn extract_hex(val: &str, prefix: &str) -> Option<String> {
        let upper_val = val.to_ascii_uppercase();
        let start = upper_val.find(prefix)?;

        let val = &val[start + prefix.len()..];
        let end = val
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(val.len());

        if end == 0 {
            None
        } else {
            u16::from_str_radix(&val[..end], 16)
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

        let mut p_buf = [0u16; 256];
        let mut s_buf = [0u16; 256];
        let product = unsafe { HidD_GetProductString(handle.0, p_buf.as_mut_ptr().cast(), 512) }
            .as_bool()
            .then(|| {
                String::from_utf16_lossy(&p_buf)
                    .trim_end_matches('\0')
                    .to_owned()
            });
        let serial =
            unsafe { HidD_GetSerialNumberString(handle.0, s_buf.as_mut_ptr().cast(), 512) }
                .as_bool()
                .then(|| {
                    String::from_utf16_lossy(&s_buf)
                        .trim_end_matches('\0')
                        .to_owned()
                })
                .filter(|s| !s.is_empty());
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
                    if let Some(loc) = Self::query_prop(
                        handle,
                        &dev_info,
                        &DEVPKEY_Device_LocationPaths,
                        DEVPROP_TYPE_STRING_LIST,
                    ) {
                        break Some(loc);
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
}

pub(crate) struct RawInput {
    value: Box<RAWINPUT>,
}

impl RawInput {
    pub(crate) fn from_message(lparam: LPARAM) -> io::Result<Self> {
        let mut size = 0u32;
        let handle = HRAWINPUT(lparam.0 as *mut c_void);
        if unsafe {
            GetRawInputData(
                handle,
                RID_INPUT,
                None,
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            )
        } == u32::MAX
        {
            return Err(win32_error("GetRawInputData(size) failed"));
        }
        let mut buf = vec![0u8; size as usize];
        if unsafe {
            GetRawInputData(
                handle,
                RID_INPUT,
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            )
        } == u32::MAX
        {
            return Err(win32_error("GetRawInputData(data) failed"));
        }

        let value = unsafe { Box::from_raw(buf.as_mut_ptr() as *mut RAWINPUT) };
        std::mem::forget(buf);

        Ok(Self { value })
    }

    pub(crate) fn device(&self) -> HANDLE {
        self.value.header.hDevice
    }

    pub(crate) fn is_keyboard(&self) -> bool {
        self.value.header.dwType == RIM_TYPEKEYBOARD.0
    }

    pub(crate) fn vkey(&self) -> u16 {
        unsafe { self.value.data.keyboard.VKey }
    }

    pub(crate) fn flags(&self) -> u16 {
        unsafe { self.value.data.keyboard.Flags }
    }

    pub(crate) fn is_key_up(&self) -> bool {
        (self.flags() & 0x01) != 0
            || matches!(
                unsafe { self.value.data.keyboard.Message },
                WM_KEYUP | WM_SYSKEYUP
            )
    }

    pub(crate) fn scancode(&self) -> u16 {
        unsafe { self.value.data.keyboard.MakeCode }
    }

    pub(crate) fn is_extended(&self) -> bool {
        (self.flags() & 0x02) != 0
    }
}
