use super::Probe;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;

type AXUIElementRef = *const c_void;
type AXError = i32;
const SUCCESS: AXError = 0;
const NO_VALUE: AXError = -25212;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, secs: f32) -> AXError;
    fn AXUIElementCopyAttributeValue(
        el: AXUIElementRef,
        attr: CFStringRef,
        out: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementIsAttributeSettable(
        el: AXUIElementRef,
        attr: CFStringRef,
        out: *mut u8,
    ) -> AXError;
    fn AXUIElementGetPid(el: AXUIElementRef, pid: *mut i32) -> AXError;
}

/// An owned AXUIElement/CF value (released on drop).
struct Owned(CFType);

impl Owned {
    fn ptr(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef()
    }

    fn attr(&self, name: &'static str) -> Result<Owned, AXError> {
        let mut out: CFTypeRef = std::ptr::null();
        let key = CFString::from_static_string(name);
        let err = unsafe {
            AXUIElementCopyAttributeValue(self.ptr(), key.as_concrete_TypeRef(), &mut out)
        };
        match err {
            SUCCESS if out.is_null() => Err(NO_VALUE),
            SUCCESS => Ok(Owned(unsafe { CFType::wrap_under_create_rule(out) })),
            e => Err(e),
        }
    }

    fn string(&self, name: &'static str) -> String {
        self.attr(name)
            .ok()
            .and_then(|v| v.0.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn settable(&self, name: &'static str) -> bool {
        let mut out = 0u8;
        let key = CFString::from_static_string(name);
        let err = unsafe {
            AXUIElementIsAttributeSettable(self.ptr(), key.as_concrete_TypeRef(), &mut out)
        };
        err == SUCCESS && out != 0
    }
}

fn app_name(app: &Owned) -> String {
    let mut pid = 0;
    if unsafe { AXUIElementGetPid(app.ptr(), &mut pid) } != SUCCESS {
        return String::new();
    }
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return String::new();
    }
    let path = String::from_utf8_lossy(&buf[..n as usize]).to_string();
    path.rsplit('/').next().unwrap_or_default().to_string()
}

fn focused_app() -> Option<Owned> {
    let sys = unsafe { AXUIElementCreateSystemWide() };
    if sys.is_null() {
        return None;
    }
    let sys = Owned(unsafe { CFType::wrap_under_create_rule(sys) });
    unsafe { AXUIElementSetMessagingTimeout(sys.ptr(), 0.25) };
    let app = sys.attr("AXFocusedApplication").ok()?;
    unsafe { AXUIElementSetMessagingTimeout(app.ptr(), 0.25) };
    Some(app)
}

/// The frontmost app's executable name and its focused window's title
/// (empty when it has none). Off the main thread, like `probe`.
pub fn frontmost() -> Option<(String, String)> {
    let app = focused_app()?;
    let name = app_name(&app);
    if name.is_empty() {
        return None;
    }
    let title = app
        .attr("AXFocusedWindow")
        .map(|w| w.string("AXTitle"))
        .unwrap_or_default();
    Some((name, title))
}

/// The focused element's selected text: empty when nothing is selected,
/// None when the app doesn't say (web views, custom-drawn editors). Off
/// the main thread, like `probe`.
pub fn selected_text() -> Option<String> {
    focused_app()?
        .attr("AXFocusedUIElement")
        .ok()?
        .attr("AXSelectedText")
        .ok()?
        .0
        .downcast::<CFString>()
        .map(|s| s.to_string())
}

/// A handful of AX round-trips (~1ms total); a hung app costs at most
/// the messaging timeout per call before we give up and paste anyway.
pub fn probe() -> Probe {
    // Needs a WindowServer connection (any GUI app has one; a bare CLI
    // process gets kAXErrorCannotComplete → Failed → paste as before).
    let Some(app) = focused_app() else {
        return Probe::Failed;
    };
    let name = app_name(&app);
    if name.is_empty() {
        return Probe::Failed;
    }
    let el = match app.attr("AXFocusedUIElement") {
        Ok(el) => el,
        Err(NO_VALUE) => return Probe::NoElement { app: name },
        Err(_) => return Probe::Failed,
    };
    let role = el.string("AXRole");
    if role.is_empty() {
        return Probe::Failed;
    }
    let mut in_web = false;
    let mut cur = el.attr("AXParent").ok();
    for _ in 0..32 {
        let Some(p) = cur else { break };
        if p.string("AXRole") == "AXWebArea" {
            in_web = true;
            break;
        }
        cur = p.attr("AXParent").ok();
    }
    Probe::Element {
        app: name,
        subrole: el.string("AXSubrole"),
        value_settable: el.settable("AXValue"),
        role,
        in_web,
    }
}
