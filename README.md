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
use keyboard_identifier::{
    KeyboardEvent, KeyboardManager,
    keyboard_types::{
        Code::{KeyC, KeyR},
        KeyState,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    println!("Hello from windows");

    #[cfg(target_os = "macos")]
    println!("Hello from macos");

    #[cfg(target_os = "linux")]
    println!("Hello from linux");

    // Create the manager and discover the keyboards currently connected.
    let manager = KeyboardManager::new().await.unwrap();
    let keyboards = manager.get_keyboards();

    if keyboards.is_empty() {
        println!("No keyboards found.");
        return Ok(());
    }

    println!("Available keyboards:");
    for (i, kb) in keyboards.iter().enumerate() {
        println!("{}.\n     {},\n     {}", i, kb.keyboard_id, kb.port_id);
    }

    println!("\n=======================================================");
    println!(" - Press Alt+C on the keyboard you want to consume");
    println!(" - Press Alt+R on the keyboard you want to release");
    println!(" - Press Ctrl+C to exit");
    println!("=======================================================\n");

    // Start the background listener.
    manager.listen().await;

    // Register callbacks for applications that prefer a simple event-driven API.
    manager.on_plugged(|kb| println!("Plugged: \n     {},\n     {}", kb.keyboard_id, kb.port_id));

    manager
        .on_unplugged(|kb| println!("Unplugged: \n     {},\n     {}", kb.keyboard_id, kb.port_id));

    manager.on_key_action(|kb, event| {
        let name = kb.keyboard_id.name.as_deref().unwrap_or("Unknown Keyboard");
        println!(
            "Pressed: {} | event: {:?}, {:?}",
            name, event.state, event.code
        )
    });

    // Alternatively subscribe to keyboard events,
    // handle the event stream directly when more control is needed (consume/release).
    let mut events = manager.subscribe();
    while let Ok(event) = events.recv().await {
        match event {
            KeyboardEvent::KeyAction(kb, ke) => {
                let name = kb.keyboard_id.name.as_deref().unwrap_or("Unknown Keyboard");

                // Consume or release the keyboard using a simple Alt+key shortcut.
                if ke.state == KeyState::Down && !ke.repeat && ke.modifiers.alt() {
                    if ke.code == KeyC {
                        if let Err(e) = manager.consume(&kb).await {
                            eprintln!("Failed to consume keyboard: {}", e);
                        } else {
                            println!("Consumed keyboard: {}", name);
                        }
                    }

                    if ke.code == KeyR {
                        if let Err(e) = manager.release(&kb).await {
                            eprintln!("Failed to release keyboard: {}", e);
                        } else {
                            println!("Released keyboard: {}", name);
                        }
                    }
                }
            }
            KeyboardEvent::Plugged(_) => {}
            KeyboardEvent::Unplugged(_) => {}
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

### `KeyEvent`
A rename to KeyboardEvent struct from keyboard_types crate, found on : https://crates.io/crates/keyboard-types

```rust,no_run
use keyboard_types::{Code, Key, KeyState, Location, Modifiers};

pub struct KeyEvent {
    pub state: KeyState,
    pub key: Key,
    pub code: Code,
    pub location: Location,
    pub modifiers: Modifiers,
    /* … */
}
```

## Supported OS
- **Windows** (`target_os = "windows"`): Uses Raw Input and SetupAPI.
- **Linux** (`target_os = "linux"`): Uses standard input file descriptors.
- **macOS** (`target_os = "macos"`): Native implementation using FFI and C callbacks[cite: 2].
## License

This project is licensed under \[MIT\].
