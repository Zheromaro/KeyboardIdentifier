use interception::{KeyState, ScanCode};
use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};

pub(crate) fn interception_to_key_event(
    code: ScanCode,
    state: KeyState,
    modifiers: Modifiers,
    repeat: bool,
) -> keyboard_types::KeyboardEvent {
    keyboard_types::KeyboardEvent {
        state: if state.contains(KeyState::UP) {
            keyboard_types::KeyState::Up
        } else {
            keyboard_types::KeyState::Down
        },
        key: Key::Named(NamedKey::Unidentified),
        code: scan_code_to_code(code, state),
        location: scan_code_to_location(code, state),
        modifiers,
        repeat,
        is_composing: false,
    }
}

pub(crate) fn scan_code_to_code(code: ScanCode, state: KeyState) -> Code {
    let scan_code = code as u16;
    let e0 = state.contains(KeyState::E0);
    let e1 = state.contains(KeyState::E1);

    if e1 && scan_code == 0x45 {
        return Code::Pause;
    }

    if e0 {
        return match scan_code {
            0x1C => Code::NumpadEnter,
            0x1D => Code::ControlRight,
            0x35 => Code::NumpadDivide,
            0x37 => Code::PrintScreen,
            0x38 => Code::AltRight,
            0x47 => Code::Home,
            0x48 => Code::ArrowUp,
            0x49 => Code::PageUp,
            0x4B => Code::ArrowLeft,
            0x4D => Code::ArrowRight,
            0x4F => Code::End,
            0x50 => Code::ArrowDown,
            0x51 => Code::PageDown,
            0x52 => Code::Insert,
            0x53 => Code::Delete,
            0x5D => Code::ContextMenu,
            _ => scan_code_to_base_code(scan_code),
        };
    }
    scan_code_to_base_code(scan_code)
}

fn scan_code_to_base_code(scan_code: u16) -> Code {
    match scan_code {
        0x01 => Code::Escape,
        0x02 => Code::Digit1,
        0x03 => Code::Digit2,
        0x04 => Code::Digit3,
        0x05 => Code::Digit4,
        0x06 => Code::Digit5,
        0x07 => Code::Digit6,
        0x08 => Code::Digit7,
        0x09 => Code::Digit8,
        0x0A => Code::Digit9,
        0x0B => Code::Digit0,
        0x0C => Code::Minus,
        0x0D => Code::Equal,
        0x0E => Code::Backspace,
        0x0F => Code::Tab,
        0x10 => Code::KeyQ,
        0x11 => Code::KeyW,
        0x12 => Code::KeyE,
        0x13 => Code::KeyR,
        0x14 => Code::KeyT,
        0x15 => Code::KeyY,
        0x16 => Code::KeyU,
        0x17 => Code::KeyI,
        0x18 => Code::KeyO,
        0x19 => Code::KeyP,
        0x1A => Code::BracketLeft,
        0x1B => Code::BracketRight,
        0x1C => Code::Enter,
        0x1D => Code::ControlLeft,
        0x1E => Code::KeyA,
        0x1F => Code::KeyS,
        0x20 => Code::KeyD,
        0x21 => Code::KeyF,
        0x22 => Code::KeyG,
        0x23 => Code::KeyH,
        0x24 => Code::KeyJ,
        0x25 => Code::KeyK,
        0x26 => Code::KeyL,
        0x27 => Code::Semicolon,
        0x28 => Code::Quote,
        0x29 => Code::Backquote,
        0x2A => Code::ShiftLeft,
        0x2B => Code::Backslash,
        0x2C => Code::KeyZ,
        0x2D => Code::KeyX,
        0x2E => Code::KeyC,
        0x2F => Code::KeyV,
        0x30 => Code::KeyB,
        0x31 => Code::KeyN,
        0x32 => Code::KeyM,
        0x33 => Code::Comma,
        0x34 => Code::Period,
        0x35 => Code::Slash,
        0x36 => Code::ShiftRight,
        0x37 => Code::NumpadMultiply,
        0x38 => Code::AltLeft,
        0x39 => Code::Space,
        0x3A => Code::CapsLock,
        0x3B => Code::F1,
        0x3C => Code::F2,
        0x3D => Code::F3,
        0x3E => Code::F4,
        0x3F => Code::F5,
        0x40 => Code::F6,
        0x41 => Code::F7,
        0x42 => Code::F8,
        0x43 => Code::F9,
        0x44 => Code::F10,
        0x45 => Code::NumLock,
        0x46 => Code::ScrollLock,
        0x47 => Code::Numpad7,
        0x48 => Code::Numpad8,
        0x49 => Code::Numpad9,
        0x4A => Code::NumpadSubtract,
        0x4B => Code::Numpad4,
        0x4C => Code::Numpad5,
        0x4D => Code::Numpad6,
        0x4E => Code::NumpadAdd,
        0x4F => Code::Numpad1,
        0x50 => Code::Numpad2,
        0x51 => Code::Numpad3,
        0x52 => Code::Numpad0,
        0x53 => Code::NumpadDecimal,
        0x54 => Code::PrintScreen,
        0x56 => Code::IntlBackslash,
        0x57 => Code::F11,
        0x58 => Code::F12,
        0x5A => Code::IntlYen,
        0x5B => Code::IntlRo,
        0x5C => Code::KanaMode,
        0x5D => Code::Unidentified, // FIXED: Base 0x5D is non-standard; E0 5D correctly maps to ContextMenu above
        0x64 => Code::F13,
        0x65 => Code::F14,
        0x66 => Code::F15,
        0x67 => Code::F16,
        0x68 => Code::F17,
        0x69 => Code::F18,
        0x6A => Code::F19,
        0x6B => Code::F20,
        0x6C => Code::F21,
        0x6D => Code::F22,
        0x6E => Code::F23,
        0x6F => Code::Unidentified,
        0x70 => Code::Katakana,
        0x71 => Code::Unidentified,
        0x76 => Code::F24,
        0x77 => Code::Unidentified,
        0x79 => Code::Convert,
        0x7B => Code::NonConvert,
        _ => Code::Unidentified,
    }
}

fn scan_code_to_location(code: ScanCode, state: KeyState) -> Location {
    let scan_code = code as u16;
    let e0 = state.contains(KeyState::E0);

    match scan_code {
        0x2A => Location::Left,
        0x36 => Location::Right,
        0x1D if e0 => Location::Right,
        0x1D => Location::Left,
        0x38 if e0 => Location::Right,
        0x38 => Location::Left,
        0x47..=0x53 if !e0 => Location::Numpad,
        0x1C if e0 => Location::Numpad,
        0x35 if e0 => Location::Numpad,
        0x37 if !e0 => Location::Numpad,
        0x4A if !e0 => Location::Numpad,
        0x4E if !e0 => Location::Numpad,
        _ => Location::Standard,
    }
}

pub(crate) fn modifier_for_key(code: ScanCode, state: KeyState) -> Option<Modifiers> {
    let scan_code = code as u16;
    let e0 = state.contains(KeyState::E0);

    match scan_code {
        0x2A | 0x36 => Some(Modifiers::SHIFT),
        0x1D => Some(Modifiers::CONTROL),
        0x38 => {
            if e0 {
                Some(Modifiers::ALT_GRAPH)
            } else {
                Some(Modifiers::ALT)
            }
        }
        0x5B | 0x5C if e0 => Some(Modifiers::META),
        0x3A => Some(Modifiers::CAPS_LOCK),
        0x45 => Some(Modifiers::NUM_LOCK),
        0x46 => Some(Modifiers::SCROLL_LOCK),
        _ => None,
    }
}
