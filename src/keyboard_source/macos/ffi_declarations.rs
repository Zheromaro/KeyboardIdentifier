use std::ffi::c_void;

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFMutableDictionaryRef = *mut c_void;
pub type CFNumberRef = *const c_void;
pub type CFSetRef = *const c_void;
pub type CFRunLoopRef = *mut c_void;
pub type IOHIDManagerRef = *mut c_void;
pub type IOHIDDeviceRef = *mut c_void;
pub type IOHIDValueRef = *mut c_void;
pub type IOHIDElementRef = *mut c_void;
pub type IOHIDDeviceCallback = extern "C" fn(*mut c_void, i32, *mut c_void, IOHIDDeviceRef);
pub type IOHIDValueCallback = extern "C" fn(*mut c_void, i32, *mut c_void, IOHIDValueRef);

/// `io_service_t` is a Mach port name used by IOKit.
pub type IoServiceT = u32;

unsafe extern "C" {
    pub static kCFTypeDictionaryKeyCallBacks: *const c_void;
    pub static kCFTypeDictionaryValueCallBacks: *const c_void;

    pub fn CFStringCreateWithCString(
        alloc: *mut c_void,
        cStr: *const i8,
        encoding: u32,
    ) -> CFStringRef;

    pub fn CFNumberCreate(
        allocator: *mut c_void,
        theType: isize,
        valuePtr: *const c_void,
    ) -> CFNumberRef;

    pub fn CFDictionaryCreateMutable(
        allocator: *mut c_void,
        capacity: isize,
        keyCallBacks: *const c_void,
        valueCallBacks: *const c_void,
    ) -> CFMutableDictionaryRef;

    pub fn CFDictionarySetValue(
        theDict: CFMutableDictionaryRef,
        key: *const c_void,
        value: *const c_void,
    );

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFGetTypeID(cf: CFTypeRef) -> usize;
    pub fn CFStringGetTypeID() -> usize;
    pub fn CFNumberGetTypeID() -> usize;

    pub fn CFStringGetCString(
        theString: CFStringRef,
        buffer: *mut i8,
        bufferSize: isize,
        encoding: u32,
    ) -> u8;

    pub fn CFNumberGetValue(number: CFNumberRef, theType: isize, valuePtr: *mut c_void) -> u8;
    pub fn CFSetGetCount(theSet: CFSetRef) -> isize;
    pub fn CFSetGetValues(theSet: CFSetRef, values: *mut *const c_void);
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopRun();
    pub fn CFRunLoopStop(rl: CFRunLoopRef);

    // IOKit HID
    pub fn IOHIDManagerCreate(allocator: *mut c_void, options: u32) -> IOHIDManagerRef;
    pub fn IOHIDManagerSetDeviceMatching(manager: IOHIDManagerRef, matching: CFDictionaryRef);
    pub fn IOHIDManagerRegisterDeviceMatchingCallback(
        manager: IOHIDManagerRef,
        callback: IOHIDDeviceCallback,
        context: *mut c_void,
    );
    pub fn IOHIDManagerRegisterDeviceRemovalCallback(
        manager: IOHIDManagerRef,
        callback: IOHIDDeviceCallback,
        context: *mut c_void,
    );
    pub fn IOHIDManagerRegisterInputValueCallback(
        manager: IOHIDManagerRef,
        callback: IOHIDValueCallback,
        context: *mut c_void,
    );
    pub fn IOHIDManagerScheduleWithRunLoop(
        manager: IOHIDManagerRef,
        runLoop: CFRunLoopRef,
        runLoopMode: CFStringRef,
    );
    pub fn IOHIDManagerUnscheduleFromRunLoop(
        manager: IOHIDManagerRef,
        runLoop: CFRunLoopRef,
        runLoopMode: CFStringRef,
    );
    pub fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: u32) -> i32;
    pub fn IOHIDManagerClose(manager: IOHIDManagerRef, options: u32) -> i32;
    pub fn IOHIDManagerCopyDevices(manager: IOHIDManagerRef) -> CFSetRef;

    pub fn IOHIDDeviceCreate(allocator: *mut c_void, service: IoServiceT) -> IOHIDDeviceRef;

    pub fn IOHIDDeviceGetService(device: IOHIDDeviceRef) -> IoServiceT;

    pub fn IOHIDDeviceOpen(device: IOHIDDeviceRef, options: u32) -> i32;

    pub fn IOHIDDeviceClose(device: IOHIDDeviceRef, options: u32) -> i32;

    pub fn IOHIDDeviceGetProperty(device: IOHIDDeviceRef, key: CFStringRef) -> CFTypeRef;
    pub fn IOHIDValueGetIntegerValue(value: IOHIDValueRef) -> isize;
    pub fn IOHIDValueGetElement(value: IOHIDValueRef) -> IOHIDElementRef;
    pub fn IOHIDElementGetUsage(element: IOHIDElementRef) -> u32;
    pub fn IOHIDElementGetDevice(element: IOHIDElementRef) -> IOHIDDeviceRef;
}
