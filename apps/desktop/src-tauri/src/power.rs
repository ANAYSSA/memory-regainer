//! Operation-scoped idle sleep protection. User sleep, lid closure and shutdown
//! remain under OS control. The guard must stay on its creating worker thread.
use std::{marker::PhantomData, rc::Rc};

pub struct SleepGuard {
    #[cfg(target_os = "macos")]
    assertion: u32,
    _thread_bound: PhantomData<Rc<()>>,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn SetThreadExecutionState(flags: u32) -> u32;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        allocator: *const std::ffi::c_void,
        bytes: *const std::ffi::c_char,
        encoding: u32,
    ) -> *const std::ffi::c_void;
    fn CFRelease(value: *const std::ffi::c_void);
}

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        kind: *const std::ffi::c_void,
        level: u32,
        name: *const std::ffi::c_void,
        id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
}

impl SleepGuard {
    pub fn acquire() -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            // ES_CONTINUOUS | ES_SYSTEM_REQUIRED; do not keep the display lit.
            if unsafe { SetThreadExecutionState(0x80000001) } == 0 {
                return Err("Windows could not enable idle sleep protection".into());
            }
            Ok(Self {
                _thread_bound: PhantomData,
            })
        }
        #[cfg(target_os = "macos")]
        {
            let mut assertion = 0;
            // Static NUL-terminated UTF-8 strings; CF objects are released on
            // every branch and IOKit owns the assertion until guard drop.
            let status = unsafe {
                let kind = CFStringCreateWithCString(
                    std::ptr::null(),
                    c"PreventUserIdleSystemSleep".as_ptr(),
                    0x08000100,
                );
                let name = CFStringCreateWithCString(
                    std::ptr::null(),
                    c"Memory Regainer active disk operation".as_ptr(),
                    0x08000100,
                );
                if kind.is_null() || name.is_null() {
                    if !kind.is_null() {
                        CFRelease(kind);
                    }
                    if !name.is_null() {
                        CFRelease(name);
                    }
                    return Err("Cannot allocate macOS sleep assertion".into());
                }
                let status = IOPMAssertionCreateWithName(kind, 255, name, &mut assertion);
                CFRelease(kind);
                CFRelease(name);
                status
            };
            if status != 0 {
                return Err(format!(
                    "macOS could not enable idle sleep protection (IOKit {status})"
                ));
            }
            Ok(Self {
                assertion,
                _thread_bound: PhantomData,
            })
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Err("Automatic idle sleep protection is supported on Windows and macOS".into())
        }
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            SetThreadExecutionState(0x80000000);
        }
        #[cfg(target_os = "macos")]
        unsafe {
            IOPMAssertionRelease(self.assertion);
        }
    }
}

#[cfg(all(test, any(target_os = "windows", target_os = "macos")))]
mod tests {
    use super::*;
    #[test]
    fn native_sleep_guard_acquires_and_releases_on_worker() {
        std::thread::spawn(|| {
            let guard = SleepGuard::acquire().expect("native idle sleep assertion");
            drop(guard);
        })
        .join()
        .unwrap();
    }
}
