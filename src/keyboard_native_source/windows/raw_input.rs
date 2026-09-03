use std::ffi::c_void;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::UI::Input::{GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTHEADER, RID_INPUT};

pub(crate) struct RawInput {
    value: Box<RAWINPUT>,
}

impl RawInput {
    pub(crate) fn from_message(lparam: LPARAM) -> Option<Self> {
        unsafe {
            let raw_handle = HRAWINPUT(lparam.0 as *mut c_void);
            let mut size = 0u32;

            let result = GetRawInputData(
                raw_handle,
                RID_INPUT,
                None,
                &mut size,
                std::mem::size_of::<RAWINPUTHEADER>() as u32,
            );

            if result == u32::MAX || size == 0 {
                return None;
            }

            if size as usize > std::mem::size_of::<RAWINPUT>() {
                return None;
            }

            let mut value = Box::<RAWINPUT>::new_uninit();

            let result = GetRawInputData(
                raw_handle,
                RID_INPUT,
                Some(value.as_mut_ptr() as *mut c_void),
                &mut size,
                std::mem::size_of::<RAWINPUTHEADER>() as u32,
            );

            if result == u32::MAX {
                return None;
            }

            Some(Self {
                value: value.assume_init(),
            })
        }
    }

    pub(crate) fn header(&self) -> &RAWINPUTHEADER {
        &self.value.header
    }

    pub(crate) fn keyboard_message(&self) -> Option<u32> {
        Some(unsafe { self.value.data.keyboard.Message })
    }
}
