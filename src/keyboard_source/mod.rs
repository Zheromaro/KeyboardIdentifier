//! Core types and traits for keyboard event sourcing.
//!
//! This module defines the data structures representing keyboards and their
//! events, as well as the [`KeyboardSource`] trait that abstracts OS-specific
//! input handling.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxKeyboardSource as NativeKeyboardSource;

#[cfg(target_os = "windows")]
mod windows;
use tokio::sync::broadcast;
#[cfg(target_os = "windows")]
pub use windows::WindowsKeyboardSource as NativeKeyboardSource;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacosKeyboardSource as NativeKeyboardSource;

pub use keyboard_types::KeyboardEvent as KeyEvent;
use std::{fmt, future::Future, io, sync::Arc};

/// Represents the physical port or connection path of a keyboard.
///
/// This is useful for differentiating between multiple identical keyboards
/// (same Vendor ID, Product ID, and Name) plugged into different USB ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortID {
    /// The physical path of the port (e.g., USB topology path) if available.
    pub physical_path: Option<String>,
}

impl fmt::Display for PortID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PortID {{ physical_path: {} }}",
            self.physical_path.as_deref().unwrap_or("None")
        )
    }
}

/// Represents the hardware identification details of a keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardID {
    /// The human-readable name of the keyboard if available.
    pub name: Option<String>,
    /// The Vendor ID (VID) of the keyboard if available.
    pub vendor_id: Option<String>,
    /// The Product ID (PID) of the keyboard if available.
    pub product_id: Option<String>,
    /// The serial number of the keyboard, if available.
    pub serial: Option<String>,
}

impl fmt::Display for KeyboardID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KeyboardID {{ name: {}, vendor_id: {}, product_id: {}, serial: {} }}",
            self.name.as_deref().unwrap_or("None"),
            self.vendor_id.as_deref().unwrap_or("None"),
            self.product_id.as_deref().unwrap_or("None"),
            self.serial.as_deref().unwrap_or("None"),
        )
    }
}

/// Defines the level of access requested for an input device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Standard shared access alongside other applications.
    Shared,
    /// Exclusive access, preventing other applications from receiving device inputs.
    Exclusive,
}

/// Represents a connected keyboard device, combining its hardware identity
/// and physical connection port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    /// The hardware identification details of the keyboard.
    pub keyboard_id: KeyboardID,
    /// The physical port information of the keyboard.
    pub port_id: PortID,
    /// The level of access requested for an input device.
    pub access: Access,
}

impl fmt::Display for Keyboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Keyboard {{ keyboard_id: {}, port_id: {} }}",
            self.keyboard_id, self.port_id
        )
    }
}

/// Represents an event related to a keyboard device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardEvent {
    /// A keyboard was plugged into the system.
    Plugged(Arc<Keyboard>),
    /// A keyboard was unplugged from the system.
    Unplugged(Arc<Keyboard>),
    /// A key action (press or release) occurred on the keyboard.
    /// KeyEvent for rich event data.
    KeyAction(Arc<Keyboard>, KeyEvent),
}

impl fmt::Display for KeyboardEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyboardEvent::Plugged(kb) => write!(f, "Plugged({})", kb),
            KeyboardEvent::Unplugged(kb) => write!(f, "Unplugged({})", kb),
            KeyboardEvent::KeyAction(kb, event) => {
                write!(
                    f,
                    "KeyAction({}, state={:?}, key={:?}, code={:?})",
                    kb, event.state, event.key, event.code
                )
            }
        }
    }
}

/// A trait for abstracting OS-specific keyboard event sources.
///
/// Implementors of this trait handle the low-level details of enumerating
/// keyboards and receiving input events for a specific operating system.
pub trait KeyboardSource: Send + Sync + Sized {
    /// Creates a new instance of the keyboard source.
    ///
    /// This is an asynchronous operation as it may require initializing
    /// OS-specific handles, threads, or device listeners.
    fn new() -> impl Future<Output = io::Result<Self>> + Send;

    /// Returns a list of currently connected keyboards.
    fn enumerate_keyboards(&self) -> Vec<Keyboard>;

    /// Asynchronously waits for and returns the next keyboard event.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the underlying OS event source fails,
    /// encounters a permissions issue, or is disconnected.
    fn subscribe(&self) -> broadcast::Receiver<KeyboardEvent>;

    /// Consumes (grabs) a specific keyboard device.
    ///
    /// Once consumed, the keyboard's input is suppressed and will not reach
    /// other applications or the operating system.
    fn consume(&self, keyboard: &Keyboard) -> impl Future<Output = io::Result<()>> + Send;

    /// Releases a previously consumed keyboard device.
    ///
    /// Once released, the keyboard's input will pass through normally to
    /// other applications and the operating system.
    fn release(&self, keyboard: &Keyboard) -> impl Future<Output = io::Result<()>> + Send;
}
