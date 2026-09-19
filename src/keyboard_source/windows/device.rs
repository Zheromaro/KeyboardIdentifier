use super::{key_mapping::*, win32_props};
use crate::keyboard_source::{Keyboard, KeyboardEvent, KeyboardID, PortID};
use interception::{Device, Interception, Stroke};
use keyboard_types::{KeyState, Modifiers};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredKeyboard {
    pub(crate) device: Device,
    pub(crate) keyboard: Keyboard,
}

pub(crate) struct DeviceEnumerator;

impl DeviceEnumerator {
    pub(crate) fn enumerate_keyboards(context: &Interception) -> Vec<DiscoveredKeyboard> {
        let mut keyboards = Vec::new();
        for i in 1..=interception::MAX_KEYBOARD {
            let device = Device::new(i);
            if let Some(keyboard) = Self::keyboard_from_device(context, device) {
                keyboards.push(DiscoveredKeyboard { device, keyboard });
            }
        }
        keyboards
    }

    pub(crate) fn keyboard_from_device(context: &Interception, device: Device) -> Option<Keyboard> {
        let raw_id = context.get_hardware_id(device)?;
        if raw_id.is_empty() {
            return None;
        }

        // Interception uses "\??\" but Windows APIs expect "\\?\"
        let path = raw_id.replacen(r"\??\", r"\\?\", 1);
        let hardware_id = path.split('#').nth(1).unwrap_or_default();
        let (product, serial) = win32_props::hid_strings(&path);

        let mut keyboard = Keyboard {
            keyboard_id: KeyboardID {
                name: product,
                vendor_id: Self::extract_hex(hardware_id, "VID_"),
                product_id: Self::extract_hex(hardware_id, "PID_"),
                serial,
            },
            port_id: PortID {
                physical_path: win32_props::physical_path(&path),
            },
        };

        Some(keyboard)
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
    sender: mpsc::Sender<Result<KeyboardEvent, std::io::Error>>,
    keyboards: Vec<(Device, Arc<Keyboard>)>,
    modifiers: Modifiers,
    key_action_keys: HashSet<(Device, u16)>,
}

impl InterceptionState {
    pub(crate) fn new(
        sender: mpsc::Sender<Result<KeyboardEvent, std::io::Error>>,
        context: &Interception,
    ) -> Self {
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
                    let _ = self
                        .sender
                        .blocking_send(Ok(KeyboardEvent::Plugged(Arc::clone(&k))));
                    k
                } else {
                    return;
                }
            }
        };

        let is_e0 = (state & 2) != 0;
        let is_up = (state & 1) != 0;
        let key_state = if is_up { KeyState::Up } else { KeyState::Down };

        let key_tuple = (device, *code);
        let repeat = if is_up {
            self.key_action_keys.remove(&key_tuple);
            false
        } else {
            !self.key_action_keys.insert(key_tuple)
        };

        let modifier = modifier_for_scancode(*code, is_e0);
        let is_lock_key = is_lock_scancode(*code);

        let event_modifiers = match key_state {
            KeyState::Down => {
                if let Some(modifier) = modifier {
                    if is_lock_key && !repeat {
                        self.modifiers.toggle(modifier);
                    } else if !is_lock_key {
                        self.modifiers.insert(modifier);
                    }
                }
                self.modifiers
            }
            KeyState::Up => {
                if !is_lock_key {
                    if let Some(modifier) = modifier {
                        self.modifiers.remove(modifier);
                    }
                }
                self.modifiers
            }
        };

        let key_event = super::KeyEvent {
            state: key_state,
            key: scancode_to_key(*code, is_e0),
            code: scancode_to_code(*code, is_e0),
            location: scancode_to_location(*code, is_e0),
            modifiers: event_modifiers,
            repeat,
            is_composing: false,
        };

        let _ = self
            .sender
            .blocking_send(Ok(KeyboardEvent::KeyAction(keyboard, key_event)));
    }
}
