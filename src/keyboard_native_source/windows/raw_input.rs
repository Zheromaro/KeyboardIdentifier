use super::errors::win32_error;
use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use windows::Win32::Foundation::{HANDLE, LPARAM};
use windows::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, HRAWINPUT, RAWINPUT,
    RAWINPUTDEVICELIST, RAWINPUTHEADER, RID_INPUT, RIDI_DEVICENAME, RIM_TYPEKEYBOARD,
};
use windows::Win32::UI::WindowsAndMessaging::{WM_KEYDOWN, WM_SYSKEYDOWN};

pub(crate) fn list_keyboards() -> io::Result<Vec<HANDLE>> {
    Ok(list_devices()?
        .into_iter()
        .filter(|device| device.dwType == RIM_TYPEKEYBOARD)
        .map(|device| device.hDevice)
        .collect())
}

fn list_devices() -> io::Result<Vec<RAWINPUTDEVICELIST>> {
    let mut count = 0u32;

    let result =
        unsafe { GetRawInputDeviceList(None, &mut count, size_of::<RAWINPUTDEVICELIST>() as u32) };

    if result == u32::MAX {
        return Err(win32_error("GetRawInputDeviceList(size) failed"));
    }

    if count == 0 {
        return Ok(Vec::new());
    }

    let mut devices = vec![RAWINPUTDEVICELIST::default(); count as usize];

    let result = unsafe {
        GetRawInputDeviceList(
            Some(devices.as_mut_ptr()),
            &mut count,
            size_of::<RAWINPUTDEVICELIST>() as u32,
        )
    };

    if result == u32::MAX {
        return Err(win32_error("GetRawInputDeviceList(data) failed"));
    }

    devices.truncate(result as usize);

    Ok(devices)
}

pub(crate) fn device_name(handle: HANDLE) -> io::Result<String> {
    let mut size = 0u32;

    let result = unsafe { GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, None, &mut size) };

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

    let result = unsafe {
        GetRawInputDeviceInfoW(
            handle,
            RIDI_DEVICENAME,
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            &mut actual_size,
        )
    };

    if result == u32::MAX {
        return Err(win32_error("GetRawInputDeviceInfoW(data) failed"));
    }

    let len = actual_size.min(buffer.len() as u32) as usize;

    Ok(String::from_utf16_lossy(&buffer[..len])
        .trim_end_matches('\0')
        .to_owned())
}

pub(crate) struct RawInput {
    value: Box<RAWINPUT>,
}

impl RawInput {
    pub(crate) fn from_message(lparam: LPARAM) -> io::Result<Self> {
        let raw_handle = HRAWINPUT(lparam.0 as *mut c_void);

        let mut size = 0u32;

        let result = unsafe {
            GetRawInputData(
                raw_handle,
                RID_INPUT,
                None,
                &mut size,
                std::mem::size_of::<RAWINPUTHEADER>() as u32,
            )
        };

        if result == u32::MAX {
            return Err(win32_error("GetRawInputData(size) failed"));
        }

        if size == 0 {
            return Err(io::Error::other("GetRawInputData returned an empty input"));
        }

        if size as usize > std::mem::size_of::<RAWINPUT>() {
            return Err(io::Error::other("RAWINPUT buffer is larger than expected"));
        }

        let mut value = Box::<RAWINPUT>::new_uninit();

        let result = unsafe {
            GetRawInputData(
                raw_handle,
                RID_INPUT,
                Some(value.as_mut_ptr().cast::<c_void>()),
                &mut size,
                std::mem::size_of::<RAWINPUTHEADER>() as u32,
            )
        };

        if result == u32::MAX {
            return Err(win32_error("GetRawInputData(data) failed"));
        }

        // SAFETY:
        // GetRawInputData successfully populated the buffer.
        let value = unsafe { value.assume_init() };

        Ok(Self { value })
    }

    pub(crate) fn device(&self) -> HANDLE {
        self.value.header.hDevice
    }

    pub(crate) fn is_keyboard(&self) -> bool {
        self.value.header.dwType == RIM_TYPEKEYBOARD.0
    }

    pub(crate) fn is_key_down(&self) -> bool {
        // SAFETY:
        // This field belongs to the keyboard union member.
        let message = unsafe { self.value.data.keyboard.Message };

        matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN)
    }
}
