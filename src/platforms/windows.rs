use crate::keyboard_provider::{DeviceProvider, Keyboard, KeyboardID, PortID, ProviderEvent};
use std::future::Future;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICENAME,
    RIM_TYPEKEYBOARD,
};

pub struct WindowsDeviceProvider {
    // A tokio::sync::mpsc::Receiver will eventually be stored here
    // to await hotplug and keystroke messages from the background thread.
}

impl WindowsDeviceProvider {
    /// Extracts VID, PID, and Serial from a Windows Raw Input device name.
    fn parse_device_path(path: &str) -> KeyboardID {
        let mut vendor_id = None;
        let mut product_id = None;
        let mut serial = None;
        let upper_path = path.to_uppercase();

        if let Some(vid_idx) = upper_path.find("VID_") {
            if vid_idx + 8 <= upper_path.len() {
                vendor_id = Some(upper_path[vid_idx + 4..vid_idx + 8].to_string());
            }
        }

        if let Some(pid_idx) = upper_path.find("PID_") {
            if pid_idx + 8 <= upper_path.len() {
                product_id = Some(upper_path[pid_idx + 4..pid_idx + 8].to_string());
            }
        }

        // Serial numbers are occasionally stored in the 3rd section of the instance ID string
        let segments: Vec<&str> = path.split('#').collect();
        if segments.len() >= 3 && !segments[2].contains('&') {
            serial = Some(segments[2].to_string());
        }

        KeyboardID {
            name: Some("Windows Raw Input Keyboard".to_string()),
            vendor_id,
            product_id,
            serial,
        }
    }
}

impl DeviceProvider for WindowsDeviceProvider {
    fn get_keyboards(&self) -> Vec<Keyboard> {
        let mut num_devices = 0;

        unsafe {
            GetRawInputDeviceList(
                None,
                &mut num_devices,
                std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
            );
        }

        if num_devices == 0 {
            return Vec::new();
        }

        let mut device_list = vec![RAWINPUTDEVICELIST::default(); num_devices as usize];
        unsafe {
            GetRawInputDeviceList(
                Some(device_list.as_mut_ptr()),
                &mut num_devices,
                std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
            );
        }

        let mut keyboards = Vec::new();

        for device in device_list.iter() {
            if device.dwType == RIM_TYPEKEYBOARD {
                let mut name_size = 0;

                unsafe {
                    GetRawInputDeviceInfoW(device.hDevice, RIDI_DEVICENAME, None, &mut name_size);
                }

                if name_size > 0 {
                    let mut name_buffer: Vec<u16> = vec![0; name_size as usize];

                    unsafe {
                        GetRawInputDeviceInfoW(
                            device.hDevice,
                            RIDI_DEVICENAME,
                            Some(name_buffer.as_mut_ptr() as *mut _),
                            &mut name_size,
                        );
                    }

                    // Convert null-terminated UTF-16 buffer to a standard Rust string
                    let path = String::from_utf16_lossy(&name_buffer)
                        .trim_end_matches('\0')
                        .to_string();

                    let keyboard_id = Self::parse_device_path(&path);

                    keyboards.push(Keyboard {
                        keyboard_id,
                        port_id: PortID {
                            physical_path: Some(path),
                        },
                    });
                }
            }
        }

        keyboards
    }

    fn next_event(&self) -> impl Future<Output = Result<ProviderEvent, std::io::Error>> + Send {
        async {
            // Note: To return Plugged, Unplugged, and Pressed events on Windows,
            // you must await messages from a hidden background window handling WM_INPUT.
            std::future::pending().await
        }
    }
}
