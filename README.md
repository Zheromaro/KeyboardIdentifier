# keyboard_identifier

A cross-platform Rust library for uniquely identifying physical keyboards, tracking their hardware ports, and listening to device-specific events (key presses, plug, and unplug events). 

Built with `tokio`, this library abstracts away OS-specific input APIs (Raw Input / SetupAPI on Windows, standard inputs on Linux) to provide a unified, asynchronous event-driven interface. Perfect for projects involving custom macro pads, POS systems, or multi-keyboard setups.

## Features

* **Device Differentiation:** Identify keyboards by Vendor ID, Product ID, Serial Number, and Name.
* **Topology Tracking:** Differentiate identical keyboards plugged into different USB ports using `PortID` (physical path).
* **Async Event Callbacks:** Listen to `Pressed`, `Plugged`, and `Unplugged` events non-blocking via Tokio.
* **Cross-Platform:** Native support for Windows and Linux.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
keyboard_identifier = "0.1.0" # replace with your actual version
tokio = { version = "1", features = ["full"] }
```

## Example Usage

This example demonstrates how to list available keyboards, ask the user to press a key to "select" a specific keyboard, and then monitor events specifically for that device.

```rust,no_run
use keyboard_identifier::{KeyboardManager, keyboard_source::*};
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize the manager and list currently connected keyboards
    let mut manager = KeyboardManager::new().await?;
    let keyboards = manager.get_keyboards();
    
    if keyboards.is_empty() {
        println!("No keyboards found.");
        return Ok(());
    }

    println!("Available keyboards:");
    for (i, kb) in keyboards.iter().enumerate() {
        println!(" [{}] ID: {:?} | Port: {:?}", i, kb.keyboard_id.name, kb.port_id.physical_path);
    }

    // 2. Set up a single channel to route all keyboard events to our main loop
    let (tx, mut rx) = mpsc::unbounded_channel();

    let tx_pressed = tx.clone();
    manager.on_pressed(move |kb| { let _ = tx_pressed.send(KeyboardEvent::Pressed(Arc::new(kb.clone()))); });

    let tx_plugged = tx.clone();
    manager.on_plugged(move |kb| { let _ = tx_plugged.send(KeyboardEvent::Plugged(Arc::new(kb.clone()))); });

    let tx_unplugged = tx.clone();
    manager.on_unplugged(move |kb| { let _ = tx_unplugged.send(KeyboardEvent::Unplugged(Arc::new(kb.clone()))); });

    // Start background listening
    manager.listen().await;

    // 3. Main Event Loop (Selection & Monitoring)
    println!("\nPress any key on the keyboard you want to monitor...");
    let mut selected_keyboard = None;

    while let Some(event) = rx.recv().await {
        match event {
            KeyboardEvent::Pressed(kb) => {
                // If we haven't selected a keyboard yet, the first one to press a key wins
                if selected_keyboard.is_none() {
                    selected_keyboard = Some(kb.clone());
                    println!("\n✅ Selected keyboard: {:?}", kb.keyboard_id.name);
                    println!("Now listening for events on the selected keyboard. Press Ctrl+C to quit.");
                } 
                // If a keyboard is already selected, only log presses for that specific keyboard
                else if Some(&kb) == selected_keyboard.as_ref() {
                    println!("⌨️ Key pressed on selected keyboard!");
                }
            }
            KeyboardEvent::Plugged(kb) => {
                println!("🔌 Plugged: {:?} (Port: {:?})", kb.keyboard_id.name, kb.port_id.physical_path);
            }
            KeyboardEvent::Unplugged(kb) => {
                println!("❌ Unplugged: {:?} (Port: {:?})", kb.keyboard_id.name, kb.port_id.physical_path);
            }
        }
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

### `KeyboardEvent`
An enum representing what happened to a keyboard:
* `Plugged(Arc<Keyboard>)`
* `Unplugged(Arc<Keyboard>)`
* `Pressed(Arc<Keyboard>)`

## Supported OS
- **Windows** (`target_os = "windows"`): Uses Raw Input and SetupAPI.
- **Linux** (`target_os = "linux"`): Uses standard input file descriptors.

## License

This project is licensed under [MIT] - see the LICENSE file for details.
