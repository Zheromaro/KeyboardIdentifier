use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub fn raw_to_code(vkey: VIRTUAL_KEY, is_e0: bool, scancode: u16) -> Code {
    match vkey {
        VK_ESCAPE => Code::Escape,

        VK_0 => Code::Digit0,
        VK_1 => Code::Digit1,
        VK_2 => Code::Digit2,
        VK_3 => Code::Digit3,
        VK_4 => Code::Digit4,
        VK_5 => Code::Digit5,
        VK_6 => Code::Digit6,
        VK_7 => Code::Digit7,
        VK_8 => Code::Digit8,
        VK_9 => Code::Digit9,

        VK_A => Code::KeyA,
        VK_B => Code::KeyB,
        VK_C => Code::KeyC,
        VK_D => Code::KeyD,
        VK_E => Code::KeyE,
        VK_F => Code::KeyF,
        VK_G => Code::KeyG,
        VK_H => Code::KeyH,
        VK_I => Code::KeyI,
        VK_J => Code::KeyJ,
        VK_K => Code::KeyK,
        VK_L => Code::KeyL,
        VK_M => Code::KeyM,
        VK_N => Code::KeyN,
        VK_O => Code::KeyO,
        VK_P => Code::KeyP,
        VK_Q => Code::KeyQ,
        VK_R => Code::KeyR,
        VK_S => Code::KeyS,
        VK_T => Code::KeyT,
        VK_U => Code::KeyU,
        VK_V => Code::KeyV,
        VK_W => Code::KeyW,
        VK_X => Code::KeyX,
        VK_Y => Code::KeyY,
        VK_Z => Code::KeyZ,

        VK_RETURN => {
            if is_e0 {
                Code::NumpadEnter
            } else {
                Code::Enter
            }
        }
        VK_SPACE => Code::Space,
        VK_TAB => Code::Tab,
        VK_BACK => Code::Backspace,

        VK_LSHIFT => Code::ShiftLeft,
        VK_RSHIFT => Code::ShiftRight,
        VK_LCONTROL => Code::ControlLeft,
        VK_RCONTROL => Code::ControlRight,
        VK_LMENU => Code::AltLeft,
        VK_RMENU => Code::AltRight,
        VK_LWIN => Code::MetaLeft,
        VK_RWIN => Code::MetaRight,
        VK_SHIFT => {
            if scancode == 0x36 {
                Code::ShiftRight
            } else {
                Code::ShiftLeft
            }
        }
        VK_CONTROL => {
            if is_e0 {
                Code::ControlRight
            } else {
                Code::ControlLeft
            }
        }
        VK_MENU => {
            if is_e0 {
                Code::AltRight
            } else {
                Code::AltLeft
            }
        }

        VK_CAPITAL => Code::CapsLock,
        VK_NUMLOCK => Code::NumLock,
        VK_SCROLL => Code::ScrollLock,

        VK_OEM_1 => Code::Semicolon,
        VK_OEM_PLUS => Code::Equal,
        VK_OEM_COMMA => Code::Comma,
        VK_OEM_MINUS => Code::Minus,
        VK_OEM_PERIOD => Code::Period,
        VK_OEM_2 => Code::Slash,
        VK_OEM_3 => Code::Backquote,
        VK_OEM_4 => Code::BracketLeft,
        VK_OEM_5 => Code::Backslash,
        VK_OEM_6 => Code::BracketRight,
        VK_OEM_7 => Code::Quote,

        VK_NUMPAD0 => Code::Numpad0,
        VK_NUMPAD1 => Code::Numpad1,
        VK_NUMPAD2 => Code::Numpad2,
        VK_NUMPAD3 => Code::Numpad3,
        VK_NUMPAD4 => Code::Numpad4,
        VK_NUMPAD5 => Code::Numpad5,
        VK_NUMPAD6 => Code::Numpad6,
        VK_NUMPAD7 => Code::Numpad7,
        VK_NUMPAD8 => Code::Numpad8,
        VK_NUMPAD9 => Code::Numpad9,
        VK_MULTIPLY => Code::NumpadMultiply,
        VK_ADD => Code::NumpadAdd,
        VK_SUBTRACT => Code::NumpadSubtract,
        VK_DECIMAL => Code::NumpadDecimal,
        VK_DIVIDE => Code::NumpadDivide,

        VK_PRIOR => Code::PageUp,
        VK_NEXT => Code::PageDown,
        VK_END => Code::End,
        VK_HOME => Code::Home,
        VK_LEFT => Code::ArrowLeft,
        VK_UP => Code::ArrowUp,
        VK_RIGHT => Code::ArrowRight,
        VK_DOWN => Code::ArrowDown,
        VK_INSERT => Code::Insert,
        VK_DELETE => Code::Delete,

        VK_SNAPSHOT => Code::PrintScreen,
        VK_PAUSE => Code::Pause,
        VK_APPS => Code::ContextMenu,

        VK_F1 => Code::F1,
        VK_F2 => Code::F2,
        VK_F3 => Code::F3,
        VK_F4 => Code::F4,
        VK_F5 => Code::F5,
        VK_F6 => Code::F6,
        VK_F7 => Code::F7,
        VK_F8 => Code::F8,
        VK_F9 => Code::F9,
        VK_F10 => Code::F10,
        VK_F11 => Code::F11,
        VK_F12 => Code::F12,
        VK_F13 => Code::F13,
        VK_F14 => Code::F14,
        VK_F15 => Code::F15,
        VK_F16 => Code::F16,
        VK_F17 => Code::F17,
        VK_F18 => Code::F18,
        VK_F19 => Code::F19,
        VK_F20 => Code::F20,
        VK_F21 => Code::F21,
        VK_F22 => Code::F22,
        VK_F23 => Code::F23,
        VK_F24 => Code::F24,

        _ => Code::Unidentified,
    }
}

pub fn raw_to_location(vkey: VIRTUAL_KEY, is_e0: bool) -> Location {
    match vkey {
        VK_LSHIFT | VK_LCONTROL | VK_LMENU | VK_LWIN => Location::Left,
        VK_RSHIFT | VK_RCONTROL | VK_RMENU | VK_RWIN => Location::Right,
        VK_CONTROL | VK_MENU => {
            if is_e0 {
                Location::Right
            } else {
                Location::Left
            }
        }
        VK_NUMPAD0 | VK_NUMPAD1 | VK_NUMPAD2 | VK_NUMPAD3 | VK_NUMPAD4 | VK_NUMPAD5
        | VK_NUMPAD6 | VK_NUMPAD7 | VK_NUMPAD8 | VK_NUMPAD9 | VK_MULTIPLY | VK_ADD
        | VK_SUBTRACT | VK_DECIMAL | VK_DIVIDE => Location::Numpad,
        VK_RETURN if is_e0 => Location::Numpad,
        _ => Location::Standard,
    }
}

pub fn raw_to_key(vkey: VIRTUAL_KEY) -> Key {
    let named_key = match vkey {
        VK_ESCAPE => Some(NamedKey::Escape),
        VK_RETURN => Some(NamedKey::Enter),
        VK_TAB => Some(NamedKey::Tab),
        VK_BACK => Some(NamedKey::Backspace),
        VK_DELETE => Some(NamedKey::Delete),
        VK_INSERT => Some(NamedKey::Insert),

        VK_HOME => Some(NamedKey::Home),
        VK_END => Some(NamedKey::End),
        VK_PRIOR => Some(NamedKey::PageUp),
        VK_NEXT => Some(NamedKey::PageDown),

        VK_UP => Some(NamedKey::ArrowUp),
        VK_DOWN => Some(NamedKey::ArrowDown),
        VK_LEFT => Some(NamedKey::ArrowLeft),
        VK_RIGHT => Some(NamedKey::ArrowRight),

        VK_CAPITAL => Some(NamedKey::CapsLock),
        VK_NUMLOCK => Some(NamedKey::NumLock),
        VK_SCROLL => Some(NamedKey::ScrollLock),

        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some(NamedKey::Shift),
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => Some(NamedKey::Control),
        VK_MENU | VK_LMENU | VK_RMENU => Some(NamedKey::Alt),
        VK_LWIN | VK_RWIN => Some(NamedKey::Meta),

        VK_SNAPSHOT => Some(NamedKey::PrintScreen),
        VK_PAUSE => Some(NamedKey::Pause),
        VK_APPS => Some(NamedKey::ContextMenu),

        VK_F1 => Some(NamedKey::F1),
        VK_F2 => Some(NamedKey::F2),
        VK_F3 => Some(NamedKey::F3),
        VK_F4 => Some(NamedKey::F4),
        VK_F5 => Some(NamedKey::F5),
        VK_F6 => Some(NamedKey::F6),
        VK_F7 => Some(NamedKey::F7),
        VK_F8 => Some(NamedKey::F8),
        VK_F9 => Some(NamedKey::F9),
        VK_F10 => Some(NamedKey::F10),
        VK_F11 => Some(NamedKey::F11),
        VK_F12 => Some(NamedKey::F12),
        VK_F13 => Some(NamedKey::F13),
        VK_F14 => Some(NamedKey::F14),
        VK_F15 => Some(NamedKey::F15),
        VK_F16 => Some(NamedKey::F16),
        VK_F17 => Some(NamedKey::F17),
        VK_F18 => Some(NamedKey::F18),
        VK_F19 => Some(NamedKey::F19),
        VK_F20 => Some(NamedKey::F20),
        VK_F21 => Some(NamedKey::F21),
        VK_F22 => Some(NamedKey::F22),
        VK_F23 => Some(NamedKey::F23),
        VK_F24 => Some(NamedKey::F24),

        _ => None,
    };

    match named_key {
        Some(key) => Key::Named(key),
        None => Key::Named(NamedKey::Unidentified),
    }
}

pub fn modifier_for_key(vkey: VIRTUAL_KEY, is_e0: bool) -> Option<Modifiers> {
    match vkey {
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some(Modifiers::SHIFT),
        VK_CONTROL | VK_LCONTROL => Some(Modifiers::CONTROL),
        VK_RCONTROL => Some(Modifiers::CONTROL),
        VK_LMENU => Some(Modifiers::ALT),
        VK_RMENU => Some(Modifiers::ALT_GRAPH),
        VK_MENU => {
            if is_e0 {
                Some(Modifiers::ALT_GRAPH)
            } else {
                Some(Modifiers::ALT)
            }
        }
        VK_LWIN | VK_RWIN => Some(Modifiers::META),
        VK_CAPITAL => Some(Modifiers::CAPS_LOCK),
        VK_NUMLOCK => Some(Modifiers::NUM_LOCK),
        VK_SCROLL => Some(Modifiers::SCROLL_LOCK),
        _ => None,
    }
}
