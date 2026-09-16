use super::ffi_declarations::*;
use super::key_mapping::*;
use super::{DeviceId, DeviceState, HidContext};
use crate::keyboard_source::KeyboardEvent;
use keyboard_types::{KeyState, KeyboardEvent as KeyEvent, Modifiers};
use std::{
    ffi::{CString, c_void},
    ptr,
    sync::Arc,
};
use tracing::warn;

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_NUMBER_SINT32_TYPE: isize = 3;

/// # Safety
///
/// `context` must be the `HidContext` pointer registered with the HID
/// manager and must remain valid until the manager has been closed.
#[inline]
unsafe fn get_ctx<'a>(context: *mut c_void) -> Option<&'a mut HidContext> {
    if context.is_null() {
        None
    } else {
        // SAFETY: Enforced by the HID manager callback lifetime.
        Some(unsafe { &mut *(context as *mut HidContext) })
    }
}

/// Closes and releases an IOHIDDeviceRef owned by this crate.
///
/// The caller must only pass references created with IOHIDDeviceCreate.
pub(super) unsafe fn close_and_release_device(device: IOHIDDeviceRef) {
    if device.is_null() {
        return;
    }

    let _ = unsafe { IOHIDDeviceClose(device, 0) };

    unsafe {
        CFRelease(device as CFTypeRef);
    }
}

pub(super) unsafe fn create_cf_string(value: &str) -> Option<CFStringRef> {
    let c_string = CString::new(value).ok()?;

    let string = unsafe {
        CFStringCreateWithCString(
            ptr::null_mut(),
            c_string.as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        )
    };

    (!string.is_null()).then_some(string)
}

unsafe fn cf_num_i32(value: i32) -> Option<CFNumberRef> {
    let number = unsafe {
        CFNumberCreate(
            ptr::null_mut(),
            K_CF_NUMBER_SINT32_TYPE,
            &value as *const _ as *const c_void,
        )
    };

    (!number.is_null()).then_some(number)
}

// ============================================================
// Device callbacks
// ============================================================

pub(super) extern "C" fn matching_callback(
    context: *mut c_void,
    _result: i32,
    _sender: *mut c_void,
    device: IOHIDDeviceRef,
) {
    let Some(ctx) = (unsafe { get_ctx(context) }) else {
        return;
    };

    if device.is_null() {
        return;
    }

    let Some(keyboard) = super::map_to_keyboard(device) else {
        return;
    };

    let device_id = DeviceId::from(device);

    if ctx.devices.contains_key(&device_id) {
        warn!(?device_id, "Received duplicate macOS HID matching callback");

        return;
    }

    let keyboard = Arc::new(keyboard);

    ctx.keyboards.insert(device_id, Arc::clone(&keyboard));

    ctx.modifiers.insert(device_id, Modifiers::empty());

    ctx.devices.insert(
        device_id,
        DeviceState {
            keyboard: Arc::clone(&keyboard),
            device,
            consumed_device: None,
        },
    );

    if let Err(error) = ctx.sender.try_send(Ok(KeyboardEvent::Plugged(keyboard))) {
        warn!(
            error = %error,
            "Dropped Plugged event: channel full"
        );
    }
}

pub(super) extern "C" fn removal_callback(
    context: *mut c_void,
    _result: i32,
    _sender: *mut c_void,
    device: IOHIDDeviceRef,
) {
    let Some(ctx) = (unsafe { get_ctx(context) }) else {
        return;
    };

    if device.is_null() {
        return;
    }

    let device_id = DeviceId::from(device);

    // The callback executes on the HID thread, so the device cannot
    // concurrently disappear while we're manipulating this state.
    let Some(mut device_state) = ctx.devices.remove(&device_id) else {
        return;
    };

    ctx.keyboards.remove(&device_id);
    ctx.modifiers.remove(&device_id);

    if let Some(consumed_device) = device_state.consumed_device.take() {
        // SAFETY: This reference was created by this crate.
        unsafe {
            close_and_release_device(consumed_device);
        }
    }

    if let Err(error) = ctx
        .sender
        .try_send(Ok(KeyboardEvent::Unplugged(device_state.keyboard)))
    {
        warn!(
            error = %error,
            "Dropped Unplugged event: channel full"
        );
    }
}

// ============================================================
// Input callback
// ============================================================

pub(super) extern "C" fn input_callback(
    context: *mut c_void,
    _result: i32,
    _sender: *mut c_void,
    value: IOHIDValueRef,
) {
    let Some(ctx) = (unsafe { get_ctx(context) }) else {
        return;
    };

    if value.is_null() {
        return;
    }

    unsafe {
        let integer_value = IOHIDValueGetIntegerValue(value);

        let Some((state, repeat)) = macos_value_to_key_state(integer_value) else {
            return;
        };

        let element = IOHIDValueGetElement(value);

        if element.is_null() {
            return;
        }

        let usage = IOHIDElementGetUsage(element);

        if !is_keyboard_usage(usage) {
            return;
        }

        let device = IOHIDElementGetDevice(element);

        if device.is_null() {
            return;
        }

        let device_id = DeviceId::from(device);

        let Some(keyboard) = ctx.keyboards.get(&device_id) else {
            return;
        };

        let Some(modifiers) = ctx.modifiers.get_mut(&device_id) else {
            return;
        };

        let code = macos_hid_to_code(usage);

        let key = macos_hid_to_key(usage);

        let location = macos_hid_to_location(usage);

        let modifier = modifier_for_usage(usage);

        if state == KeyState::Down {
            if let Some(modifier) = modifier {
                if is_lock_modifier(usage) {
                    // Lock modifiers toggle state on key-down.
                    if modifiers.contains(modifier) {
                        modifiers.remove(modifier);
                    } else {
                        modifiers.insert(modifier);
                    }
                } else {
                    modifiers.insert(modifier);
                }
            }
        }

        let key_event = KeyEvent {
            state,
            key,
            code,
            location,
            modifiers: *modifiers,
            repeat,
            is_composing: false,
        };

        if let Err(error) = ctx
            .sender
            .try_send(Ok(KeyboardEvent::KeyAction(keyboard.clone(), key_event)))
        {
            warn!(
                error = %error,
                "Dropped key event: channel full"
            );
        }

        // For non-lock modifiers, the modifier is removed AFTER
        // emitting the release event. Therefore a Shift-up event
        // still correctly reports Shift as active during that event.
        if state == KeyState::Up {
            if let Some(modifier) = modifier {
                if !is_lock_modifier(usage) {
                    modifiers.remove(modifier);
                }
            }
        }
    }
}

// ============================================================
// CoreFoundation helpers
// ============================================================

pub(super) fn create_matching_dictionary() -> Option<CFMutableDictionaryRef> {
    unsafe {
        let dict = CFDictionaryCreateMutable(
            ptr::null_mut(),
            0,
            kCFTypeDictionaryKeyCallBacks,
            kCFTypeDictionaryValueCallBacks,
        );

        if dict.is_null() {
            return None;
        }

        let page_key = create_cf_string("DeviceUsagePage")?;

        let usage_key = create_cf_string("DeviceUsage")?;

        let page_number = cf_num_i32(super::USAGE_PAGE_GENERIC_DESKTOP)?;

        let usage_number = cf_num_i32(super::USAGE_KEYBOARD)?;

        CFDictionarySetValue(dict, page_key, page_number);

        CFDictionarySetValue(dict, usage_key, usage_number);

        CFRelease(page_key);
        CFRelease(usage_key);
        CFRelease(page_number);
        CFRelease(usage_number);

        Some(dict)
    }
}

pub(super) fn get_string_property(device: IOHIDDeviceRef, key: &str) -> Option<String> {
    if device.is_null() {
        return None;
    }

    unsafe {
        let cf_key = create_cf_string(key)?;

        let cf_value = IOHIDDeviceGetProperty(device, cf_key);

        CFRelease(cf_key);

        if cf_value.is_null() || CFGetTypeID(cf_value) != CFStringGetTypeID() {
            return None;
        }

        let mut buffer = [0_u8; 256];

        let success = CFStringGetCString(
            cf_value,
            buffer.as_mut_ptr() as *mut i8,
            buffer.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        ) != 0;

        if !success {
            return None;
        }

        let length = buffer
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(buffer.len());

        String::from_utf8(buffer[..length].to_vec()).ok()
    }
}

pub(super) fn get_int_property(device: IOHIDDeviceRef, key: &str) -> Option<i32> {
    if device.is_null() {
        return None;
    }

    unsafe {
        let cf_key = create_cf_string(key)?;

        let cf_value = IOHIDDeviceGetProperty(device, cf_key);

        CFRelease(cf_key);

        if cf_value.is_null() || CFGetTypeID(cf_value) != CFNumberGetTypeID() {
            return None;
        }

        let mut result = 0_i32;

        let success = CFNumberGetValue(
            cf_value,
            K_CF_NUMBER_SINT32_TYPE,
            &mut result as *mut _ as *mut c_void,
        ) != 0;

        success.then_some(result)
    }
}
