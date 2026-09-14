use evdev::KeyCode;
use keyboard_types::{Code, Key, KeyState, Location, Modifiers, NamedKey};

pub fn evdev_to_code(key: KeyCode) -> Code {
    match key {
        KeyCode::KEY_ESC => Code::Escape,

        // Number row
        KeyCode::KEY_1 => Code::Digit1,
        KeyCode::KEY_2 => Code::Digit2,
        KeyCode::KEY_3 => Code::Digit3,
        KeyCode::KEY_4 => Code::Digit4,
        KeyCode::KEY_5 => Code::Digit5,
        KeyCode::KEY_6 => Code::Digit6,
        KeyCode::KEY_7 => Code::Digit7,
        KeyCode::KEY_8 => Code::Digit8,
        KeyCode::KEY_9 => Code::Digit9,
        KeyCode::KEY_0 => Code::Digit0,
        KeyCode::KEY_MINUS => Code::Minus,
        KeyCode::KEY_EQUAL => Code::Equal,
        KeyCode::KEY_BACKSPACE => Code::Backspace,

        // QWERTY rows
        KeyCode::KEY_TAB => Code::Tab,

        KeyCode::KEY_Q => Code::KeyQ,
        KeyCode::KEY_W => Code::KeyW,
        KeyCode::KEY_E => Code::KeyE,
        KeyCode::KEY_R => Code::KeyR,
        KeyCode::KEY_T => Code::KeyT,
        KeyCode::KEY_Y => Code::KeyY,
        KeyCode::KEY_U => Code::KeyU,
        KeyCode::KEY_I => Code::KeyI,
        KeyCode::KEY_O => Code::KeyO,
        KeyCode::KEY_P => Code::KeyP,

        KeyCode::KEY_LEFTBRACE => Code::BracketLeft,
        KeyCode::KEY_RIGHTBRACE => Code::BracketRight,
        KeyCode::KEY_ENTER => Code::Enter,

        KeyCode::KEY_LEFTCTRL => Code::ControlLeft,

        KeyCode::KEY_A => Code::KeyA,
        KeyCode::KEY_S => Code::KeyS,
        KeyCode::KEY_D => Code::KeyD,
        KeyCode::KEY_F => Code::KeyF,
        KeyCode::KEY_G => Code::KeyG,
        KeyCode::KEY_H => Code::KeyH,
        KeyCode::KEY_J => Code::KeyJ,
        KeyCode::KEY_K => Code::KeyK,
        KeyCode::KEY_L => Code::KeyL,

        KeyCode::KEY_SEMICOLON => Code::Semicolon,
        KeyCode::KEY_APOSTROPHE => Code::Quote,
        KeyCode::KEY_GRAVE => Code::Backquote,

        KeyCode::KEY_LEFTSHIFT => Code::ShiftLeft,
        KeyCode::KEY_BACKSLASH => Code::Backslash,

        KeyCode::KEY_Z => Code::KeyZ,
        KeyCode::KEY_X => Code::KeyX,
        KeyCode::KEY_C => Code::KeyC,
        KeyCode::KEY_V => Code::KeyV,
        KeyCode::KEY_B => Code::KeyB,
        KeyCode::KEY_N => Code::KeyN,
        KeyCode::KEY_M => Code::KeyM,

        KeyCode::KEY_COMMA => Code::Comma,
        KeyCode::KEY_DOT => Code::Period,
        KeyCode::KEY_SLASH => Code::Slash,

        KeyCode::KEY_RIGHTSHIFT => Code::ShiftRight,

        KeyCode::KEY_LEFTALT => Code::AltLeft,
        KeyCode::KEY_SPACE => Code::Space,
        KeyCode::KEY_CAPSLOCK => Code::CapsLock,

        // Function keys
        KeyCode::KEY_F1 => Code::F1,
        KeyCode::KEY_F2 => Code::F2,
        KeyCode::KEY_F3 => Code::F3,
        KeyCode::KEY_F4 => Code::F4,
        KeyCode::KEY_F5 => Code::F5,
        KeyCode::KEY_F6 => Code::F6,
        KeyCode::KEY_F7 => Code::F7,
        KeyCode::KEY_F8 => Code::F8,
        KeyCode::KEY_F9 => Code::F9,
        KeyCode::KEY_F10 => Code::F10,
        KeyCode::KEY_F11 => Code::F11,
        KeyCode::KEY_F12 => Code::F12,

        // Lock keys
        KeyCode::KEY_NUMLOCK => Code::NumLock,
        KeyCode::KEY_SCROLLLOCK => Code::ScrollLock,

        // Numeric keypad
        KeyCode::KEY_KP7 => Code::Numpad7,
        KeyCode::KEY_KP8 => Code::Numpad8,
        KeyCode::KEY_KP9 => Code::Numpad9,
        KeyCode::KEY_KPMINUS => Code::NumpadSubtract,

        KeyCode::KEY_KP4 => Code::Numpad4,
        KeyCode::KEY_KP5 => Code::Numpad5,
        KeyCode::KEY_KP6 => Code::Numpad6,
        KeyCode::KEY_KPPLUS => Code::NumpadAdd,

        KeyCode::KEY_KP1 => Code::Numpad1,
        KeyCode::KEY_KP2 => Code::Numpad2,
        KeyCode::KEY_KP3 => Code::Numpad3,
        KeyCode::KEY_KP0 => Code::Numpad0,
        KeyCode::KEY_KPDOT => Code::NumpadDecimal,

        KeyCode::KEY_KPASTERISK => Code::NumpadMultiply,
        KeyCode::KEY_KPSLASH => Code::NumpadDivide,
        KeyCode::KEY_KPENTER => Code::NumpadEnter,

        // Right-side modifiers
        KeyCode::KEY_RIGHTCTRL => Code::ControlRight,
        KeyCode::KEY_RIGHTALT => Code::AltRight,

        // Navigation
        KeyCode::KEY_HOME => Code::Home,
        KeyCode::KEY_UP => Code::ArrowUp,
        KeyCode::KEY_PAGEUP => Code::PageUp,

        KeyCode::KEY_LEFT => Code::ArrowLeft,
        KeyCode::KEY_RIGHT => Code::ArrowRight,

        KeyCode::KEY_END => Code::End,
        KeyCode::KEY_DOWN => Code::ArrowDown,
        KeyCode::KEY_PAGEDOWN => Code::PageDown,

        KeyCode::KEY_INSERT => Code::Insert,
        KeyCode::KEY_DELETE => Code::Delete,

        // Meta / Super
        KeyCode::KEY_LEFTMETA => Code::MetaLeft,
        KeyCode::KEY_RIGHTMETA => Code::MetaRight,
        KeyCode::KEY_COMPOSE => Code::ContextMenu,

        // Miscellaneous
        KeyCode::KEY_PRINT => Code::PrintScreen,
        KeyCode::KEY_PAUSE => Code::Pause,
        KeyCode::KEY_MENU => Code::ContextMenu,

        // Additional function keys
        KeyCode::KEY_F13 => Code::F13,
        KeyCode::KEY_F14 => Code::F14,
        KeyCode::KEY_F15 => Code::F15,
        KeyCode::KEY_F16 => Code::F16,
        KeyCode::KEY_F17 => Code::F17,
        KeyCode::KEY_F18 => Code::F18,
        KeyCode::KEY_F19 => Code::F19,
        KeyCode::KEY_F20 => Code::F20,
        KeyCode::KEY_F21 => Code::F21,
        KeyCode::KEY_F22 => Code::F22,
        KeyCode::KEY_F23 => Code::F23,
        KeyCode::KEY_F24 => Code::F24,

        _ => Code::Unidentified,
    }
}

pub fn evdev_to_key_state(value: i32) -> Option<(KeyState, bool)> {
    match value {
        0 => Some((KeyState::Up, false)),
        1 => Some((KeyState::Down, false)),
        2 => Some((KeyState::Down, true)),
        _ => None,
    }
}

pub fn evdev_to_location(key: KeyCode) -> Location {
    match key {
        KeyCode::KEY_LEFTCTRL
        | KeyCode::KEY_LEFTSHIFT
        | KeyCode::KEY_LEFTALT
        | KeyCode::KEY_LEFTMETA => Location::Left,

        KeyCode::KEY_RIGHTCTRL
        | KeyCode::KEY_RIGHTSHIFT
        | KeyCode::KEY_RIGHTALT
        | KeyCode::KEY_RIGHTMETA => Location::Right,

        KeyCode::KEY_KP0
        | KeyCode::KEY_KP1
        | KeyCode::KEY_KP2
        | KeyCode::KEY_KP3
        | KeyCode::KEY_KP4
        | KeyCode::KEY_KP5
        | KeyCode::KEY_KP6
        | KeyCode::KEY_KP7
        | KeyCode::KEY_KP8
        | KeyCode::KEY_KP9
        | KeyCode::KEY_KPDOT
        | KeyCode::KEY_KPENTER
        | KeyCode::KEY_KPPLUS
        | KeyCode::KEY_KPMINUS
        | KeyCode::KEY_KPASTERISK
        | KeyCode::KEY_KPSLASH => Location::Numpad,

        _ => Location::Standard,
    }
}

pub fn evdev_to_key(key: KeyCode) -> Key {
    let named_key = match key {
        KeyCode::KEY_ESC => Some(NamedKey::Escape),
        KeyCode::KEY_ENTER | KeyCode::KEY_KPENTER => Some(NamedKey::Enter),
        KeyCode::KEY_TAB => Some(NamedKey::Tab),
        KeyCode::KEY_BACKSPACE => Some(NamedKey::Backspace),
        KeyCode::KEY_DELETE => Some(NamedKey::Delete),
        KeyCode::KEY_INSERT => Some(NamedKey::Insert),

        KeyCode::KEY_HOME => Some(NamedKey::Home),
        KeyCode::KEY_END => Some(NamedKey::End),
        KeyCode::KEY_PAGEUP => Some(NamedKey::PageUp),
        KeyCode::KEY_PAGEDOWN => Some(NamedKey::PageDown),

        KeyCode::KEY_UP => Some(NamedKey::ArrowUp),
        KeyCode::KEY_DOWN => Some(NamedKey::ArrowDown),
        KeyCode::KEY_LEFT => Some(NamedKey::ArrowLeft),
        KeyCode::KEY_RIGHT => Some(NamedKey::ArrowRight),

        KeyCode::KEY_CAPSLOCK => Some(NamedKey::CapsLock),
        KeyCode::KEY_NUMLOCK => Some(NamedKey::NumLock),
        KeyCode::KEY_SCROLLLOCK => Some(NamedKey::ScrollLock),

        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => Some(NamedKey::Shift),
        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => Some(NamedKey::Control),
        KeyCode::KEY_LEFTALT | KeyCode::KEY_RIGHTALT => Some(NamedKey::Alt),
        KeyCode::KEY_LEFTMETA | KeyCode::KEY_RIGHTMETA => Some(NamedKey::Meta),

        KeyCode::KEY_PRINT => Some(NamedKey::PrintScreen),
        KeyCode::KEY_PAUSE => Some(NamedKey::Pause),
        KeyCode::KEY_MENU => Some(NamedKey::ContextMenu),

        KeyCode::KEY_F1 => Some(NamedKey::F1),
        KeyCode::KEY_F2 => Some(NamedKey::F2),
        KeyCode::KEY_F3 => Some(NamedKey::F3),
        KeyCode::KEY_F4 => Some(NamedKey::F4),
        KeyCode::KEY_F5 => Some(NamedKey::F5),
        KeyCode::KEY_F6 => Some(NamedKey::F6),
        KeyCode::KEY_F7 => Some(NamedKey::F7),
        KeyCode::KEY_F8 => Some(NamedKey::F8),
        KeyCode::KEY_F9 => Some(NamedKey::F9),
        KeyCode::KEY_F10 => Some(NamedKey::F10),
        KeyCode::KEY_F11 => Some(NamedKey::F11),
        KeyCode::KEY_F12 => Some(NamedKey::F12),

        KeyCode::KEY_F13 => Some(NamedKey::F13),
        KeyCode::KEY_F14 => Some(NamedKey::F14),
        KeyCode::KEY_F15 => Some(NamedKey::F15),
        KeyCode::KEY_F16 => Some(NamedKey::F16),
        KeyCode::KEY_F17 => Some(NamedKey::F17),
        KeyCode::KEY_F18 => Some(NamedKey::F18),
        KeyCode::KEY_F19 => Some(NamedKey::F19),
        KeyCode::KEY_F20 => Some(NamedKey::F20),
        KeyCode::KEY_F21 => Some(NamedKey::F21),
        KeyCode::KEY_F22 => Some(NamedKey::F22),
        KeyCode::KEY_F23 => Some(NamedKey::F23),
        KeyCode::KEY_F24 => Some(NamedKey::F24),

        _ => None,
    };

    match named_key {
        Some(key) => Key::Named(key),
        None => Key::Named(NamedKey::Unidentified),
    }
}

pub fn modifier_for_key(key: KeyCode) -> Option<Modifiers> {
    match key {
        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => Some(Modifiers::SHIFT),

        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => Some(Modifiers::CONTROL),

        KeyCode::KEY_LEFTALT => Some(Modifiers::ALT),

        KeyCode::KEY_RIGHTALT => Some(Modifiers::ALT_GRAPH),

        KeyCode::KEY_LEFTMETA | KeyCode::KEY_RIGHTMETA => Some(Modifiers::META),

        KeyCode::KEY_CAPSLOCK => Some(Modifiers::CAPS_LOCK),

        KeyCode::KEY_NUMLOCK => Some(Modifiers::NUM_LOCK),

        KeyCode::KEY_SCROLLLOCK => Some(Modifiers::SCROLL_LOCK),

        _ => None,
    }
}
