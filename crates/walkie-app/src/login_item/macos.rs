use super::Status;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{msg_send, rc::autoreleasepool};
use std::ffi::{c_char, CStr};

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}

/// `SMAppService.mainApp`, or None before macOS 13.
fn main_app() -> Option<*mut AnyObject> {
    let cls = AnyClass::get(c"SMAppService")?;
    let svc: *mut AnyObject = unsafe { msg_send![cls, mainAppService] };
    (!svc.is_null()).then_some(svc)
}

/// Reads the "open application" Apple event macOS launched us with;
/// it carries `keyAELaunchedAsLogInItem` when we're a login item. Only
/// current while the app is finishing launching (i.e. during setup).
/// None if there's no such event to read.
pub fn launched_at_login() -> Option<bool> {
    const OPEN_APP: u32 = u32::from_be_bytes(*b"oapp"); // kAEOpenApplication
    const PROP_DATA: u32 = u32::from_be_bytes(*b"prdt"); // keyAEPropData
    const LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit"); // keyAELaunchedAsLogInItem
    autoreleasepool(|_| unsafe {
        let mgr: *mut AnyObject = msg_send![
            AnyClass::get(c"NSAppleEventManager")?,
            sharedAppleEventManager
        ];
        let ev: *mut AnyObject = msg_send![mgr.as_ref()?, currentAppleEvent];
        let ev = ev.as_ref()?;
        let id: u32 = msg_send![ev, eventID];
        if id != OPEN_APP {
            return None;
        }
        let prop: *mut AnyObject = msg_send![ev, paramDescriptorForKeyword: PROP_DATA];
        Some(match prop.as_ref() {
            Some(p) => {
                let code: u32 = msg_send![p, enumCodeValue];
                code == LOGIN_ITEM
            }
            None => false,
        })
    })
}

pub fn status() -> Status {
    autoreleasepool(|_| match main_app() {
        Some(svc) => Status::from_raw(unsafe { msg_send![svc, status] }),
        None => Status::NotFound,
    })
}

pub fn set(enabled: bool) -> Result<(), String> {
    autoreleasepool(|_| {
        let svc = main_app().ok_or("launch at login needs macOS 13 or later")?;
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let out = &mut err as *mut *mut AnyObject;
        let ok: Bool = unsafe {
            if enabled {
                msg_send![svc, registerAndReturnError: out]
            } else {
                msg_send![svc, unregisterAndReturnError: out]
            }
        };
        if ok.as_bool() {
            return Ok(());
        }
        if err.is_null() {
            return Err("unknown error".into());
        }
        unsafe {
            let desc: *mut AnyObject = msg_send![err, localizedDescription];
            let s: *const c_char = msg_send![desc, UTF8String];
            Err(CStr::from_ptr(s).to_string_lossy().into_owned())
        }
    })
}
