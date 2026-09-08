#![doc = include_str!("../README.md")]

mod keyboard_manager;
pub mod keyboard_source;
mod registry;
pub use keyboard_manager::KeyboardManager;
