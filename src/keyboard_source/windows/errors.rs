use std::io;

use windows::Win32::Foundation::GetLastError;

pub(crate) fn win32_error(message: &'static str) -> io::Error {
    let code = unsafe { GetLastError().0 as i32 };

    if code == 0 {
        io::Error::other(message)
    } else {
        io::Error::from_raw_os_error(code)
    }
}

pub(crate) fn windows_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error)
}
