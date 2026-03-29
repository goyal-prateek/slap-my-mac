//! Wake Apple SPU HID drivers so accelerometer reports actually flow.
//! See: https://github.com/taigrr/apple-silicon-accelerometer (wakeSPUDrivers).

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_void, CString};
use std::ptr;

type io_object_t = u32;
type io_iterator_t = u32;
type kern_return_t = i32;
type CFStringRef = *const c_void;
type CFNumberRef = *const c_void;
type CFTypeRef = *const c_void;

const KERN_SUCCESS: kern_return_t = 0;
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_NUMBER_SINT32_TYPE: i32 = 3;

#[link(name = "IOKit", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
  fn IOServiceMatching(name: *const c_char) -> *mut c_void;
  fn IOServiceGetMatchingServices(
    main_port: u32,
    matching: *mut c_void,
    existing: *mut io_iterator_t,
  ) -> kern_return_t;
  fn IOIteratorNext(iterator: io_iterator_t) -> io_object_t;
  fn IOObjectRelease(object: io_object_t) -> kern_return_t;
  fn IORegistryEntrySetCFProperty(
    entry: io_object_t,
    property_name: CFStringRef,
    property: CFTypeRef,
  ) -> kern_return_t;

  fn CFStringCreateWithCString(
    alloc: *mut c_void,
    c_str: *const c_char,
    encoding: u32,
  ) -> CFStringRef;
  fn CFNumberCreate(
    alloc: *mut c_void,
    the_type: i32,
    value_ptr: *const c_void,
  ) -> CFNumberRef;
  fn CFRelease(cf: CFTypeRef);
}

unsafe fn cf_string(s: &str) -> Option<CFStringRef> {
  let c = CString::new(s).ok()?;
  let r = CFStringCreateWithCString(ptr::null_mut(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8);
  if r.is_null() {
    None
  } else {
    Some(r)
  }
}

unsafe fn set_i32_prop(entry: io_object_t, key: &str, val: i32) {
  let Some(k) = cf_string(key) else {
    return;
  };
  let mut v = val;
  let num = CFNumberCreate(
    ptr::null_mut(),
    K_CF_NUMBER_SINT32_TYPE,
    &mut v as *mut i32 as *const c_void,
  );
  if !num.is_null() {
    let _ = IORegistryEntrySetCFProperty(entry, k, num as CFTypeRef);
    CFRelease(num as CFTypeRef);
  }
  CFRelease(k as CFTypeRef);
}

/// Sets registry properties on `AppleSPUHIDDriver` services so the IMU starts reporting.
pub fn wake_spu_drivers() {
  unsafe {
    let Some(name) = CString::new("AppleSPUHIDDriver").ok() else {
      return;
    };
    let matching = IOServiceMatching(name.as_ptr());
    if matching.is_null() {
      return;
    }
    let mut it: io_iterator_t = 0;
    let kr = IOServiceGetMatchingServices(0, matching, &mut it);
    if kr != KERN_SUCCESS {
      return;
    }
    loop {
      let svc = IOIteratorNext(it);
      if svc == 0 {
        break;
      }
      set_i32_prop(svc, "SensorPropertyReportingState", 1);
      set_i32_prop(svc, "SensorPropertyPowerState", 1);
      set_i32_prop(svc, "ReportInterval", 1000);
      let _ = IOObjectRelease(svc);
    }
    let _ = IOObjectRelease(it);
  }
}
