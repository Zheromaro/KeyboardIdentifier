use super::ffi_declarations::*;
use super::{HidContext, PRESSED, USAGE_KEYBOARD, USAGE_PAGE_GENERIC_DESKTOP, map_to_keyboard};
use crate::keyboard_source::KeyboardEvent;
use keyboard_types::Code;
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
        ctx.keyboards.insert(device as isize, kb_arc.clone());
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
    if let Some(kb) = ctx.keyboards.remove(&(device as isize)) {
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
        if int_value == PRESSED as isize {
            let element = IOHIDValueGetElement(value);
            let usage = IOHIDElementGetUsage(element) as usize;
            let code = MACOS_HID_MAP
                .get(usage)
                .copied()
                .unwrap_or(Code::Unidentified);

            let device = IOHIDElementGetDevice(element);
            if let Some(kb) = ctx.keyboards.get(&(device as isize)) {
                if let Err(e) = ctx
                    .sender
                    .try_send(Ok(KeyboardEvent::Pressed(kb.clone(), code)))
                {
                    warn!(error = %e, "Dropped Pressed event: channel full");
                }
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
        let page_val = USAGE_PAGE_GENERIC_DESKTOP;
        let usage_val = USAGE_KEYBOARD;
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
