use super::ffi_declarations::*;
use super::key_mapping::*;
use super::{HidContext, map_to_keyboard};
use crate::keyboard_source::KeyboardEvent;
use keyboard_types::{KeyboardEvent as KeyEvent, Modifiers};
use std::{ffi::c_void, ptr, sync::Arc};
use tracing::warn;

pub(super) extern "C" fn matching_callback(
    context: *mut c_void,
    _result: i32,
    _sender: *mut c_void,
    device: IOHIDDeviceRef,
) {
    if context.is_null() {
        return;
    }
    let ctx = unsafe { &mut *(context as *mut HidContext) };
    if let Some(keyboard) = map_to_keyboard(device) {
        let kb_arc = Arc::new(keyboard);
        let device_id = device as isize;
        ctx.keyboards.insert(device_id, kb_arc.clone());
        ctx.modifiers.insert(device_id, Modifiers::empty());
        if let Err(e) = ctx.sender.try_send(Ok(KeyboardEvent::Plugged(kb_arc))) {
            warn!(error = %e, "Dropped Plugged event: channel full");
        }
    }
}

pub(super) extern "C" fn removal_callback(
    context: *mut c_void,
    _result: i32,
    _sender: *mut c_void,
    device: IOHIDDeviceRef,
) {
    if context.is_null() {
        return;
    }
    let ctx = unsafe { &mut *(context as *mut HidContext) };
    let device_id = device as isize;
    ctx.modifiers.remove(&device_id);
    if let Some(kb) = ctx.keyboards.remove(&device_id) {
        if let Err(e) = ctx.sender.try_send(Ok(KeyboardEvent::Unplugged(kb))) {
            warn!(error = %e, "Dropped Unplugged event: channel full");
        }
    }
}

pub(super) extern "C" fn input_callback(
    context: *mut c_void,
    _result: i32,
    _sender: *mut c_void,
    value: IOHIDValueRef,
) {
    if context.is_null() {
        return;
    }
    let ctx = unsafe { &mut *(context as *mut HidContext) };
    unsafe {
        let int_value = IOHIDValueGetIntegerValue(value);
        let Some((state, repeat)) = macos_value_to_key_state(int_value) else {
            return;
        };

        let element = IOHIDValueGetElement(value);
        let usage = IOHIDElementGetUsage(element) as usize;
        let device = IOHIDElementGetDevice(element);
        let device_id = device as isize;

        let Some(kb) = ctx.keyboards.get(&device_id) else {
            return;
        };

        let code = macos_hid_to_code(usage);
        let key = macos_hid_to_key(usage);
        let location = macos_hid_to_location(usage);
        let modifier = modifier_for_usage(usage);

        let Some(modifiers) = ctx.modifiers.get_mut(&device_id) else {
            return;
        };

        let event_modifiers = match state {
            keyboard_types::KeyState::Down => {
                if let Some(modifier) = modifier {
                    modifiers.insert(modifier);
                }
                *modifiers
            }
            keyboard_types::KeyState::Up => *modifiers,
        };

        let key_event = KeyEvent {
            state,
            key,
            code,
            location,
            modifiers: event_modifiers,
            repeat,
            is_composing: false,
        };

        if let Err(e) = ctx
            .sender
            .try_send(Ok(KeyboardEvent::Pressed(kb.clone(), key_event)))
        {
            warn!(error = %e, "Dropped Pressed event: channel full");
        }

        // Remove modifier after sending the Up event
        if state == keyboard_types::KeyState::Up {
            if let Some(modifier) = modifier {
                modifiers.remove(modifier);
            }
        }
    }
}

// --- Helper Functions ---

pub(super) fn create_matching_dictionary() -> CFMutableDictionaryRef {
    unsafe {
        let dict = CFDictionaryCreateMutable(
            ptr::null_mut(),
            0,
            kCFTypeDictionaryKeyCallBacks,
            kCFTypeDictionaryValueCallBacks,
        );
        let page_key = CFStringCreateWithCString(
            ptr::null_mut(),
            b"DeviceUsagePage\0".as_ptr() as _,
            0x08000100,
        );
        let usage_key =
            CFStringCreateWithCString(ptr::null_mut(), b"DeviceUsage\0".as_ptr() as _, 0x08000100);
        let page_val = super::USAGE_PAGE_GENERIC_DESKTOP;
        let usage_val = super::USAGE_KEYBOARD;
        let page_num = CFNumberCreate(ptr::null_mut(), 3, &page_val as *const _ as _);
        let usage_num = CFNumberCreate(ptr::null_mut(), 3, &usage_val as *const _ as _);

        CFDictionarySetValue(dict, page_key, page_num);
        CFDictionarySetValue(dict, usage_key, usage_num);
        CFRelease(page_key);
        CFRelease(usage_key);
        CFRelease(page_num);
        CFRelease(usage_num);
        dict
    }
}

pub(super) fn get_string_property(device: IOHIDDeviceRef, key: &[u8]) -> Option<String> {
    unsafe {
        let cf_key = CFStringCreateWithCString(ptr::null_mut(), key.as_ptr() as _, 0x08000100);
        let cf_val = IOHIDDeviceGetProperty(device, cf_key);
        CFRelease(cf_key);

        if cf_val.is_null() || CFGetTypeID(cf_val) != CFStringGetTypeID() {
            return None;
        }
        let mut buffer = vec![0u8; 256];
        if CFStringGetCString(
            cf_val,
            buffer.as_mut_ptr() as *mut i8,
            buffer.len() as isize,
            0x08000100,
        ) != 0
        {
            if let Some(end) = buffer.iter().position(|&c| c == 0) {
                buffer.truncate(end);
            }
            String::from_utf8(buffer).ok()
        } else {
            None
        }
    }
}

pub(super) fn get_int_property(device: IOHIDDeviceRef, key: &[u8]) -> Option<i32> {
    unsafe {
        let cf_key = CFStringCreateWithCString(ptr::null_mut(), key.as_ptr() as _, 0x08000100);
        let cf_val = IOHIDDeviceGetProperty(device, cf_key);
        CFRelease(cf_key);

        if cf_val.is_null() || CFGetTypeID(cf_val) != CFNumberGetTypeID() {
            return None;
        }
        let mut result: i32 = 0;
        if CFNumberGetValue(cf_val, 3, &mut result as *mut _ as _) != 0 {
            Some(result)
        } else {
            None
        }
    }
}
