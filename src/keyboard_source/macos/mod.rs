mod c_callbacks;
mod ffi_declarations;
mod key_mapping;

use super::{Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID};
use c_callbacks::*;
use ffi_declarations::*;
use keyboard_types::Modifiers;
use std::{
    collections::HashMap,
    ffi::c_void,
    io, ptr,
    sync::{Arc, Mutex},
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::error;

const IOHID_OPTIONS_TYPE_NONE: u32 = 0;
const IOHID_OPTIONS_TYPE_SEIZE_DEVICE: u32 = 1;

pub(super) const USAGE_PAGE_GENERIC_DESKTOP: i32 = 1;
pub(super) const USAGE_KEYBOARD: i32 = 6;

/// Context state passed to the C callbacks.
struct HidContext {
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    keyboards: HashMap<isize, Arc<Keyboard>>,
    modifiers: HashMap<isize, Modifiers>,

    /// Currently connected devices.
    ///
    /// The raw `IOHIDDeviceRef` is stored as `usize` so this shared state
    /// remains Send + Sync.
    devices: Arc<Mutex<HashMap<isize, (Arc<Keyboard>, usize)>>>,

    /// Independently opened HID device references that are currently seized.
    consumed: Arc<Mutex<HashMap<isize, usize>>>,
}

pub struct MacosKeyboardSource {
    receiver: mpsc::Receiver<Result<KeyboardEvent, io::Error>>,
    shutdown: broadcast::Sender<()>,

    devices: Arc<Mutex<HashMap<isize, (Arc<Keyboard>, usize)>>>,
    consumed: Arc<Mutex<HashMap<isize, usize>>>,
}

impl KeyboardSource for MacosKeyboardSource {
    async fn new() -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel(128);
        let (shutdown, _) = broadcast::channel(1);
        let shutdown_rx = shutdown.subscribe();
        let (init_tx, init_rx) = oneshot::channel();

        let devices = Arc::new(Mutex::new(HashMap::new()));
        let consumed = Arc::new(Mutex::new(HashMap::new()));

        let devices_for_thread = devices.clone();
        let consumed_for_thread = consumed.clone();

        std::thread::spawn(move || {
            if let Err(error) =
                macos_hid_loop(sender, shutdown_rx, devices_for_thread, consumed_for_thread)
            {
                let _ = init_tx.send(Err(error));
                error!("macOS HID loop terminated unexpectedly");
            } else {
                let _ = init_tx.send(Ok(()));
            }
        });

        match init_rx.await {
            Ok(Ok(())) => Ok(Self {
                receiver,
                shutdown,
                devices,
                consumed,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::Other,
                "macOS initialization thread died unexpectedly",
            )),
        }
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

    async fn consume(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        consume_macos_keyboard(&self.devices, &self.consumed, keyboard)
    }

    async fn release(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        release_macos_keyboard(&self.devices, &self.consumed, keyboard)
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
    devices: Arc<Mutex<HashMap<isize, (Arc<Keyboard>, usize)>>>,
    consumed: Arc<Mutex<HashMap<isize, usize>>>,
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
            modifiers: HashMap::new(),
            devices: devices.clone(),
            consumed: consumed.clone(),
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

        if IOHIDManagerOpen(manager, IOHID_OPTIONS_TYPE_NONE) != 0 {
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

        IOHIDManagerUnscheduleFromRunLoop(manager, run_loop, default_mode);

        IOHIDManagerClose(manager, IOHID_OPTIONS_TYPE_NONE);

        CFRelease(default_mode);
        CFRelease(manager);

        // Release any devices still consumed when the source shuts down.
        if let Ok(mut consumed) = consumed.lock() {
            for (_, device_ptr) in consumed.drain() {
                let device = device_ptr as IOHIDDeviceRef;

                let _ = IOHIDDeviceClose(device, IOHID_OPTIONS_TYPE_NONE);

                CFRelease(device as CFTypeRef);
            }
        }

        let _ = Box::from_raw(context_ptr as *mut HidContext);
    }

    Ok(())
}

fn find_device_id(
    devices: &Mutex<HashMap<isize, (Arc<Keyboard>, usize)>>,
    keyboard: &Keyboard,
) -> io::Result<(isize, usize)> {
    let devices = devices.lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::Other,
            "macOS HID device state mutex is poisoned",
        )
    })?;

    devices
        .iter()
        .find_map(|(&device_id, (kb, device_ptr))| {
            if kb.as_ref() == keyboard {
                Some((device_id, *device_ptr))
            } else {
                None
            }
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Keyboard is not currently connected",
            )
        })
}

fn consume_macos_keyboard(
    devices: &Mutex<HashMap<isize, (Arc<Keyboard>, usize)>>,
    consumed: &Mutex<HashMap<isize, usize>>,
    keyboard: &Keyboard,
) -> io::Result<()> {
    let (device_id, device_ptr) = find_device_id(devices, keyboard)?;

    let mut consumed_devices = consumed.lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::Other,
            "macOS consumed-device state mutex is poisoned",
        )
    })?;

    if consumed_devices.contains_key(&device_id) {
        return Ok(());
    }

    unsafe {
        let device = device_ptr as IOHIDDeviceRef;

        let service = IOHIDDeviceGetService(device);

        if service == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Failed to obtain macOS HID service for keyboard",
            ));
        }

        // Create an independent HID device reference.
        let seized_device = IOHIDDeviceCreate(ptr::null_mut(), service);

        if seized_device.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create macOS HID device reference",
            ));
        }

        let result = IOHIDDeviceOpen(seized_device, IOHID_OPTIONS_TYPE_SEIZE_DEVICE);

        if result != 0 {
            CFRelease(seized_device as CFTypeRef);

            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Failed to seize macOS keyboard: 0x{:08x}", result as u32),
            ));
        }

        consumed_devices.insert(device_id, seized_device as usize);
    }

    Ok(())
}

fn release_macos_keyboard(
    devices: &Mutex<HashMap<isize, (Arc<Keyboard>, usize)>>,
    consumed: &Mutex<HashMap<isize, usize>>,
    keyboard: &Keyboard,
) -> io::Result<()> {
    let (device_id, _) = find_device_id(devices, keyboard)?;

    let consumed_device = {
        let mut consumed_devices = consumed.lock().map_err(|_| {
            io::Error::new(
                io::ErrorKind::Other,
                "macOS consumed-device state mutex is poisoned",
            )
        })?;

        consumed_devices.remove(&device_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Keyboard is not currently consumed",
            )
        })?
    };

    unsafe {
        let device = consumed_device as IOHIDDeviceRef;

        let result = IOHIDDeviceClose(device, IOHID_OPTIONS_TYPE_NONE);

        CFRelease(device as CFTypeRef);

        if result != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to release macOS keyboard: 0x{:08x}", result as u32),
            ));
        }
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
