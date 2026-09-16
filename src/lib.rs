#![doc = include_str!("../README.md")]

mod keyboard_manager;
pub mod keyboard_source;
mod registry;

// primary types
pub use keyboard_manager::KeyboardManager;
pub use keyboard_source::{KeyEvent, Keyboard, KeyboardID, PortID};

// external types
pub use keyboard_types;
