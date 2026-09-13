mod c_callbacks;
mod ffi_declarations;

use super::{Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID};
use c_callbacks::*;
use ffi_declarations::*;
use std::{collections::HashMap, ffi::c_void, io, ptr, sync::Arc};
use tokio::sync::{broadcast, mpsc};
use tracing::error; // Removed unused `warn`

const PRESSED: i32 = 1;
const USAGE_PAGE_GENERIC_DESKTOP: i32 = 1;
const USAGE_KEYBOARD: i32 = 6;

/// Context state passed to the C callbacks.
struct HidContext {
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    keyboards: HashMap<isize, Arc<Keyboard>>,
}

pub struct MacosKeyboardSource {
    receiver: mpsc::Receiver<Result<KeyboardEvent, io::Error>>,
    shutdown: broadcast::Sender<()>,
}

impl KeyboardSource for MacosKeyboardSource {
    async fn new() -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel(128);
        let (shutdown, _) = broadcast::channel(1);
        let shutdown_rx = shutdown.subscribe();
        let sender_clone = sender.clone();

        std::thread::spawn(move || {
            if let Err(error) = macos_hid_loop(sender_clone, shutdown_rx) {
                error!(error = %error, "macOS HID loop terminated unexpectedly");
            }
        });
        Ok(Self { receiver, shutdown })
    }

    async fn receive_event(&mut self) -> Result<KeyboardEvent, io::Error> {
        match self.receiver.recv().await {
            Some(result) => result,
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "macOS device event channel closed",
            )),
        }
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        match enumerate_macos_keyboards() {
            Ok(keyboards) => keyboards,
            Err(error) => {
                error!(error = %error, "Failed to enumerate macOS keyboards");
                Vec::new()
            }
        }
    }
}

impl Drop for MacosKeyboardSource {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}

// --- Core OS Integrations ---
fn macos_hid_loop(
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    mut shutdown: broadcast::Receiver<()>,
) -> io::Result<()> {
    unsafe {
        let manager = IOHIDManagerCreate(ptr::null_mut(), 0);
        if manager.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create IOHIDManager",
            ));
        }

        let matching_dict = create_matching_dictionary();
        IOHIDManagerSetDeviceMatching(manager, matching_dict);
        CFRelease(matching_dict);

        let context = Box::new(HidContext {
            sender,
            keyboards: HashMap::new(),
        });
        let context_ptr = Box::into_raw(context) as *mut c_void;

        IOHIDManagerRegisterDeviceMatchingCallback(manager, matching_callback, context_ptr);
        IOHIDManagerRegisterDeviceRemovalCallback(manager, removal_callback, context_ptr);
        IOHIDManagerRegisterInputValueCallback(manager, input_callback, context_ptr);

        let run_loop = CFRunLoopGetCurrent();
        let default_mode = CFStringCreateWithCString(
            ptr::null_mut(),
            b"kCFRunLoopDefaultMode\0".as_ptr() as _,
            0x08000100,
        );

        IOHIDManagerScheduleWithRunLoop(manager, run_loop, default_mode);

        if IOHIDManagerOpen(manager, 0) != 0 {
            let _ = Box::from_raw(context_ptr as *mut HidContext);
            CFRelease(default_mode);
            CFRelease(manager);
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Failed to open IOHIDManager",
            ));
        }

        let run_loop_ptr = run_loop as usize;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let _ = shutdown.recv().await;
            });
            CFRunLoopStop(run_loop_ptr as CFRunLoopRef);
        });

        CFRunLoopRun();

        // Cleanup
        IOHIDManagerUnscheduleFromRunLoop(manager, run_loop, default_mode);
        IOHIDManagerClose(manager, 0);
        CFRelease(default_mode);
        CFRelease(manager);
        let _ = Box::from_raw(context_ptr as *mut HidContext);
    }
    Ok(())
}

fn enumerate_macos_keyboards() -> Result<Vec<Keyboard>, io::Error> {
    unsafe {
        let manager = IOHIDManagerCreate(ptr::null_mut(), 0);
        if manager.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create IOHIDManager",
            ));
        }

        let matching_dict = create_matching_dictionary();
        IOHIDManagerSetDeviceMatching(manager, matching_dict);
        CFRelease(matching_dict);

        let device_set = IOHIDManagerCopyDevices(manager);
        CFRelease(manager);

        if device_set.is_null() {
            return Ok(Vec::new());
        }

        let count = CFSetGetCount(device_set);
        let mut devices: Vec<IOHIDDeviceRef> = vec![ptr::null_mut(); count as usize];
        CFSetGetValues(device_set, devices.as_mut_ptr() as _);

        let keyboards = devices.into_iter().filter_map(map_to_keyboard).collect();
        CFRelease(device_set);

        Ok(keyboards)
    }
}

pub(super) fn map_to_keyboard(device: IOHIDDeviceRef) -> Option<Keyboard> {
    let name = get_string_property(device, b"Product\0");
    let vendor_id = get_int_property(device, b"VendorID\0").map(|v| format!("{:04x}", v));
    let product_id = get_int_property(device, b"ProductID\0").map(|p| format!("{:04x}", p));
    let serial = get_string_property(device, b"SerialNumber\0");
    let physical_path = get_int_property(device, b"LocationID\0").map(|l| format!("{:x}", l));

    if name.is_none() && vendor_id.is_none() && product_id.is_none() {
        return None;
    }

    Some(Keyboard {
        keyboard_id: KeyboardID {
            name,
            vendor_id,
            product_id,
            serial,
        },
        port_id: PortID { physical_path },
    })
}
