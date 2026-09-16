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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::JoinHandle,
};
use tokio::sync::oneshot;
use tracing::error;

const IOHID_OPTIONS_TYPE_NONE: u32 = 0;
const IOHID_OPTIONS_TYPE_SEIZE_DEVICE: u32 = 1;

/// Maximum amount of time the HID run loop waits before checking
/// for commands/shutdown.
const RUN_LOOP_TICK_SECONDS: f64 = 0.01;

pub(super) const USAGE_PAGE_GENERIC_DESKTOP: i32 = 1;
pub(super) const USAGE_KEYBOARD: i32 = 6;

/// Identifies an IOHID device for the lifetime of the HID manager.
///
/// This is only an in-process bookkeeping identifier. It must never be
/// treated as a persistent device identifier.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct DeviceId(usize);

impl From<IOHIDDeviceRef> for DeviceId {
    fn from(device: IOHIDDeviceRef) -> Self {
        Self(device as usize)
    }
}

/// State owned exclusively by the macOS HID thread.
///
/// The raw IOKit references in this structure never cross into Tokio.
struct DeviceState {
    /// Keyboard description associated with this HID device.
    keyboard: Arc<Keyboard>,

    /// Reference supplied by IOHIDManager.
    ///
    /// The manager owns this reference. We only borrow it while the
    /// corresponding device is registered with the manager.
    device: IOHIDDeviceRef,

    /// Independently-created device reference used for seizure.
    ///
    /// We own this reference and must close/release it.
    consumed_device: Option<IOHIDDeviceRef>,
}

enum HidCommand {
    Consume {
        keyboard: Keyboard,
        response: oneshot::Sender<io::Result<()>>,
    },

    Release {
        keyboard: Keyboard,
        response: oneshot::Sender<io::Result<()>>,
    },
}

struct HidContext {
    sender: tokio::sync::mpsc::Sender<Result<KeyboardEvent, io::Error>>,

    /// All active keyboards known by the HID manager.
    keyboards: HashMap<DeviceId, Arc<Keyboard>>,

    /// Current modifier state per physical HID device.
    modifiers: HashMap<DeviceId, Modifiers>,

    /// Complete HID device state.
    ///
    /// All access occurs on the HID thread.
    devices: HashMap<DeviceId, DeviceState>,
}

pub struct MacosKeyboardSource {
    receiver: tokio::sync::mpsc::Receiver<Result<KeyboardEvent, io::Error>>,

    command_sender: Sender<HidCommand>,

    shutdown: Arc<AtomicBool>,

    join_handle: Option<JoinHandle<()>>,
}

impl KeyboardSource for MacosKeyboardSource {
    async fn new() -> io::Result<Self> {
        let (event_sender, receiver) = tokio::sync::mpsc::channel(128);

        let (command_sender, command_receiver) = mpsc::channel();

        let shutdown = Arc::new(AtomicBool::new(false));

        let shutdown_for_thread = Arc::clone(&shutdown);

        let (init_sender, init_receiver) = oneshot::channel();

        let join_handle = std::thread::spawn(move || {
            if let Err(error) = macos_hid_loop(
                event_sender,
                command_receiver,
                shutdown_for_thread,
                init_sender,
            ) {
                error!(
                    error = %error,
                    "macOS HID loop terminated unexpectedly"
                );
            }
        });

        match init_receiver.await {
            Ok(Ok(())) => Ok(Self {
                receiver,
                command_sender,
                shutdown,
                join_handle: Some(join_handle),
            }),

            Ok(Err(error)) => {
                shutdown.store(true, Ordering::Release);

                let _ = join_handle.join();

                Err(error)
            }

            Err(_) => {
                shutdown.store(true, Ordering::Release);

                let _ = join_handle.join();

                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "macOS HID initialization thread died unexpectedly",
                ))
            }
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
                error!(
                    error = %error,
                    "Failed to enumerate macOS keyboards"
                );

                Vec::new()
            }
        }
    }

    async fn consume(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        let (response_sender, response_receiver) = oneshot::channel();

        self.command_sender
            .send(HidCommand::Consume {
                keyboard: keyboard.clone(),
                response: response_sender,
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "macOS HID thread is no longer running",
                )
            })?;

        response_receiver.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "macOS HID thread stopped before consume completed",
            )
        })?
    }

    async fn release(&mut self, keyboard: &Keyboard) -> io::Result<()> {
        let (response_sender, response_receiver) = oneshot::channel();

        self.command_sender
            .send(HidCommand::Release {
                keyboard: keyboard.clone(),
                response: response_sender,
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "macOS HID thread is no longer running",
                )
            })?;

        response_receiver.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "macOS HID thread stopped before release completed",
            )
        })?
    }
}

impl Drop for MacosKeyboardSource {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);

        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

// ============================================================
// HID Thread
// ============================================================

fn macos_hid_loop(
    sender: tokio::sync::mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    command_receiver: Receiver<HidCommand>,
    shutdown: Arc<AtomicBool>,
    init_sender: oneshot::Sender<io::Result<()>>,
) -> io::Result<()> {
    unsafe {
        let manager = IOHIDManagerCreate(ptr::null_mut(), 0);

        if manager.is_null() {
            let error = io::Error::new(io::ErrorKind::Other, "Failed to create IOHIDManager");

            let _ = init_sender.send(Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create IOHIDManager",
            )));

            return Err(error);
        }

        let matching_dict = match create_matching_dictionary() {
            Some(dict) => dict,

            None => {
                CFRelease(manager);

                let error = io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create macOS HID matching dictionary",
                );

                let _ = init_sender.send(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create macOS HID matching dictionary",
                )));

                return Err(error);
            }
        };

        IOHIDManagerSetDeviceMatching(manager, matching_dict);

        CFRelease(matching_dict);

        let context = Box::new(HidContext {
            sender,
            keyboards: HashMap::new(),
            modifiers: HashMap::new(),
            devices: HashMap::new(),
        });

        let context_ptr = Box::into_raw(context) as *mut c_void;

        IOHIDManagerRegisterDeviceMatchingCallback(manager, matching_callback, context_ptr);

        IOHIDManagerRegisterDeviceRemovalCallback(manager, removal_callback, context_ptr);

        IOHIDManagerRegisterInputValueCallback(manager, input_callback, context_ptr);

        let run_loop = CFRunLoopGetCurrent();

        if run_loop.is_null() {
            let _ = Box::from_raw(context_ptr as *mut HidContext);

            CFRelease(manager);

            let error = io::Error::new(io::ErrorKind::Other, "Failed to obtain current CFRunLoop");

            let _ = init_sender.send(Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to obtain current CFRunLoop",
            )));

            return Err(error);
        }

        let default_mode = match create_cf_string("kCFRunLoopDefaultMode") {
            Some(mode) => mode,

            None => {
                let _ = Box::from_raw(context_ptr as *mut HidContext);

                CFRelease(manager);

                let error = io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create CFRunLoop mode string",
                );

                let _ = init_sender.send(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create CFRunLoop mode string",
                )));

                return Err(error);
            }
        };

        IOHIDManagerScheduleWithRunLoop(manager, run_loop, default_mode);

        let result = IOHIDManagerOpen(manager, IOHID_OPTIONS_TYPE_NONE);

        if result != 0 {
            let _ = Box::from_raw(context_ptr as *mut HidContext);

            CFRelease(default_mode);
            CFRelease(manager);

            let error = io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Failed to open IOHIDManager: 0x{:08x}", result as u32),
            );

            let _ = init_sender.send(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Failed to open IOHIDManager: 0x{:08x}", result as u32),
            )));

            return Err(error);
        }

        // Initialization is complete as soon as the HID manager has
        // successfully opened. The run loop has not ended yet.
        let _ = init_sender.send(Ok(()));

        // --------------------------------------------------------
        // Main HID thread loop
        // --------------------------------------------------------
        while !shutdown.load(Ordering::Acquire) {
            // Run the CFRunLoop for a short period. HID callbacks are
            // dispatched during this call.
            let _ = CFRunLoopRunInMode(default_mode, RUN_LOOP_TICK_SECONDS, 0);

            // Commands are deliberately processed on this same thread
            // that owns the IOKit device references.
            while let Ok(command) = command_receiver.try_recv() {
                process_command(command, context_ptr);
            }
        }

        // --------------------------------------------------------
        // Shutdown
        // --------------------------------------------------------

        // Drop any commands that raced with shutdown, returning an
        // appropriate error to their callers.
        while let Ok(command) = command_receiver.try_recv() {
            let error = io::Error::new(
                io::ErrorKind::Interrupted,
                "macOS HID source is shutting down",
            );

            match command {
                HidCommand::Consume { response, .. } | HidCommand::Release { response, .. } => {
                    let _ = response.send(Err(error));
                }
            }
        }

        IOHIDManagerUnscheduleFromRunLoop(manager, run_loop, default_mode);

        let _ = IOHIDManagerClose(manager, IOHID_OPTIONS_TYPE_NONE);

        // All callbacks are finished once the manager has been closed
        // and the run loop is no longer being executed.
        //
        // First clean up every consumed device that we own.
        let context = &mut *context_ptr.cast::<HidContext>();

        for device_state in context.devices.values_mut() {
            if let Some(consumed_device) = device_state.consumed_device.take() {
                close_and_release_device(consumed_device);
            }
        }

        CFRelease(default_mode);
        CFRelease(manager);

        let _ = Box::from_raw(context_ptr as *mut HidContext);
    }

    Ok(())
}

fn process_command(command: HidCommand, context_ptr: *mut c_void) {
    let context = unsafe { &mut *context_ptr.cast::<HidContext>() };

    match command {
        HidCommand::Consume { keyboard, response } => {
            let result = consume_macos_keyboard(context, &keyboard);

            let _ = response.send(result);
        }

        HidCommand::Release { keyboard, response } => {
            let result = release_macos_keyboard(context, &keyboard);

            let _ = response.send(result);
        }
    }
}

// ============================================================
// Device consume/release
// ============================================================

fn consume_macos_keyboard(context: &mut HidContext, keyboard: &Keyboard) -> io::Result<()> {
    let device_id = find_device_id(context, keyboard)?;

    let device_state = context.devices.get_mut(&device_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Keyboard is not currently connected",
        )
    })?;

    if device_state.consumed_device.is_some() {
        return Ok(());
    }

    unsafe {
        let service = IOHIDDeviceGetService(device_state.device);

        if service == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Failed to obtain macOS HID service for keyboard",
            ));
        }

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

        device_state.consumed_device = Some(seized_device);
    }

    Ok(())
}

fn release_macos_keyboard(context: &mut HidContext, keyboard: &Keyboard) -> io::Result<()> {
    let device_id = find_device_id(context, keyboard)?;

    let device_state = context.devices.get_mut(&device_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Keyboard is not currently connected",
        )
    })?;

    let consumed_device = device_state.consumed_device.take().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Keyboard is not currently consumed",
        )
    })?;

    unsafe {
        let result = IOHIDDeviceClose(consumed_device, IOHID_OPTIONS_TYPE_NONE);

        CFRelease(consumed_device as CFTypeRef);

        if result != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to release macOS keyboard: 0x{:08x}", result as u32),
            ));
        }
    }

    Ok(())
}

fn find_device_id(context: &HidContext, keyboard: &Keyboard) -> io::Result<DeviceId> {
    context
        .devices
        .iter()
        .find_map(|(&device_id, device_state)| {
            if device_state.keyboard.as_ref() == keyboard {
                Some(device_id)
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

// ============================================================
// Enumeration
// ============================================================

fn enumerate_macos_keyboards() -> Result<Vec<Keyboard>, io::Error> {
    unsafe {
        let manager = IOHIDManagerCreate(ptr::null_mut(), 0);

        if manager.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create IOHIDManager",
            ));
        }

        let matching_dict = match create_matching_dictionary() {
            Some(dict) => dict,

            None => {
                CFRelease(manager);

                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create macOS HID matching dictionary",
                ));
            }
        };

        IOHIDManagerSetDeviceMatching(manager, matching_dict);

        CFRelease(matching_dict);

        let device_set = IOHIDManagerCopyDevices(manager);

        CFRelease(manager);

        if device_set.is_null() {
            return Ok(Vec::new());
        }

        let count = CFSetGetCount(device_set);

        if count <= 0 {
            CFRelease(device_set);
            return Ok(Vec::new());
        }

        let mut devices = vec![ptr::null_mut(); count as usize];

        CFSetGetValues(device_set, devices.as_mut_ptr() as *mut *const c_void);

        let keyboards = devices.into_iter().filter_map(map_to_keyboard).collect();

        CFRelease(device_set);

        Ok(keyboards)
    }
}

// ============================================================
// Keyboard mapping
// ============================================================

pub(super) fn map_to_keyboard(device: IOHIDDeviceRef) -> Option<Keyboard> {
    let name = get_string_property(device, "Product");

    let vendor_id = get_int_property(device, "VendorID").map(|value| format!("{value:04x}"));

    let product_id = get_int_property(device, "ProductID").map(|value| format!("{value:04x}"));

    let serial = get_string_property(device, "SerialNumber");

    // macOS does not expose a Linux-style /sys/... path here.
    // LocationID is used as the platform-specific physical location.
    let physical_path = get_int_property(device, "LocationID").map(|value| format!("{value:x}"));

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
