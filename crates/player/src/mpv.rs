//! Minimal libmpv C ABI boundary. One worker thread owns the handle and library.
//! Only typed, locally validated player operations are exposed to callers.
use crate::{Error, Result};
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_ulong, c_void, CString},
    ptr,
    sync::{Arc, Mutex, OnceLock},
};

#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
    playlist_insert_id: i64,
    playlist_insert_num_entries: c_int,
}
type Create = unsafe extern "C" fn() -> *mut c_void;
type Initialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type Destroy = unsafe extern "C" fn(*mut c_void);
type SetString = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type Property = unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int;
type Command = unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int;
type Wait = unsafe extern "C" fn(*mut c_void, f64) -> *const Event;

pub struct Mpv {
    handle: *mut c_void,
    destroy: Destroy,
    set: Property,
    get: Property,
    command_fn: Command,
    wait: Wait,
    // Retain library until after the handle has been destroyed.
    _library: Arc<Library>,
}
impl Mpv {
    pub fn new(null_audio: bool) -> Result<Self> {
        // Codec libraries can own process-wide callbacks and worker state.
        // Retain one successful load across player/probe handle lifetimes.
        static LIBRARY: OnceLock<Mutex<Option<Arc<Library>>>> = OnceLock::new();
        let mut cached = LIBRARY
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| Error::EngineInitialization)?;
        #[cfg(target_os = "linux")]
        let names = ["libmpv.so.2"];
        #[cfg(target_os = "macos")]
        let names = ["libmpv.2.dylib", "libmpv.dylib"];
        #[cfg(target_os = "windows")]
        let names = ["libmpv-2.dll", "mpv-2.dll"];
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let names = ["libmpv.so.2"];
        let sibling = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        let library = if let Some(library) = cached.as_ref() {
            library.clone()
        } else {
            let loaded = names
                .iter()
                .find_map(|name| {
                    // SAFETY: only trusted application-adjacent/system library names;
                    // all symbols below use the documented libmpv C ABI.
                    unsafe {
                        sibling
                            .as_ref()
                            .and_then(|p| Library::new(p.join(name)).ok())
                            .or_else(|| Library::new(name).ok())
                    }
                })
                .ok_or(Error::EngineUnavailable)?;
            let library = Arc::new(loaded);
            *cached = Some(library.clone());
            library
        };
        drop(cached);
        unsafe {
            let version: unsafe extern "C" fn() -> c_ulong = *library
                .get(b"mpv_client_api_version\0")
                .map_err(|_| Error::EngineUnavailable)?;
            if version() >> 16 != 2 {
                return Err(Error::EngineUnavailable);
            }
            let create: Create = *library
                .get(b"mpv_create\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let initialize: Initialize = *library
                .get(b"mpv_initialize\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let destroy: Destroy = *library
                .get(b"mpv_terminate_destroy\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let option: SetString = *library
                .get(b"mpv_set_option_string\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let set = *library
                .get(b"mpv_set_property\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let get = *library
                .get(b"mpv_get_property\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let command_fn = *library
                .get(b"mpv_command\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let wait = *library
                .get(b"mpv_wait_event\0")
                .map_err(|_| Error::EngineUnavailable)?;
            let handle = create();
            if handle.is_null() {
                return Err(Error::EngineInitialization);
            }
            let player = Self {
                handle,
                destroy,
                set,
                get,
                command_fn,
                wait,
                _library: library,
            };
            for (name, value) in [
                ("config", "no"),
                ("load-scripts", "no"),
                ("ytdl", "no"),
                ("sub-auto", "no"),
                ("audio-file-auto", "no"),
                ("access-references", "no"),
                ("video", "no"),
                ("audio-display", "no"),
                ("terminal", "no"),
                ("osc", "no"),
                ("idle", "yes"),
                ("pause", "yes"),
            ] {
                let name = CString::new(name).unwrap();
                let value = CString::new(value).unwrap();
                if option(handle, name.as_ptr(), value.as_ptr()) < 0 {
                    return Err(Error::EngineInitialization);
                }
            }
            if null_audio && option(handle, c"ao".as_ptr(), c"null".as_ptr()) < 0 {
                return Err(Error::EngineUnavailable);
            }
            if initialize(handle) < 0 {
                return Err(Error::EngineInitialization);
            }
            Ok(player)
        }
    }
    pub fn command(&self, args: &[&str]) -> Result<()> {
        let strings: Vec<_> = args
            .iter()
            .map(|s| CString::new(*s).map_err(|_| Error::InvalidPath))
            .collect::<Result<_>>()?;
        let pointers: Vec<_> = strings
            .iter()
            .map(|s| s.as_ptr())
            .chain(std::iter::once(ptr::null()))
            .collect();
        // SAFETY: null-terminated argv and strings remain alive for the call.
        if unsafe { (self.command_fn)(self.handle, pointers.as_ptr()) } < 0 {
            Err(Error::Playback)
        } else {
            Ok(())
        }
    }
    pub fn number(&self, name: &str) -> Option<f64> {
        let name = CString::new(name).ok()?;
        let mut value = 0f64;
        // SAFETY: MPV_FORMAT_DOUBLE (5) writes one f64 into the valid out pointer.
        (unsafe {
            (self.get)(
                self.handle,
                name.as_ptr(),
                5,
                (&mut value as *mut f64).cast(),
            )
        } >= 0
            && value.is_finite())
        .then_some(value)
    }
    pub fn set_number(&self, name: &str, mut value: f64) -> Result<()> {
        if !value.is_finite() {
            return Err(Error::InvalidControl);
        }
        let name = CString::new(name).map_err(|_| Error::InvalidControl)?;
        // SAFETY: MPV_FORMAT_DOUBLE (5) reads the provided f64 synchronously.
        if unsafe {
            (self.set)(
                self.handle,
                name.as_ptr(),
                5,
                (&mut value as *mut f64).cast(),
            )
        } < 0
        {
            Err(Error::Playback)
        } else {
            Ok(())
        }
    }
    pub fn pause(&self, pause: bool) -> Result<()> {
        let mut value: c_int = pause.into();
        // SAFETY: MPV_FORMAT_FLAG (3) reads one C int synchronously.
        if unsafe {
            (self.set)(
                self.handle,
                c"pause".as_ptr(),
                3,
                (&mut value as *mut c_int).cast(),
            )
        } < 0
        {
            Err(Error::Playback)
        } else {
            Ok(())
        }
    }
    pub fn seeking(&self) -> bool {
        let mut value: c_int = 0;
        // SAFETY: MPV_FORMAT_FLAG (3) writes one C int to this valid pointer.
        unsafe {
            (self.get)(
                self.handle,
                c"seeking".as_ptr(),
                3,
                (&mut value as *mut c_int).cast(),
            ) >= 0
                && value != 0
        }
    }
    pub fn event(&self) -> Option<PlayerEvent> {
        // SAFETY: libmpv owns this event until the next wait call. Copy its fields
        // immediately; never retain a pointer or access absent event payloads.
        unsafe {
            let event = (self.wait)(self.handle, 0.0).as_ref()?;
            match event.id {
                0 => None,
                8 => Some(PlayerEvent::Loaded),
                7 if !event.data.is_null() => {
                    let end = &*event.data.cast::<EndFile>();
                    Some(match end.reason {
                        0 => PlayerEvent::Eof,
                        4 => PlayerEvent::Failed,
                        _ => PlayerEvent::Other,
                    })
                }
                _ => Some(PlayerEvent::Other),
            }
        }
    }
}
impl Drop for Mpv {
    fn drop(&mut self) {
        // SAFETY: uniquely owned live handle; library is still loaded here.
        unsafe {
            (self.destroy)(self.handle);
        }
    }
}
pub enum PlayerEvent {
    Loaded,
    Eof,
    Failed,
    Other,
}
