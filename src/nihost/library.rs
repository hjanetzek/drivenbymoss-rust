// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! `libdbm_nihost.so` (support/linux/ni_host/include/dbm_nihost.h), loaded at runtime with
//! `dlopen` (from the C library, which std links already: no crate), and the checks of the
//! system for the transport selection: the device's hidraw node (sysfs), whether another process
//! holds it (the library's exclusive `flock`), whether dbm-nihost runs (`/proc`).

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{DeviceIo, DeviceState, HidDevice, SystemProbe};

extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn flock(fd: c_int, operation: c_int) -> c_int;
}

const RTLD_NOW: c_int = 2;
const LOCK_EX: c_int = 2;
const LOCK_NB: c_int = 4;
const LOCK_UN: c_int = 8;
const O_NONBLOCK: i32 = 0o4000;

/// The API version of dbm_nihost.h this binding is written for.
const API_VERSION: c_int = 1;

/// The environment variable with the path of the library (takes the place of the search).
pub const ENV_LIBRARY: &str = "DBM_NIHOST_LIBRARY";

/// The read buffer: many messages per read (the longest is a sysex of a few dozen bytes, the
/// library wants 2 bytes length plus the message to fit).
const READ_BUFFER_BYTES: usize = 4096;

type ApiVersionFn = unsafe extern "C" fn() -> c_int;
type OpenFn = unsafe extern "C" fn(c_int) -> *mut c_void;
type CloseFn = unsafe extern "C" fn(*mut c_void);
type IsAttachedFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type ReadFn = unsafe extern "C" fn(*mut c_void, *mut u8, usize, c_int) -> c_int;
type SendFn = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> c_int;
type FlushFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type TestAttachFn = unsafe extern "C" fn(*mut c_void, c_int);
type TestReportFn = unsafe extern "C" fn(*mut c_void, *const u8, usize);
type TestTakeReportFn = unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> c_int;

/// The functions of the library. It is loaded once per process and never unloaded.
pub struct Library {
    pub path: String,
    open: OpenFn,
    open_test: Option<OpenFn>,
    close: CloseFn,
    is_attached: IsAttachedFn,
    read: ReadFn,
    send: SendFn,
    flush: FlushFn,
    test_attach: Option<TestAttachFn>,
    test_report: Option<TestReportFn>,
    test_take_report: Option<TestTakeReportFn>,
}

/// The loaded library (a failed search is not kept: the next controller start searches again,
/// e.g. after the library was installed while the DAW runs).
static LOADED: Mutex<Option<&'static Library>> = Mutex::new(None);

/// Where the library is searched (in this order): `$DBM_NIHOST_LIBRARY` alone if it is set,
/// else the drivenbymoss package, dbm-nihost's own installation (system, `/usr/local`, a CMake
/// install to `~/.local`) and the builds in the repository of this core (development).
pub fn candidates() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os(ENV_LIBRARY).filter(|path| !path.is_empty()) {
        return vec![PathBuf::from(path)];
    }
    let mut paths = vec![
        PathBuf::from("/usr/lib/drivenbymoss/libdbm_nihost.so"),
        PathBuf::from("/usr/lib/dbm-nihost/libdbm_nihost.so"),
        PathBuf::from("/usr/local/lib/dbm-nihost/libdbm_nihost.so"),
    ];
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        paths.push(Path::new(&home).join(".local/lib/dbm-nihost/libdbm_nihost.so"));
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    for build in ["build-nihost", "support/linux/ni_host/build", "target/ci/ni_host"] {
        paths.push(repository.join(build).join("libdbm_nihost.so"));
    }
    paths
}

/// The library, loaded on the first call; the error says where it was searched or why it could
/// not be loaded.
pub fn load() -> Result<&'static Library, String> {
    let mut loaded = LOADED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(library) = *loaded {
        return Ok(library);
    }
    let candidates = candidates();
    let Some(path) = candidates.iter().find(|path| path.is_file()) else {
        let searched: Vec<String> = candidates.iter().map(|path| path.display().to_string()).collect();
        return Err(format!("not found ({})", searched.join(", ")));
    };
    let library: &'static Library = Box::leak(Box::new(open_library(path)?));
    *loaded = Some(library);
    Ok(library)
}

fn last_dl_error() -> String {
    // SAFETY: dlerror returns null or a NUL terminated string of the C library
    let error = unsafe { dlerror() };
    if error.is_null() {
        "unknown error".to_string()
    } else {
        unsafe { CStr::from_ptr(error) }.to_string_lossy().into_owned()
    }
}

fn open_library(path: &Path) -> Result<Library, String> {
    let name = path.display().to_string();
    let c_path = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| format!("{name}: invalid path"))?;
    // SAFETY: a valid NUL terminated path; the handle is never closed
    let handle = unsafe { dlopen(c_path.as_ptr(), RTLD_NOW) };
    if handle.is_null() {
        return Err(format!("{name}: {}", last_dl_error()));
    }
    let symbol = |symbol: &str| -> *mut c_void {
        let c_symbol = CString::new(symbol).unwrap_or_default();
        // SAFETY: a handle of dlopen and a NUL terminated name
        unsafe { dlsym(handle, c_symbol.as_ptr()) }
    };
    macro_rules! function {
        ($symbol:literal, $type:ty) => {{
            let pointer = symbol($symbol);
            if pointer.is_null() {
                None
            } else {
                // SAFETY: the symbol of dbm_nihost.h has this signature
                Some(unsafe { std::mem::transmute::<*mut c_void, $type>(pointer) })
            }
        }};
    }
    macro_rules! required {
        ($symbol:literal, $type:ty) => {
            function!($symbol, $type).ok_or_else(|| format!("{name}: no function {}", $symbol))?
        };
    }
    let api_version: ApiVersionFn = required!("dbm_nihost_api_version", ApiVersionFn);
    // SAFETY: the function takes no arguments
    let version = unsafe { api_version() };
    if version != API_VERSION {
        return Err(format!("{name}: API version {version}, expected {API_VERSION}"));
    }
    Ok(Library {
        path: name.clone(),
        open: required!("dbm_nihost_open", OpenFn),
        open_test: function!("dbm_nihost_open_test", OpenFn),
        close: required!("dbm_nihost_close", CloseFn),
        is_attached: required!("dbm_nihost_is_attached", IsAttachedFn),
        read: required!("dbm_nihost_read", ReadFn),
        send: required!("dbm_nihost_send", SendFn),
        flush: required!("dbm_nihost_flush", FlushFn),
        test_attach: function!("dbm_nihost_test_attach", TestAttachFn),
        test_report: function!("dbm_nihost_test_report", TestReportFn),
        test_take_report: function!("dbm_nihost_test_take_report", TestTakeReportFn),
    })
}

/// A device driven by the library. The library's thread looks for the hidraw node, opens it
/// and translates its reports; this reads the queued MIDI without waiting (on the core's tick),
/// sends the core's MIDI and writes the changed LEDs and displays ([`DeviceIo`]). Closed when
/// dropped.
pub struct NihostDevice {
    library: &'static Library,
    handle: *mut c_void,
    buffer: std::cell::RefCell<Vec<u8>>,
}

impl NihostDevice {
    /// Start driving the device (it need not be connected yet).
    pub fn open(library: &'static Library, device: HidDevice) -> Result<Self, String> {
        // SAFETY: a function of the library, any kind is allowed (null for an unknown one)
        let handle = unsafe { (library.open)(device.kind) };
        Self::new(library, handle, device)
    }

    /// A test device of the library ([`NihostDevice::test_attach`] and the other `test_*`
    /// stand in for the hardware).
    pub fn open_test(library: &'static Library, device: HidDevice) -> Result<Self, String> {
        let open = library.open_test.ok_or("the library has no test devices")?;
        // SAFETY: as open
        let handle = unsafe { open(device.kind) };
        Self::new(library, handle, device)
    }

    fn new(library: &'static Library, handle: *mut c_void, device: HidDevice) -> Result<Self, String> {
        if handle.is_null() {
            return Err(format!("{}: no device of kind {}", library.path, device.kind));
        }
        Ok(NihostDevice { library, handle, buffer: std::cell::RefCell::new(vec![0; READ_BUFFER_BYTES]) })
    }

    /// True while the device is attached (its hidraw node is open).
    pub fn is_attached(&self) -> bool {
        // SAFETY: a handle of open, not closed
        unsafe { (self.library.is_attached)(self.handle) == 1 }
    }

    /// Test devices: attach (the hello comes) or unplug ("gone") the device.
    pub fn test_attach(&self, attached: bool) {
        if let Some(attach) = self.library.test_attach {
            // SAFETY: as is_attached
            unsafe { attach(self.handle, c_int::from(attached)) };
        }
    }

    /// Test devices: an input report of the device.
    pub fn test_report(&self, report: &[u8]) {
        if let Some(input) = self.library.test_report {
            // SAFETY: as is_attached; the report is readable for its length
            unsafe { input(self.handle, report.as_ptr(), report.len()) };
        }
    }

    /// Test devices: the output reports written by the flushes, the oldest first.
    pub fn test_take_reports(&self) -> Vec<Vec<u8>> {
        let mut reports = Vec::new();
        let Some(take) = self.library.test_take_report else {
            return reports;
        };
        let mut buffer = vec![0u8; 4096];
        loop {
            // SAFETY: as is_attached; the buffer is writable for its length
            let length = unsafe { take(self.handle, buffer.as_mut_ptr(), buffer.len()) };
            if length <= 0 {
                return reports;
            }
            reports.push(buffer[..length as usize].to_vec());
        }
    }
}

impl Drop for NihostDevice {
    fn drop(&mut self) {
        // SAFETY: the handle of open; nothing else uses it (the device is not shared between
        // threads, the library's own thread is stopped by close)
        unsafe { (self.library.close)(self.handle) };
    }
}

/// The messages of a read: each with 2 bytes length (little endian) before it.
pub fn split_messages(data: &[u8], out: &mut Vec<Vec<u8>>) {
    let mut i = 0;
    while i + 2 <= data.len() {
        let length = usize::from(data[i]) | (usize::from(data[i + 1]) << 8);
        let end = (i + 2 + length).min(data.len());
        out.push(data[i + 2..end].to_vec());
        i = end;
    }
}

impl DeviceIo for NihostDevice {
    fn read(&self, out: &mut Vec<Vec<u8>>) {
        let mut buffer = self.buffer.borrow_mut();
        // Several reads if more is queued than fits
        for _ in 0..64 {
            // SAFETY: as is_attached; the buffer is writable for its length; no waiting
            let length = unsafe { (self.library.read)(self.handle, buffer.as_mut_ptr(), buffer.len(), 0) };
            if length <= 0 {
                return;
            }
            split_messages(&buffer[..length as usize], out);
        }
    }

    fn send(&self, data: &[u8]) {
        if !data.is_empty() {
            // SAFETY: as is_attached; the data is readable for its length
            unsafe { (self.library.send)(self.handle, data.as_ptr(), data.len()) };
        }
    }

    fn flush(&self) -> i32 {
        // SAFETY: as is_attached
        unsafe { (self.library.flush)(self.handle) }
    }
}

/// The checks of the real system (sysfs, `/dev`, `/proc`).
pub struct LinuxProbe;

impl SystemProbe for LinuxProbe {
    fn library(&self) -> Result<String, String> {
        load().map(|library| library.path.clone())
    }

    fn daemon_drives(&self, device: HidDevice) -> bool {
        daemon_drives(Path::new("/proc"), device)
    }

    fn device_state(&self, device: HidDevice) -> DeviceState {
        match find_hidraw(Path::new("/sys/class/hidraw"), device.vendor_id, device.product_id) {
            None => DeviceState::Absent,
            Some(node) => lock_state(&format!("/dev/{node}")),
        }
    }
}

/// The hidraw node (e.g. "hidraw7") of the device: the one whose uevent has its `HID_ID`
/// (`HID_ID=0003:000017CC:00001500`: bus, vendor, product), as the library finds it.
pub fn find_hidraw(sysfs: &Path, vendor_id: u16, product_id: u16) -> Option<String> {
    let mut nodes: Vec<String> = std::fs::read_dir(sysfs).ok()?.filter_map(|entry| entry.ok()).map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
    nodes.sort();
    nodes.into_iter().find(|node| {
        let Ok(uevent) = std::fs::read_to_string(sysfs.join(node).join("device/uevent")) else {
            return false;
        };
        uevent.lines().filter_map(|line| line.strip_prefix("HID_ID=")).any(|id| {
            let mut parts = id.split(':').skip(1).map(|part| u32::from_str_radix(part.trim(), 16));
            matches!((parts.next(), parts.next()), (Some(Ok(vendor)), Some(Ok(product))) if vendor == u32::from(vendor_id) && product == u32::from(product_id))
        })
    })
}

/// Whether the node can be driven: free, held by another process (its exclusive lock), or not
/// accessible (the udev rule is missing).
fn lock_state(path: &str) -> DeviceState {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let file = match std::fs::OpenOptions::new().read(true).write(true).custom_flags(O_NONBLOCK).open(path) {
        Ok(file) => file,
        Err(error) => return DeviceState::NoAccess(path.to_string(), error.to_string()),
    };
    // SAFETY: an open file descriptor
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
        return DeviceState::Locked(path.to_string());
    }
    // SAFETY: as above; closing the file would release it as well
    unsafe { flock(file.as_raw_fd(), LOCK_UN) };
    DeviceState::Free(path.to_string())
}

/// True if a dbm-nihost process runs which drives the device: all devices without `--mk2` /
/// `--jam`, else the ones given.
pub fn daemon_drives(proc: &Path, device: HidDevice) -> bool {
    let Ok(entries) = std::fs::read_dir(proc) else {
        return false;
    };
    entries.filter_map(|entry| entry.ok()).filter(|entry| entry.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())).any(|entry| {
        let is_daemon = std::fs::read_to_string(entry.path().join("comm")).is_ok_and(|comm| comm.trim_end() == "dbm-nihost");
        if !is_daemon {
            return false;
        }
        let arguments = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
        let arguments: Vec<&[u8]> = arguments.split(|b| *b == 0).collect();
        let has = |flag: &str| arguments.contains(&flag.as_bytes());
        let (mk2, jam) = (has("--mk2"), has("--jam"));
        (!mk2 && !jam) || (mk2 && device.kind == super::KIND_MK2) || (jam && device.kind == super::KIND_JAM)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nihost::{HELLO_JAM, MESSAGE_GONE};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm-nihost-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn messages_of_a_read() {
        let mut out = Vec::new();
        split_messages(&[3, 0, 0x90, 36, 127, 1, 0, 0xFE, 2, 0], &mut out);
        assert_eq!(out, [vec![0x90, 36, 127], vec![0xFE], vec![]]);
    }

    #[test]
    fn the_hidraw_node_of_a_device() {
        let sysfs = temp_dir("sysfs");
        for (node, id) in [("hidraw0", "0003:00001E71:00002007"), ("hidraw7", "0003:000017CC:00001500")] {
            std::fs::create_dir_all(sysfs.join(node).join("device")).unwrap();
            std::fs::write(sysfs.join(node).join("device/uevent"), format!("DRIVER=hid-generic\nHID_ID={id}\nHID_NAME=x\n")).unwrap();
        }
        assert_eq!(find_hidraw(&sysfs, 0x17CC, 0x1500).as_deref(), Some("hidraw7"));
        assert_eq!(find_hidraw(&sysfs, 0x17CC, 0x1140), None);
        assert_eq!(find_hidraw(&sysfs.join("missing"), 0x17CC, 0x1500), None);
        let _ = std::fs::remove_dir_all(&sysfs);
    }

    #[test]
    fn a_locked_node_is_in_use() {
        use std::os::fd::AsRawFd;
        let dir = temp_dir("lock");
        let node = dir.join("hidraw9");
        std::fs::write(&node, b"").unwrap();
        let path = node.to_string_lossy().into_owned();
        assert_eq!(lock_state(&path), DeviceState::Free(path.clone()));
        let holder = std::fs::OpenOptions::new().read(true).write(true).open(&node).unwrap();
        assert_eq!(unsafe { flock(holder.as_raw_fd(), LOCK_EX | LOCK_NB) }, 0);
        assert_eq!(lock_state(&path), DeviceState::Locked(path.clone()));
        drop(holder);
        assert_eq!(lock_state(&path), DeviceState::Free(path.clone()));
        assert!(matches!(lock_state(&dir.join("missing").to_string_lossy()), DeviceState::NoAccess(..)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_daemon_and_its_devices() {
        let proc = temp_dir("proc");
        assert!(!daemon_drives(&proc, HidDevice::JAM));
        let process = |pid: &str, comm: &str, arguments: &[&str]| {
            std::fs::create_dir_all(proc.join(pid)).unwrap();
            std::fs::write(proc.join(pid).join("comm"), format!("{comm}\n")).unwrap();
            std::fs::write(proc.join(pid).join("cmdline"), arguments.join("\0") + "\0").unwrap();
        };
        process("12", "bash", &["bash"]);
        process("self", "dbm-nihost", &["dbm-nihost"]);
        assert!(!daemon_drives(&proc, HidDevice::JAM));
        process("40", "dbm-nihost", &["/usr/bin/dbm-nihost", "--mk2", "--virmidi"]);
        assert!(daemon_drives(&proc, HidDevice::MK2));
        assert!(!daemon_drives(&proc, HidDevice::JAM));
        process("41", "dbm-nihost", &["dbm-nihost", "--virmidi"]);
        assert!(daemon_drives(&proc, HidDevice::JAM));
        let _ = std::fs::remove_dir_all(&proc);
    }

    /// The real library with a test device, if one is installed or built (else nothing to do):
    /// the hello on attach, the template MIDI of an input report, LEDs from the core's MIDI,
    /// "gone" on unplug.
    #[test]
    fn the_library_with_a_test_device() {
        let Ok(library) = load() else {
            eprintln!("libdbm_nihost not found, skipped");
            return;
        };
        let Ok(device) = NihostDevice::open_test(library, HidDevice::JAM) else {
            eprintln!("{}: no test devices, skipped", library.path);
            return;
        };
        let mut input = Vec::new();
        device.read(&mut input);
        assert!(input.is_empty(), "{input:?}");
        assert!(!device.is_attached());
        device.test_attach(true);
        assert!(device.is_attached());
        device.read(&mut input);
        assert!(input.contains(&HELLO_JAM.to_vec()), "{input:?}");
        // Some LED: the library writes reports once attached
        device.send(&[0xB0, 0x3C, 0x7F]);
        assert!(device.flush() > 0);
        assert!(!device.test_take_reports().is_empty());
        input.clear();
        device.test_attach(false);
        device.read(&mut input);
        assert!(input.contains(&MESSAGE_GONE.to_vec()), "{input:?}");
    }
}
