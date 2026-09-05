use super::handles::OwnedHandle;

use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_GetProductString, HidD_GetSerialNumberString,
};

use windows::Win32::Foundation::HANDLE;

use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

use windows::core::PCWSTR;

pub(crate) fn strings(device_path: &str) -> (Option<String>, Option<String>) {
    let path: Vec<u16> = device_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let handle = unsafe {
        match CreateFileW(
            PCWSTR(path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            HANDLE(std::ptr::null_mut()),
        ) {
            Ok(handle) => handle,
            Err(_) => return (None, None),
        }
    };

    let Some(handle) = OwnedHandle::new(handle) else {
        return (None, None);
    };

    (
        hid_product_string(handle.get()),
        hid_serial_string(handle.get()),
    )
}

fn hid_product_string(handle: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 256];

    let ok = unsafe {
        HidD_GetProductString(
            handle,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 2) as u32,
        )
    };

    if !ok.as_bool() {
        return None;
    }

    utf16_buffer_to_string(&buffer)
}

fn hid_serial_string(handle: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 256];

    let ok = unsafe {
        HidD_GetSerialNumberString(
            handle,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 2) as u32,
        )
    };

    if !ok.as_bool() {
        return None;
    }

    utf16_buffer_to_string(&buffer).filter(|value| !value.is_empty())
}

fn utf16_buffer_to_string(buffer: &[u16]) -> Option<String> {
    let len = buffer
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(buffer.len());

    String::from_utf16(&buffer[..len]).ok()
}
