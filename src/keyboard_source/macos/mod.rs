mod c_callbacks;
mod ffi_declarations;
mod key_mapping;

use super::{Access, Keyboard, KeyboardEvent, KeyboardID, KeyboardSource, PortID};
use c_callbacks::*;
use ffi_declarations::*;
use keyboard_types::Modifiers;
use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    io, ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::error;

const USAGE_PAGE_GENERIC_DESKTOP: i32 = 1;
const USAGE_KEYBOARD: i32 = 6;

/// Context state passed to the C callbacks and owned by the HID thread.
struct HidContext {
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    keyboards: HashMap<isize, Arc<Keyboard>>,
    devices: HashMap<isize, IOHIDDeviceRef>,
    consumed: HashSet<isize>,
    modifiers: HashMap<isize, Modifiers>,
}

/// A stable description of a keyboard used by consume/release commands.
#[derive(Debug)]
struct KeyboardTarget {
    name: Option<String>,
    vendor_id: Option<String>,
    product_id: Option<String>,
    serial: Option<String>,
    physical_path: Option<String>,
}

impl From<&Keyboard> for KeyboardTarget {
    fn from(keyboard: &Keyboard) -> Self {
        Self {
            name: keyboard.keyboard_id.name.clone(),
            vendor_id: keyboard.keyboard_id.vendor_id.clone(),
            product_id: keyboard.keyboard_id.product_id.clone(),
            serial: keyboard.keyboard_id.serial.clone(),
            physical_path: keyboard.port_id.physical_path.clone(),
        }
    }
}

enum HidCommand {
    Consume {
        target: KeyboardTarget,
        response: oneshot::Sender<io::Result<()>>,
    },
    Release {
        target: KeyboardTarget,
        response: oneshot::Sender<io::Result<()>>,
    },
}

pub struct MacosKeyboardSource {
    shutdown: broadcast::Sender<()>,
    broadcaster: broadcast::Sender<KeyboardEvent>,
    commands: mpsc::Sender<HidCommand>,
}

impl KeyboardSource for MacosKeyboardSource {
    async fn new() -> io::Result<Self> {
        let (sender, mut receiver) = mpsc::channel(128);
        let (commands, command_receiver) = mpsc::channel(64);
        let (shutdown, _) = broadcast::channel(1);
        let shutdown_rx = shutdown.subscribe();
        let (init_tx, init_rx) = oneshot::channel();
        let (broadcaster, _) = broadcast::channel(128);

        let bcast_clone = broadcaster.clone();
        std::thread::spawn(move || {
            if let Err(error) = macos_hid_loop(sender, command_receiver, shutdown_rx, init_tx) {
                error!("macOS HID loop terminated unexpectedly: {:?}", error);
            }
        });

        match init_rx.await {
            Ok(Ok(())) => {
                tokio::spawn(async move {
                    while let Some(event) = receiver.recv().await {
                        if let Ok(evt) = event {
                            let _ = bcast_clone.send(evt);
                        }
                    }
                });

                Ok(Self {
                    shutdown,
                    broadcaster,
                    commands,
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::Other,
                "macOS initialization thread died unexpectedly",
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

    fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.broadcaster.subscribe()
    }

    async fn consume(&self, keyboard: &Keyboard) -> io::Result<()> {
        let (response_tx, response_rx) = oneshot::channel();
        let target = KeyboardTarget::from(keyboard);
        self.commands
            .send(HidCommand::Consume {
                target,
                response: response_tx,
            })
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "macOS HID thread is no longer running",
                )
            })?;
        response_rx.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "macOS HID thread stopped before replying",
            )
        })?
    }

    async fn release(&self, keyboard: &Keyboard) -> io::Result<()> {
        let (response_tx, response_rx) = oneshot::channel();
        let target = KeyboardTarget::from(keyboard);
        self.commands
            .send(HidCommand::Release {
                target,
                response: response_tx,
            })
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "macOS HID thread is no longer running",
                )
            })?;
        response_rx.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "macOS HID thread stopped before replying",
            )
        })?
    }
}

impl Drop for MacosKeyboardSource {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}

// -----------------------------------------------------------------------------
// HID command handling
// -----------------------------------------------------------------------------
fn keyboard_matches_target(keyboard: &Keyboard, target: &KeyboardTarget) -> bool {
    keyboard.keyboard_id.name == target.name
        && keyboard.keyboard_id.vendor_id == target.vendor_id
        && keyboard.keyboard_id.product_id == target.product_id
        && keyboard.keyboard_id.serial == target.serial
        && keyboard.port_id.physical_path == target.physical_path
}

fn process_hid_commands(context: &mut HidContext, commands: &mut mpsc::Receiver<HidCommand>) {
    loop {
        let command = match commands.try_recv() {
            Ok(command) => command,
            Err(mpsc::error::TryRecvError::Empty) => break,
            Err(mpsc::error::TryRecvError::Disconnected) => break,
        };
        handle_hid_command(context, command);
    }
}

fn handle_hid_command(context: &mut HidContext, command: HidCommand) {
    match command {
        HidCommand::Consume { target, response } => {
            let result = consume_device(context, &target);
            let _ = response.send(result);
        }
        HidCommand::Release { target, response } => {
            let result = release_device(context, &target);
            let _ = response.send(result);
        }
    }
}

fn find_device_for_target(
    context: &HidContext,
    target: &KeyboardTarget,
) -> Option<(isize, IOHIDDeviceRef)> {
    context.keyboards.iter().find_map(|(&device_id, keyboard)| {
        if !context.devices.contains_key(&device_id) {
            return None;
        }
        if keyboard_matches_target(keyboard, target) {
            context
                .devices
                .get(&device_id)
                .copied()
                .map(|device| (device_id, device))
        } else {
            None
        }
    })
}

fn consume_device(context: &mut HidContext, target: &KeyboardTarget) -> io::Result<()> {
    let Some((device_id, device)) = find_device_for_target(context, target) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Keyboard is not currently connected",
        ));
    };

    // Calling consume twice is intentionally idempotent.
    if context.consumed.contains(&device_id) {
        return Ok(());
    }

    let result = unsafe { IOHIDDeviceOpen(device, kIOHIDOptionsTypeSeizeDevice) };
    if result != kIOReturnSuccess {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Failed to consume macOS keyboard (IOKit error: 0x{:08x})",
                result as u32
            ),
        ));
    }

    context.consumed.insert(device_id);
    Ok(())
}

fn release_device(context: &mut HidContext, target: &KeyboardTarget) -> io::Result<()> {
    let Some((device_id, device)) = find_device_for_target(context, target) else {
        // The device may already have been unplugged.
        return Ok(());
    };

    // Calling release on a shared device is intentionally idempotent.
    if !context.consumed.contains(&device_id) {
        return Ok(());
    }

    let result = unsafe { IOHIDDeviceClose(device, 0) };
    if result != kIOReturnSuccess {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Failed to release macOS keyboard (IOKit error: 0x{:08x})",
                result as u32
            ),
        ));
    }

    context.consumed.remove(&device_id);
    Ok(())
}

// -----------------------------------------------------------------------------
// Core OS Integrations
// -----------------------------------------------------------------------------
fn macos_hid_loop(
    sender: mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    mut commands: mpsc::Receiver<HidCommand>,
    mut shutdown: broadcast::Receiver<()>,
    init_tx: oneshot::Sender<io::Result<()>>,
) -> io::Result<()> {
    unsafe {
        let manager = IOHIDManagerCreate(ptr::null_mut(), 0);
        if manager.is_null() {
            let _ = init_tx.send(Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create IOHIDManager",
            )));
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
            devices: HashMap::new(),
            consumed: HashSet::new(),
            modifiers: HashMap::new(),
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

        let open_result = IOHIDManagerOpen(manager, 0);
        if open_result != kIOReturnSuccess {
            let _ = init_tx.send(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Failed to open IOHIDManager (IOKit error: 0x{:08x})",
                    open_result as u32
                ),
            )));
            let _ = Box::from_raw(context_ptr as *mut HidContext);
            CFRelease(default_mode);
            CFRelease(manager);
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Failed to open IOHIDManager (IOKit error: 0x{:08x})",
                    open_result as u32
                ),
            ));
        }

        let _ = init_tx.send(Ok(()));

        let should_stop = Arc::new(AtomicBool::new(false));
        let shutdown_flag = should_stop.clone();
        let run_loop_ptr = run_loop as usize;

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create macOS shutdown runtime");
            rt.block_on(async {
                let _ = shutdown.recv().await;
            });
            shutdown_flag.store(true, Ordering::Release);
            CFRunLoopStop(run_loop_ptr as CFRunLoopRef);
        });

        // We intentionally run the CFRunLoop in small increments instead of
        // calling CFRunLoopRun() forever. This gives us a safe place, on the
        // same thread as the HID callbacks, to process consume/release
        // commands without racing device attach/remove callbacks.
        while !should_stop.load(Ordering::Acquire) {
            let context = &mut *(context_ptr as *mut HidContext);
            process_hid_commands(context, &mut commands);
            // 10 ms maximum command latency while the HID system is idle.
            let _ = CFRunLoopRunInMode(default_mode, 0.010, 1);
        }

        let context = &mut *(context_ptr as *mut HidContext);

        // Release every seized device before destroying the HID manager.
        for &device_id in &context.consumed {
            if let Some(&device) = context.devices.get(&device_id) {
                let _ = IOHIDDeviceClose(device, 0);
            }
        }
        context.consumed.clear();

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
        access: Access::Shared,
    })
}
