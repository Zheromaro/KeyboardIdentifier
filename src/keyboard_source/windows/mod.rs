mod device;
mod key_mapping;
mod win32_props;

use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardSource};
use device::{DeviceEnumerator, InterceptionState};
use interception::{Filter, Interception, KeyFilter, KeyState, ScanCode, Stroke};
use std::{
    collections::HashSet,
    env, fs,
    future::Future,
    io,
    process::Command,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use tokio::sync::broadcast;

const INTERCEPTION_DLL: &[u8] = include_bytes!("../../../vendor/interception.dll");
const INSTALLER_EXE: &[u8] = include_bytes!("../../../vendor/install-interception.exe");

/// Wrapper to make Interception Send + Sync.
/// The underlying Interception C library uses internal locking, making this safe.
struct SendableInterception(Interception);
unsafe impl Send for SendableInterception {}
unsafe impl Sync for SendableInterception {}

static INTERCEPTION_OWNER: AtomicBool = AtomicBool::new(false);

pub(crate) struct InterceptionOwner;

impl InterceptionOwner {
    pub(crate) fn acquire() -> io::Result<Self> {
        if INTERCEPTION_OWNER
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Another interception instance is already active",
            ));
        }
        Ok(Self)
    }
}

impl Drop for InterceptionOwner {
    fn drop(&mut self) {
        INTERCEPTION_OWNER.store(false, Ordering::Release);
    }
}

pub struct WindowsKeyboardSource {
    sender: broadcast::Sender<KeyboardEvent>,
    context: Arc<SendableInterception>,
    consumed_devices: Arc<RwLock<HashSet<interception::Device>>>,
    should_stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl KeyboardSource for WindowsKeyboardSource {
    async fn new() -> io::Result<Self> {
        let _owner = InterceptionOwner::acquire()?;

        let context = Interception::new().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "Failed to initialize Interception context. Is the driver installed?",
            )
        })?;

        let context = Arc::new(SendableInterception(context));
        let (sender, _) = broadcast::channel(1024);
        let consumed_devices = Arc::new(RwLock::new(HashSet::new()));
        let should_stop = Arc::new(AtomicBool::new(false));

        let thread_context = Arc::clone(&context);
        let thread_sender = sender.clone();
        let thread_consumed = Arc::clone(&consumed_devices);
        let thread_should_stop = Arc::clone(&should_stop);

        let thread = thread::spawn(move || {
            // Correct filter syntax for the interception crate
            thread_context.0.set_filter(
                interception::is_keyboard,
                Filter::KeyFilter(KeyFilter::all()),
            );
            let mut state = InterceptionState::new(thread_sender, &thread_context.0);

            // Dummy stroke to initialize the receive buffer array
            let dummy_stroke = Stroke::Keyboard {
                code: ScanCode::Esc,
                state: KeyState::empty(),
                information: 0,
            };
            let mut strokes = [dummy_stroke];

            while !thread_should_stop.load(Ordering::Relaxed) {
                let device = thread_context.0.wait();

                if interception::is_keyboard(device) {
                    // receive populates the mutable slice and returns the count of strokes read
                    let count = thread_context.0.receive(device, &mut strokes);

                    if count > 0 {
                        let stroke = strokes[0];

                        // 1. Parse and broadcast the event to our application
                        state.handle_input(&thread_context.0, device, &stroke);

                        // 2. Check if the device is grabbed for exclusive access
                        let is_consumed = thread_consumed.read().unwrap().contains(&device);

                        // 3. If not consumed, inject the keystroke back into the OS
                        if !is_consumed {
                            thread_context.0.send(device, &strokes[..count as usize]);
                        }
                    }
                }
            }
        });

        Ok(Self {
            sender,
            context,
            consumed_devices,
            should_stop,
            thread: Some(thread),
        })
    }

    fn enumerate_keyboards(&self) -> Vec<Keyboard> {
        DeviceEnumerator::enumerate_keyboards(&self.context.0)
            .into_iter()
            .map(|dk| dk.keyboard)
            .collect()
    }

    fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.sender.subscribe()
    }

    fn consume(&self, keyboard: &Keyboard) -> impl Future<Output = io::Result<()>> + Send {
        let context = Arc::clone(&self.context);
        let consumed_devices = Arc::clone(&self.consumed_devices);
        let target_keyboard = keyboard.clone();

        async move {
            let device_opt = DeviceEnumerator::enumerate_keyboards(&context.0)
                .into_iter()
                .find(|dk| {
                    dk.keyboard.keyboard_id == target_keyboard.keyboard_id
                        && dk.keyboard.port_id == target_keyboard.port_id
                })
                .map(|dk| dk.device);

            if let Some(device) = device_opt {
                consumed_devices.write().unwrap().insert(device);
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Target keyboard not found or disconnected",
                ))
            }
        }
    }

    fn release(&self, keyboard: &Keyboard) -> impl Future<Output = io::Result<()>> + Send {
        let context = Arc::clone(&self.context);
        let consumed_devices = Arc::clone(&self.consumed_devices);
        let target_keyboard = keyboard.clone();

        async move {
            let device_opt = DeviceEnumerator::enumerate_keyboards(&context.0)
                .into_iter()
                .find(|dk| {
                    dk.keyboard.keyboard_id == target_keyboard.keyboard_id
                        && dk.keyboard.port_id == target_keyboard.port_id
                })
                .map(|dk| dk.device);

            if let Some(device) = device_opt {
                consumed_devices.write().unwrap().remove(&device);
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Target keyboard not found or disconnected",
                ))
            }
        }
    }
}

impl Drop for WindowsKeyboardSource {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
        self.thread.take();
    }
}
