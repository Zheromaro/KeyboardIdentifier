# Keyboard Identifier

A cross-platform Rust library for uniquely identifying physical keyboards, tracking their hardware ports, and listening to device-specific events (key presses, releases, plug, and unplug events). 

Built with `tokio`, this library abstracts away OS-specific input APIs to provide a unified, asynchronous event-driven interface. Perfect for projects involving custom macro pads, POS systems, or multi-keyboard setups.

## Features

* **Device Differentiation:** Identify keyboards by Vendor ID, Product ID, Serial Number, and Name.
* **Topology Tracking:** Differentiate identical keyboards plugged into different USB ports using `PortID` (physical path).
* **Async Event Callbacks:** Listen to `KeyAction` (press/release), `Plugged`, and `Unplugged` events non-blocking via Tokio.
* **Rich Key Events:** Integrates the external W3C standard using the `keyboard-types` crate to provide detailed key event data (logical key, physical code, modifiers, and repeat state)[cite: 1, 3].
* **Cross-Platform:** Native support for Windows, Linux, and macOS.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
keyboard_identifier = "0.1.0" # replace with your actual version
tokio = { version = "1.53.1", features = ["macros", "rt", "sync", "rt-multi-thread"] }
```

## Example Usage

This example demonstrates how to list available keyboards, ask the user to press a key to "select" a specific keyboard, and then monitor events specifically for that device.

```rust,no_run
use keyboard_identifier::{KeyEvent, Keyboard, KeyboardManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. List available keyboards (informational only)
    let mut manager = KeyboardManager::new().await.unwrap();
    let keyboards = manager.get_keyboards();
    if keyboards.is_empty() {
        println!("No keyboards found.");
        return Ok(());
    }

    println!("Available keyboards:");
    for (i, kb) in keyboards.iter().enumerate() {
        println!("{}.\n     {},\n     {}", i, kb.keyboard_id, kb.port_id);
    }

    // 2. Assigning events
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let tx_key_action = tx.clone();
    manager.on_key_action(move |kb: &Keyboard, ke: &KeyEvent| {
        let name = kb.keyboard_id.name.as_deref().unwrap_or("Unknown Keyboard");
        let _ = tx_key_action.send(format!("KeyAction: {name}\nEvent: {:?}", ke));
    });

    let tx_plugged = tx.clone();
    manager.on_plugged(move |kb: &Keyboard| {
        let _ = tx_plugged.send(format!(
            "Plugged: \n     {},\n     {}",
            kb.keyboard_id, kb.port_id
        ));
    });

    let tx_unplugged = tx.clone();
    manager.on_unplugged(move |kb: &Keyboard| {
        let _ = tx_unplugged.send(format!(
            "Unplugged: \n     {},\n     {}",
            kb.keyboard_id, kb.port_id
        ));
    });

    manager.listen().await;
    println!("Now listening for events. Press Ctrl+C to quit.");

    // 3. Print each event as it arrives
    while let Some(event_type) = rx.recv().await {
        println!("{}", event_type);
    }

    Ok(())
}
```

## Core Types

### `Keyboard`
The primary struct representing a connected device. It contains two identifiers:
* `keyboard_id`: Identifying information about the hardware model (Name, Vendor ID, Product ID, Serial).
* `port_id`: Identifying information about the physical connection (e.g., USB port topology). 

*Note: Two identical keyboards from the same manufacturer will have identical `KeyboardID`s, but different `PortID`s.*

### `KeyEvent`
A rename to KeyboardEvent struct from keyboard_types crate, found on : https://crates.io/crates/keyboard-types

```rust,no_run
pub struct KeyboardEvent {
    pub state: KeyState,
    pub key: Key,
    pub code: Code,
    pub location: Location,
    pub modifiers: Modifiers,
    /* … */
}
```
although key field is not set

## Supported OS
- **Windows** (`target_os = "windows"`): Uses Raw Input and SetupAPI.
- **Linux** (`target_os = "linux"`): Uses standard input file descriptors.
- **macOS** (`target_os = "macos"`): Native implementation using FFI and C callbacks[cite: 2].
## License

This project is licensed under \[MIT\].
