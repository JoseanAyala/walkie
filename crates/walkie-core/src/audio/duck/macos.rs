use super::OutputVolume;

/// The default output device's volume through CoreAudio — no subprocess,
/// so ducking doesn't delay the start of a recording.
pub struct SystemVolume;

mod ca {
    use std::ffi::c_void;

    #[repr(C)]
    pub struct Address {
        pub selector: u32,
        pub scope: u32,
        pub element: u32,
    }

    const fn fourcc(s: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*s)
    }
    pub const SYSTEM_OBJECT: u32 = 1;
    pub const DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
    pub const SCOPE_GLOBAL: u32 = fourcc(b"glob");
    pub const SCOPE_OUTPUT: u32 = fourcc(b"outp");
    pub const MAIN_VOLUME: u32 = fourcc(b"vmvc"); // VirtualMainVolume
    pub const VOLUME_SCALAR: u32 = fourcc(b"volm");
    pub const MUTE: u32 = fourcc(b"mute");

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        pub fn AudioObjectHasProperty(id: u32, addr: *const Address) -> u8;
        pub fn AudioObjectGetPropertyData(
            id: u32,
            addr: *const Address,
            qual_size: u32,
            qual: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
        pub fn AudioObjectSetPropertyData(
            id: u32,
            addr: *const Address,
            qual_size: u32,
            qual: *const c_void,
            size: u32,
            data: *const c_void,
        ) -> i32;
    }

    pub fn get<T: Default>(id: u32, selector: u32, scope: u32, element: u32) -> Option<T> {
        let addr = Address {
            selector,
            scope,
            element,
        };
        let mut v = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        // SAFETY: `v` is a plain value of `size` bytes; CoreAudio writes at most that.
        let st = unsafe {
            if AudioObjectHasProperty(id, &addr) == 0 {
                return None;
            }
            AudioObjectGetPropertyData(
                id,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut v as *mut T as *mut c_void,
            )
        };
        (st == 0).then_some(v)
    }

    pub fn set<T>(id: u32, selector: u32, scope: u32, element: u32, v: T) -> bool {
        let addr = Address {
            selector,
            scope,
            element,
        };
        let size = std::mem::size_of::<T>() as u32;
        // SAFETY: passes a pointer to `v` and its exact size.
        unsafe {
            AudioObjectHasProperty(id, &addr) != 0
                && AudioObjectSetPropertyData(
                    id,
                    &addr,
                    0,
                    std::ptr::null(),
                    size,
                    &v as *const T as *const c_void,
                ) == 0
        }
    }

    pub fn default_output() -> Option<u32> {
        get::<u32>(SYSTEM_OBJECT, DEFAULT_OUTPUT, SCOPE_GLOBAL, 0).filter(|&d| d != 0)
    }
}

impl OutputVolume for SystemVolume {
    fn get(&self) -> Option<f32> {
        use ca::*;
        let dev = default_output()?;
        if get::<u32>(dev, MUTE, SCOPE_OUTPUT, 0).unwrap_or(0) != 0 {
            return None;
        }
        // The virtual main volume is what the menu bar slider shows; devices
        // without one expose per-channel scalars instead.
        get::<f32>(dev, MAIN_VOLUME, SCOPE_OUTPUT, 0)
            .or_else(|| get::<f32>(dev, VOLUME_SCALAR, SCOPE_OUTPUT, 0))
            .or_else(|| get::<f32>(dev, VOLUME_SCALAR, SCOPE_OUTPUT, 1))
    }

    fn set(&self, v: f32) {
        use ca::*;
        let Some(dev) = default_output() else { return };
        let v = v.clamp(0.0, 1.0);
        if set(dev, MAIN_VOLUME, SCOPE_OUTPUT, 0, v) || set(dev, VOLUME_SCALAR, SCOPE_OUTPUT, 0, v)
        {
            return;
        }
        for ch in [1, 2] {
            set(dev, VOLUME_SCALAR, SCOPE_OUTPUT, ch, v);
        }
    }
}
