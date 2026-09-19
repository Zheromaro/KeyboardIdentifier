use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};

// Replaces `raw_to_code(vkey, is_e0, scancode)`
pub fn scancode_to_code(scancode: u16, is_e0: bool) -> Code {
    match (scancode, is_e0) {
        (0x01, false) => Code::Escape,
        (0x02, false) => Code::Digit1,
        // Map remaining standard PS/2 Make Codes here...
        _ => Code::Unidentified,
    }
}

// Replaces `raw_to_location(vkey, is_e0)`
pub fn scancode_to_location(scancode: u16, is_e0: bool) -> Location {
    match (scancode, is_e0) {
        (0x2A, false) | (0x38, false) => Location::Left, // LShift, LAlt
        (0x36, false) | (0x38, true) => Location::Right, // RShift, RAlt
        // Map numpad scancodes...
        _ => Location::Standard,
    }
}

// Replaces `raw_to_key(vkey)`
pub fn scancode_to_key(scancode: u16, is_e0: bool) -> Key {
    let named = match (scancode, is_e0) {
        (0x01, _) => Some(NamedKey::Escape),
        (0x1C, false) => Some(NamedKey::Enter),
        // Map key constants...
        _ => None,
    };
    named
        .map(Key::Named)
        .unwrap_or(Key::Named(NamedKey::Unidentified))
}

// Replaces `modifier_for_key(vkey, is_e0)`
pub fn modifier_for_scancode(scancode: u16, is_e0: bool) -> Option<Modifiers> {
    match (scancode, is_e0) {
        (0x2A, _) | (0x36, _) => Some(Modifiers::SHIFT),
        (0x1D, false) => Some(Modifiers::CONTROL),
        (0x1D, true) => Some(Modifiers::CONTROL), // RControl
        (0x3A, false) => Some(Modifiers::CAPS_LOCK),
        _ => None,
    }
}

pub fn is_lock_scancode(scancode: u16) -> bool {
    matches!(scancode, 0x3A | 0x45 | 0x46) // CapsLock, NumLock, ScrollLock
}
