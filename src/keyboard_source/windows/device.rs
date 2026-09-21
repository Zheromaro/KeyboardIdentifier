use crate::keyboard_source::{Access, Keyboard, KeyboardEvent, KeyboardID, PortID};
use interception::{Device, Interception, ScanCode, Stroke, is_keyboard};
use keyboard_types::{KeyState as KeyEventState, Modifiers};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::broadcast;

use super::{key_mapping::*, win32_props};

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredKeyboard {
    pub(crate) device: Device,
    pub(crate) keyboard: Keyboard,
}

pub(crate) struct DeviceEnumerator;

impl DeviceEnumerator {
    pub(crate) fn enumerate_keyboards(context: &Interception) -> Vec<DiscoveredKeyboard> {
        let mut keyboards = Vec::new();
        // INTERCEPTION_MAX_KEYBOARD is defined as 10 in the C library
        for i in 1..=10 {
            let device = i as Device;
            if is_keyboard(device) {
                if let Some(keyboard) = Self::keyboard_from_device(context, device) {
                    keyboards.push(DiscoveredKeyboard { device, keyboard });
                }
            }
        }
        keyboards
    }

    pub(crate) fn keyboard_from_device(context: &Interception, device: Device) -> Option<Keyboard> {
        let mut buffer = [0u8; 512];
        // get_hardware_id requires a mutable buffer and returns the length written
        let len = context.get_hardware_id(device, &mut buffer);
        if len == 0 {
            return None;
        }

        let raw_id = String::from_utf8_lossy(&buffer[..len as usize]).to_string();
        if raw_id.is_empty() {
            return None;
        }

        // Interception uses "\??\" but Windows APIs expect "\\?\"
        let path = raw_id.replacen(r"\??\", r"\\?\", 1);
        let hardware_id = path.split('#').nth(1).unwrap_or_default();
        let (product, serial) = win32_props::hid_strings(&path);

        Some(Keyboard {
            keyboard_id: KeyboardID {
                name: product,
                vendor_id: Self::extract_hex(hardware_id, "VID_"),
                product_id: Self::extract_hex(hardware_id, "PID_"),
                serial,
            },
            port_id: PortID {
                physical_path: win32_props::physical_path(&path),
            },
            access: Access::Shared,
        })
    }

    fn extract_hex(val: &str, prefix: &str) -> Option<String> {
        let upper_val = val.to_ascii_uppercase();
        let start = upper_val.find(prefix)?;
        let val = &val[start + prefix.len()..];
        let end = val
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(val.len());

        if end == 0 {
            None
        } else {
            u16::from_str_radix(&val[..end], 16)
                .ok()
                .map(|id| format!("{id:04x}"))
        }
    }
}

pub(crate) struct InterceptionState {
    sender: broadcast::Sender<KeyboardEvent>,
    keyboards: Vec<(Device, Arc<Keyboard>)>,
    modifiers: Modifiers,
    // Use ScanCode directly to avoid u16 casting mismatches
    key_action_keys: HashSet<(Device, ScanCode)>,
}

impl InterceptionState {
    pub(crate) fn new(sender: broadcast::Sender<KeyboardEvent>, context: &Interception) -> Self {
        Self {
            sender,
            keyboards: DeviceEnumerator::enumerate_keyboards(context)
                .into_iter()
                .map(|d| (d.device, Arc::new(d.keyboard)))
                .collect(),
            modifiers: Modifiers::empty(),
            key_action_keys: HashSet::new(),
        }
    }

    pub(crate) fn handle_input(&mut self, context: &Interception, device: Device, stroke: &Stroke) {
        let Stroke::Keyboard { code, state, .. } = stroke else {
            return;
        };

        let keyboard = match self.keyboards.iter().find(|(d, _)| d == &device) {
            Some((_, k)) => Arc::clone(k),
            None => {
                if let Some(k) = DeviceEnumerator::keyboard_from_device(context, device) {
                    let k = Arc::new(k);
                    self.keyboards.push((device, Arc::clone(&k)));
                    let _ = self.sender.send(KeyboardEvent::Plugged(Arc::clone(&k)));
                    k
                } else {
                    return;
                }
            }
        };

        // Use .bits() to safely check bitflags against integers
        let is_e0 = state.bits() & 2 != 0;
        let is_up = state.bits() & 1 != 0;
        let key_state = if is_up {
            KeyEventState::Up
        } else {
            KeyEventState::Down
        };

        let key_tuple = (device, *code);
        let repeat = if is_up {
            self.key_action_keys.remove(&key_tuple);
            false
        } else {
            !self.key_action_keys.insert(key_tuple)
        };

        // Cast ScanCode to u16 to match your key_mapping function signatures
        let code_u16 = *code as u16;
        let modifier = modifier_for_scancode(code_u16, is_e0);
        let is_lock_key = is_lock_scancode(code_u16);

        let event_modifiers = match key_state {
            KeyEventState::Down => {
                if let Some(modifier) = modifier {
                    if is_lock_key && !repeat {
                        self.modifiers.toggle(modifier);
                    } else if !is_lock_key {
                        self.modifiers.insert(modifier);
                    }
                }
                self.modifiers
            }
            KeyEventState::Up => {
                if !is_lock_key {
                    if let Some(modifier) = modifier {
                        self.modifiers.remove(modifier);
                    }
                }
                self.modifiers
            }
        };

        let key_event = keyboard_types::KeyboardEvent {
            state: key_state,
            key: scancode_to_key(code_u16, is_e0),
            code: scancode_to_code(code_u16, is_e0),
            location: scancode_to_location(code_u16, is_e0),
            modifiers: event_modifiers,
            repeat,
            is_composing: false,
        };

        let _ = self
            .sender
            .send(KeyboardEvent::KeyAction(keyboard, key_event));
    }
}
