//! Read-only libarchive access, matching mpv's archive:// entry names. Audio is
//! streamed by mpv; only small VTT files are read here. Nothing is extracted.
use crate::{Error, Result};
use libloading::Library;
#[cfg(not(windows))]
use std::ffi::CString;
use std::{
    ffi::{c_char, c_int, c_void, CStr},
    path::{Component, Path},
};
use zip::read::HasZipMetadata;

type Handle = *mut c_void;
type New = unsafe extern "C" fn() -> Handle;
type Status = unsafe extern "C" fn(Handle) -> c_int;
#[cfg(not(windows))]
type Open = unsafe extern "C" fn(Handle, *const c_char, usize) -> c_int;
#[cfg(windows)]
type Open = unsafe extern "C" fn(Handle, *const u16, usize) -> c_int;
type Next = unsafe extern "C" fn(Handle, *mut Handle) -> c_int;
type Name = unsafe extern "C" fn(Handle) -> *const c_char;
type FileType = unsafe extern "C" fn(Handle) -> u32;
type Size = unsafe extern "C" fn(Handle) -> i64;
type Read = unsafe extern "C" fn(Handle, *mut c_void, usize) -> isize;

pub struct Entry {
    pub name: String,
    pub stream_name: String,
    pub size: u64,
}
struct Reader {
    handle: Handle,
    headers: std::cell::Cell<usize>,
    zip_names: std::collections::BTreeMap<Vec<u8>, String>,
    free: Status,
    next: Next,
    name: Name,
    utf8_name: Name,
    file_type: FileType,
    size: Size,
    read: Read,
    _library: Library,
    // mpv uses a UTF-8 CTYPE locale for its own libarchive reader too. Leave
    // LC_NUMERIC and the process locale alone; restore the caller's locale.
    #[cfg(unix)]
    locale: (libc::locale_t, libc::locale_t),
}
impl Reader {
    fn open(path: &Path) -> Result<Self> {
        #[cfg(target_os = "linux")]
        let names = ["libarchive.so.13"];
        #[cfg(target_os = "macos")]
        let names = ["libarchive.13.dylib", "libarchive.dylib"];
        #[cfg(target_os = "windows")]
        let names = ["libarchive-13.dll", "archive.dll", "libarchive.dll"];
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let names = ["libarchive.so.13"];
        // Keep the ZIP filename contract used by downloads: explicit Unicode
        // metadata wins; unlabelled Japanese Windows names use CP932, including
        // byte sequences that also happen to be valid UTF-8.
        let mut zip_names = std::collections::BTreeMap::new();
        if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("zip"))
        {
            let mut zip =
                zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|_| Error::Archive)?;
            if zip.len() > 30_000 {
                return Err(Error::ScanLimit);
            }
            for index in 0..zip.len() {
                let entry = zip.by_index_raw(index).map_err(|_| Error::Archive)?;
                let raw = entry.name_raw();
                let name = if entry.get_metadata().is_utf8 {
                    std::str::from_utf8(raw)
                        .map_err(|_| Error::Archive)?
                        .to_owned()
                } else {
                    encoding_rs::SHIFT_JIS
                        .decode_without_bom_handling_and_without_replacement(raw)
                        .ok_or(Error::Archive)?
                        .into_owned()
                };
                zip_names.insert(raw.to_vec(), name);
            }
        }
        // SAFETY: trusted system/application libraries, using libarchive's C ABI.
        unsafe {
            let sibling = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf));
            let library = names
                .iter()
                .find_map(|name| {
                    sibling
                        .as_ref()
                        .and_then(|p| Library::new(p.join(name)).ok())
                        .or_else(|| Library::new(name).ok())
                })
                .ok_or(Error::Archive)?;
            macro_rules! symbol {
                ($name:literal, $ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|_| Error::Archive)?
                };
            }
            let new = symbol!("archive_read_new", New);
            let formats = symbol!("archive_read_support_format_all", Status);
            #[cfg(not(windows))]
            let open = symbol!("archive_read_open_filename", Open);
            #[cfg(windows)]
            let open = symbol!("archive_read_open_filename_w", Open);
            let free = symbol!("archive_read_free", Status);
            let next = symbol!("archive_read_next_header", Next);
            let name = symbol!("archive_entry_pathname", Name);
            let utf8_name = symbol!("archive_entry_pathname_utf8", Name);
            let file_type = symbol!("archive_entry_filetype", FileType);
            let size = symbol!("archive_entry_size", Size);
            let read = symbol!("archive_read_data", Read);
            #[cfg(not(windows))]
            let path = CString::new(path.to_str().ok_or(Error::InvalidPath)?)
                .map_err(|_| Error::InvalidPath)?;
            #[cfg(windows)]
            let path = {
                use std::os::windows::ffi::OsStrExt;
                let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
                if wide.contains(&0) {
                    return Err(Error::InvalidPath);
                }
                wide.push(0);
                wide
            };
            let handle = new();
            if handle.is_null() {
                return Err(Error::Archive);
            }
            let reader = Self {
                handle,
                headers: std::cell::Cell::new(0),
                zip_names,
                free,
                next,
                name,
                utf8_name,
                file_type,
                size,
                read,
                _library: library,
                #[cfg(unix)]
                locale: {
                    let locale = libc::newlocale(
                        libc::LC_CTYPE_MASK,
                        c"C.UTF-8".as_ptr(),
                        std::ptr::null_mut(),
                    );
                    let old = if locale.is_null() {
                        std::ptr::null_mut()
                    } else {
                        libc::uselocale(locale)
                    };
                    (locale, old)
                },
            };
            if formats(handle) < 0 || open(handle, path.as_ptr(), 64 * 1024) < 0 {
                return Err(Error::Archive);
            }
            Ok(reader)
        }
    }
    fn next(&self, regular_index: &mut usize) -> Result<Option<Entry>> {
        // SAFETY: the handle is live and unique to this thread; entry/name
        // pointers are borrowed only until the next header/read call.
        unsafe {
            loop {
                self.headers.set(self.headers.get() + 1);
                if self.headers.get() > 30_000 {
                    return Err(Error::ScanLimit);
                }
                let mut entry = std::ptr::null_mut();
                let status = (self.next)(self.handle, &mut entry);
                if status == 1 {
                    return Ok(None);
                } // ARCHIVE_EOF
                if status < -20 || entry.is_null() {
                    return Err(Error::Archive);
                }
                // Regular files only: reject archive symlinks and other devices.
                if (self.file_type)(entry) != 0o100000 {
                    continue;
                }
                *regular_index += 1;
                if *regular_index > 30_000 {
                    return Err(Error::ScanLimit);
                }
                let raw = (self.name)(entry);
                let bytes = if raw.is_null() {
                    &[][..]
                } else {
                    CStr::from_ptr(raw).to_bytes()
                };
                let utf8 = (self.utf8_name)(entry);
                let utf8 = if utf8.is_null() {
                    None
                } else {
                    CStr::from_ptr(utf8).to_str().ok()
                };
                let stream_name = std::str::from_utf8(bytes)
                    .ok()
                    .filter(|s| !s.is_empty())
                    .or(utf8)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("mpv_unknown#{}", *regular_index - 1));
                let display = self.zip_names.get(bytes).cloned().or_else(|| {
                    std::str::from_utf8(bytes)
                        .ok()
                        .filter(|s| !s.is_empty())
                        .or(utf8)
                        .map(str::to_owned)
                        .or_else(|| {
                            encoding_rs::SHIFT_JIS
                                .decode_without_bom_handling_and_without_replacement(bytes)
                                .map(|s| s.into_owned())
                        })
                });
                let Some(display) = display else { continue };
                let name = display.replace('\\', "/");
                let mut name = name;
                while let Some(rest) = name.strip_prefix("./") {
                    name = rest.to_owned();
                }
                if !safe_member(&name) {
                    continue;
                }
                let size = (self.size)(entry);
                if size < 0 {
                    continue;
                }
                return Ok(Some(Entry {
                    name,
                    stream_name,
                    size: size as u64,
                }));
            }
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        // SAFETY: free the reader before unloading its library and restore only
        // this thread's locale after libarchive has finished using it.
        unsafe {
            (self.free)(self.handle);
            #[cfg(unix)]
            if !self.locale.0.is_null() {
                libc::uselocale(self.locale.1);
                libc::freelocale(self.locale.0);
            }
        }
    }
}
fn safe_member(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 4096
        && !name.contains(['\0', ':'])
        && !name.split('/').any(|part| part == "..")
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
pub fn entries(path: &Path) -> Result<Vec<Entry>> {
    let reader = Reader::open(path)?;
    let mut result = Vec::new();
    let mut index = 0;
    while let Some(entry) = reader.next(&mut index)? {
        result.push(entry);
    }
    // Ambiguous duplicate entry names cannot safely identify a file for playback.
    let mut seen = std::collections::BTreeSet::new();
    if result.iter().any(|e| !seen.insert(&e.name)) {
        return Err(Error::Archive);
    }
    Ok(result)
}
pub fn subtitle(path: &Path, name: &str) -> Result<Vec<u8>> {
    const LIMIT: usize = 8 * 1024 * 1024;
    let reader = Reader::open(path)?;
    let mut index = 0;
    while let Some(entry) = reader.next(&mut index)? {
        if entry.name != name {
            continue;
        }
        if entry.size > LIMIT as u64 {
            return Err(Error::InvalidSubtitle);
        }
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            // SAFETY: writable buffer, uniquely owned live reader. No archive
            // entries are ever executed or written to the filesystem.
            let count =
                unsafe { (reader.read)(reader.handle, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count < 0 {
                return Err(Error::Archive);
            }
            if count == 0 {
                return Ok(bytes);
            }
            if bytes.len() + count as usize > LIMIT {
                return Err(Error::InvalidSubtitle);
            }
            bytes.extend_from_slice(&buffer[..count as usize]);
        }
    }
    Err(Error::InvalidPath)
}
pub fn id(archive: &str, member: &str) -> String {
    let encoded: String = archive
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("archive:{encoded}/{member}")
}
pub fn location(id: &str) -> Result<Option<(String, &str)>> {
    let Some(rest) = id.strip_prefix("archive:") else {
        return Ok(None);
    };
    let (encoded, member) = rest.split_once('/').ok_or(Error::InvalidPath)?;
    if encoded.is_empty()
        || encoded.len() > 8192
        || encoded.len() % 2 != 0
        || !encoded.is_ascii()
        || !safe_member(member)
    {
        return Err(Error::InvalidPath);
    }
    let bytes = (0..encoded.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16).map_err(|_| Error::InvalidPath))
        .collect::<Result<Vec<_>>>()?;
    let archive = String::from_utf8(bytes).map_err(|_| Error::InvalidPath)?;
    Ok(Some((archive, member)))
}
pub fn stream_url(path: &Path, entry: &Entry) -> Result<String> {
    let path = path
        .to_str()
        .ok_or(Error::InvalidPath)?
        .replace('%', "%25")
        .replace('|', "%7C");
    Ok(format!("archive://{path}|{}", entry.stream_name))
}
