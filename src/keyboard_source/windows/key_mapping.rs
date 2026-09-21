use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};

pub fn scancode_to_code(scancode: u16, is_e0: bool) -> Code {
    match (scancode, is_e0) {
        (0x01, false) => Code::Escape,
        (0x02, false) => Code::Digit1,
        (0x03, false) => Code::Digit2,
        (0x04, false) => Code::Digit3,
        (0x05, false) => Code::Digit4,
        (0x06, false) => Code::Digit5,
        (0x07, false) => Code::Digit6,
        (0x08, false) => Code::Digit7,
        (0x09, false) => Code::Digit8,
        (0x0A, false) => Code::Digit9,
        (0x0B, false) => Code::Digit0,
        (0x0C, false) => Code::Minus,
        (0x0D, false) => Code::Equal,
        (0x0E, false) => Code::Backspace,
        (0x0F, false) => Code::Tab,

        (0x10, false) => Code::KeyQ,
        (0x11, false) => Code::KeyW,
        (0x12, false) => Code::KeyE,
        (0x13, false) => Code::KeyR,
        (0x14, false) => Code::KeyT,
        (0x15, false) => Code::KeyY,
        (0x16, false) => Code::KeyU,
        (0x17, false) => Code::KeyI,
        (0x18, false) => Code::KeyO,
        (0x19, false) => Code::KeyP,
        (0x1A, false) => Code::BracketLeft,
        (0x1B, false) => Code::BracketRight,
        (0x1C, false) => Code::Enter,
        (0x1C, true) => Code::NumpadEnter,
        (0x1D, false) => Code::ControlLeft,
        (0x1D, true) => Code::ControlRight,

        (0x1E, false) => Code::KeyA,
        (0x1F, false) => Code::KeyS,
        (0x20, false) => Code::KeyD,
        (0x21, false) => Code::KeyF,
        (0x22, false) => Code::KeyG,
        (0x23, false) => Code::KeyH,
        (0x24, false) => Code::KeyJ,
        (0x25, false) => Code::KeyK,
        (0x26, false) => Code::KeyL,
        (0x27, false) => Code::Semicolon,
        (0x28, false) => Code::Quote,
        (0x29, false) => Code::Backquote,
        (0x2A, false) => Code::ShiftLeft,
        (0x2B, false) => Code::Backslash,

        (0x2C, false) => Code::KeyZ,
        (0x2D, false) => Code::KeyX,
        (0x2E, false) => Code::KeyC,
        (0x2F, false) => Code::KeyV,
        (0x30, false) => Code::KeyB,
        (0x31, false) => Code::KeyN,
        (0x32, false) => Code::KeyM,
        (0x33, false) => Code::Comma,
        (0x34, false) => Code::Period,
        (0x35, false) => Code::Slash,
        (0x35, true) => Code::NumpadDivide,
        (0x36, false) => Code::ShiftRight,
        (0x37, false) => Code::NumpadMultiply,
        (0x37, true) => Code::PrintScreen,
        (0x38, false) => Code::AltLeft,
        (0x38, true) => Code::AltRight,
        (0x39, false) => Code::Space,
        (0x3A, false) => Code::CapsLock,

        (0x3B, false) => Code::F1,
        (0x3C, false) => Code::F2,
        (0x3D, false) => Code::F3,
        (0x3E, false) => Code::F4,
        (0x3F, false) => Code::F5,
        (0x40, false) => Code::F6,
        (0x41, false) => Code::F7,
        (0x42, false) => Code::F8,
        (0x43, false) => Code::F9,
        (0x44, false) => Code::F10,

        (0x45, false) => Code::NumLock,
        (0x45, true) => Code::Pause,
        (0x46, false) => Code::ScrollLock,
        (0x47, false) => Code::Numpad7,
        (0x47, true) => Code::Home,
        (0x48, false) => Code::Numpad8,
        (0x48, true) => Code::ArrowUp,
        (0x49, false) => Code::Numpad9,
        (0x49, true) => Code::PageUp,
        (0x4A, false) => Code::NumpadSubtract,
        (0x4B, false) => Code::Numpad4,
        (0x4B, true) => Code::ArrowLeft,
        (0x4C, false) => Code::Numpad5,
        (0x4D, false) => Code::Numpad6,
        (0x4D, true) => Code::ArrowRight,
        (0x4E, false) => Code::NumpadAdd,
        (0x4F, false) => Code::Numpad1,
        (0x4F, true) => Code::End,
        (0x50, false) => Code::Numpad2,
        (0x50, true) => Code::ArrowDown,
        (0x51, false) => Code::Numpad3,
        (0x51, true) => Code::PageDown,
        (0x52, false) => Code::Numpad0,
        (0x52, true) => Code::Insert,
        (0x53, false) => Code::NumpadDecimal,
        (0x53, true) => Code::Delete,

        (0x57, false) => Code::F11,
        (0x58, false) => Code::F12,
        (0x5B, true) => Code::MetaLeft,
        (0x5C, true) => Code::MetaRight,
        (0x5D, true) => Code::ContextMenu,

        (0x64, false) => Code::F13,
        (0x65, false) => Code::F14,
        (0x66, false) => Code::F15,
        (0x67, false) => Code::F16,
        (0x68, false) => Code::F17,
        (0x69, false) => Code::F18,
        (0x6A, false) => Code::F19,
        (0x6B, false) => Code::F20,
        (0x6C, false) => Code::F21,
        (0x6D, false) => Code::F22,
        (0x6E, false) => Code::F23,
        (0x76, false) => Code::F24,

        _ => Code::Unidentified,
    }
}

pub fn scancode_to_location(scancode: u16, is_e0: bool) -> Location {
    match (scancode, is_e0) {
        (0x2A, false) | (0x1D, false) | (0x38, false) | (0x5B, true) => Location::Left,
        (0x36, false) | (0x1D, true) | (0x38, true) | (0x5C, true) => Location::Right,
        (0x37, false) | (0x4A, false) | (0x4E, false) | (0x53, false) | (0x47..=0x52, false) => {
            Location::Numpad
        }
        (0x35, true) | (0x1C, true) => Location::Numpad,
        _ => Location::Standard,
    }
}

pub fn scancode_to_key(scancode: u16, is_e0: bool) -> Key {
    let named_key = match (scancode, is_e0) {
        (0x01, false) => Some(NamedKey::Escape),
        (0x1C, _) => Some(NamedKey::Enter),
        (0x0F, false) => Some(NamedKey::Tab),
        (0x0E, false) => Some(NamedKey::Backspace),
        (0x53, true) => Some(NamedKey::Delete),
        (0x52, true) => Some(NamedKey::Insert),

        (0x47, true) => Some(NamedKey::Home),
        (0x4F, true) => Some(NamedKey::End),
        (0x49, true) => Some(NamedKey::PageUp),
        (0x51, true) => Some(NamedKey::PageDown),

        (0x48, true) => Some(NamedKey::ArrowUp),
        (0x50, true) => Some(NamedKey::ArrowDown),
        (0x4B, true) => Some(NamedKey::ArrowLeft),
        (0x4D, true) => Some(NamedKey::ArrowRight),

        (0x3A, false) => Some(NamedKey::CapsLock),
        (0x45, false) => Some(NamedKey::NumLock),
        (0x46, false) => Some(NamedKey::ScrollLock),

        (0x2A, false) | (0x36, false) => Some(NamedKey::Shift),
        (0x1D, _) => Some(NamedKey::Control),
        (0x38, _) => Some(NamedKey::Alt),
        (0x5B, true) | (0x5C, true) => Some(NamedKey::Meta),

        (0x37, true) => Some(NamedKey::PrintScreen),
        (0x45, true) => Some(NamedKey::Pause),
        (0x5D, true) => Some(NamedKey::ContextMenu),

        (0x3B, false) => Some(NamedKey::F1),
        (0x3C, false) => Some(NamedKey::F2),
        (0x3D, false) => Some(NamedKey::F3),
        (0x3E, false) => Some(NamedKey::F4),
        (0x3F, false) => Some(NamedKey::F5),
        (0x40, false) => Some(NamedKey::F6),
        (0x41, false) => Some(NamedKey::F7),
        (0x42, false) => Some(NamedKey::F8),
        (0x43, false) => Some(NamedKey::F9),
        (0x44, false) => Some(NamedKey::F10),
        (0x57, false) => Some(NamedKey::F11),
        (0x58, false) => Some(NamedKey::F12),
        (0x64, false) => Some(NamedKey::F13),
        (0x65, false) => Some(NamedKey::F14),
        (0x66, false) => Some(NamedKey::F15),
        (0x67, false) => Some(NamedKey::F16),
        (0x68, false) => Some(NamedKey::F17),
        (0x69, false) => Some(NamedKey::F18),
        (0x6A, false) => Some(NamedKey::F19),
        (0x6B, false) => Some(NamedKey::F20),
        (0x6C, false) => Some(NamedKey::F21),
        (0x6D, false) => Some(NamedKey::F22),
        (0x6E, false) => Some(NamedKey::F23),
        (0x76, false) => Some(NamedKey::F24),

        _ => None,
    };

    match named_key {
        Some(key) => Key::Named(key),
        None => Key::Named(NamedKey::Unidentified),
    }
}

pub fn modifier_for_scancode(scancode: u16, is_e0: bool) -> Option<Modifiers> {
    match (scancode, is_e0) {
        (0x2A, false) | (0x36, false) => Some(Modifiers::SHIFT),
        (0x1D, false) => Some(Modifiers::CONTROL),
        (0x1D, true) => Some(Modifiers::CONTROL),
        (0x38, false) => Some(Modifiers::ALT),
        (0x38, true) => Some(Modifiers::ALT_GRAPH),
        (0x5B, true) | (0x5C, true) => Some(Modifiers::META),
        (0x3A, false) => Some(Modifiers::CAPS_LOCK),
        (0x45, false) => Some(Modifiers::NUM_LOCK),
        (0x46, false) => Some(Modifiers::SCROLL_LOCK),
        _ => None,
    }
}

pub fn is_lock_scancode(scancode: u16) -> bool {
    matches!(scancode, 0x3A | 0x45 | 0x46)
}
