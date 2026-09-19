#![doc = include_str!("../README.md")]

mod keyboard_manager;
pub mod keyboard_source;

// primary types
pub use keyboard_manager::KeyboardManager;
pub use keyboard_source::{Access, KeyEvent, Keyboard, KeyboardEvent, KeyboardID, PortID};

// external types
pub use keyboard_types;
