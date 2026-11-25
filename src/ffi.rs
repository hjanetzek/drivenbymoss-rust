// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The C interface for hosts (the Reaper extension, Python, Java, ...). The `dbm_*` functions
//! replace the static methods of the former Java `Controller` class; [`DbmCallbacks`] replaces
//! the JNI `native` methods of `MainApp`. The header `include/drivenbymoss.h` is generated from
//! this file by cbindgen (see `build.rs`).
//!
//! Each instance ([`DbmApp`]) serializes its calls with its own mutex. Unless the instance is
//! created with [`DBM_FLAG_HOST_TICK`], a background thread calls [`App::tick`] every 30ms to
//! execute scheduled tasks and to update the controllers (the former flush timer).

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::app::App;
use crate::backend::Backend;
use std::cell::RefCell;

use crate::protocol::bin::{self, Writer};
use crate::protocol::{Arg, Cmd, Legacy};

/// Receives the commands of an instance in the binary form ([`dbm_set_command_callback`]): a
/// buffer of records (`include/dbm_protocol.h`), valid during the call.
pub type DbmCommands = extern "C" fn(user: *mut c_void, buffer: *const u8, length: usize);

/// Receives the USB bulk writes of an instance ([`dbm_set_usb_callback`]), e.g. the frames of
/// the Push 2/3 display. `instance` is the index of the controller instance (0 for
/// [`dbm_create_controller`]), `device_index` the index of the USB device of the controller (see
/// [`dbm_list_usb_devices`]); `vendor_id`, `product_id` and `interface_number` are those of the
/// device's matcher, `endpoint` the endpoint address (e.g. 0x01, OUT). `data` (`length` bytes)
/// is the complete byte stream of the write and is only valid during the call. Returns non-zero
/// if the data was written or queued, 0 if not (no device, error). It runs with the instance
/// locked: it must not block for long (queue the data and write it on another thread, drop data
/// rather than stall) and must not call into the same instance.
pub type DbmUsbBulkOut = extern "C" fn(user: *mut c_void, instance: c_int, device_index: c_int, vendor_id: u16, product_id: u16, interface_number: u8, endpoint: u8, data: *const u8, length: usize) -> c_int;

/// The version of the USB interface ([`dbm_set_usb_callback`], [`DbmUsbBulkOut`]).
pub const DBM_USB_VERSION: u32 = 1;

/// Receives the messages of the simulator side channel of an instance
/// ([`dbm_set_simulator_callback`]), e.g. the encoder state of the Push 2/3 for the Push
/// simulator (a `DBMV` message, see `doc/HOST_BRIDGE_GUIDE.md`). `instance` is the index of the
/// controller instance, `data` (`length` bytes) the complete message, the exact bytes to write
/// to the simulator's stream (between two USB writes, never inside one), only valid during the
/// call. Never write it to a real device. Returns non-zero if the data was written or queued.
/// It runs with the instance locked, as [`DbmUsbBulkOut`].
pub type DbmSimulatorData = extern "C" fn(user: *mut c_void, instance: c_int, data: *const u8, length: usize) -> c_int;

/// The version of the simulator side channel ([`dbm_set_simulator_callback`],
/// [`DbmSimulatorData`]).
pub const DBM_SIMULATOR_VERSION: u32 = 1;

/// Adds one MIDI device (ID and name) to a list, see `get_midi_inputs`.
pub type DbmAddDevice = extern "C" fn(list: *mut c_void, id: c_int, name: *const c_char);

/// The functions provided by the host (e.g. the Reaper extension). `user` is passed as the
/// first argument to each function. Function pointers may be null: the call is ignored (the
/// `open_*` functions then report failure, the `get_midi_*` functions an empty list).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DbmCallbacks {
    pub user: *mut c_void,
    pub process_no_arg: Option<extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char)>,
    pub process_string_arg: Option<extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, value: *const c_char)>,
    pub process_string_args: Option<extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, values: *const *const c_char, count: c_int)>,
    pub process_int_arg: Option<extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, value: c_int)>,
    pub process_double_arg: Option<extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, value: f64)>,
    pub delay_updates: Option<extern "C" fn(user: *mut c_void, processor: *const c_char)>,
    pub enable_updates: Option<extern "C" fn(user: *mut c_void, processor: *const c_char, enable: c_int)>,
    pub process_midi_arg: Option<extern "C" fn(user: *mut c_void, device_id: c_int, status: c_int, data1: c_int, data2: c_int)>,
    pub get_midi_inputs: Option<extern "C" fn(user: *mut c_void, list: *mut c_void, add: DbmAddDevice)>,
    pub get_midi_outputs: Option<extern "C" fn(user: *mut c_void, list: *mut c_void, add: DbmAddDevice)>,
    pub open_midi_input: Option<extern "C" fn(user: *mut c_void, device_id: c_int) -> c_int>,
    pub open_midi_output: Option<extern "C" fn(user: *mut c_void, device_id: c_int) -> c_int>,
    pub close_midi_input: Option<extern "C" fn(user: *mut c_void, device_id: c_int)>,
    pub close_midi_output: Option<extern "C" fn(user: *mut c_void, device_id: c_int)>,
    pub send_midi_data: Option<extern "C" fn(user: *mut c_void, device_id: c_int, data: *const u8, length: c_int)>,
    pub set_note_input_filters: Option<extern "C" fn(user: *mut c_void, device_id: c_int, note_input_index: c_int, filters: *const *const c_char, count: c_int)>,
    pub set_note_input_key_translation_table: Option<extern "C" fn(user: *mut c_void, device_id: c_int, note_input_index: c_int, table: *const c_int, count: c_int)>,
    pub set_note_input_velocity_translation_table: Option<extern "C" fn(user: *mut c_void, device_id: c_int, note_input_index: c_int, table: *const c_int, count: c_int)>,
    /// A log message of the library. If set, the library does not write its log to stderr (only
    /// the message of a caught panic).
    pub log: Option<extern "C" fn(user: *mut c_void, message: *const c_char)>,
}

/// Converts a Rust string to a C string, removing interior NUL characters.
fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

/// Converts a C string to a Rust string. A null pointer gives an empty string.
///
/// # Safety
/// The pointer must be null or point to a valid NUL terminated string.
unsafe fn from_c(text: *const c_char) -> String {
    if text.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    }
}

extern "C" fn add_device_to_list(list: *mut c_void, id: c_int, name: *const c_char) {
    // SAFETY: `list` is the vector passed by `collect_devices` and only used during the call.
    let list = unsafe { &mut *(list as *mut Vec<(i32, String)>) };
    list.push((id, unsafe { from_c(name) }));
}

/// The binary command form of an instance ([`dbm_set_command_callback`]).
struct BinaryCommands {
    callback: DbmCommands,
    user: *mut c_void,
    writer: Writer,
    /// The command IDs the host handles (word i bit j = ID 0x8000 + 64*i + j); empty: all.
    handled: Vec<u64>,
    /// The IDs logged as not handled (logged once per ID).
    warned: std::collections::HashSet<u16>,
}

/// The backend which forwards all calls to the C++ callbacks.
pub struct FfiBackend {
    callbacks: DbmCallbacks,
    binary: RefCell<Option<BinaryCommands>>,
    /// The command features of the binary form, derived from the handled commands (None: text).
    command_features: std::cell::Cell<Option<u32>>,
    /// The receiver of the USB writes ([`dbm_set_usb_callback`]) and its user pointer.
    usb: std::cell::Cell<Option<(DbmUsbBulkOut, *mut c_void)>>,
    /// The receiver of the simulator data ([`dbm_set_simulator_callback`]) and its user pointer.
    simulator: std::cell::Cell<Option<(DbmSimulatorData, *mut c_void)>>,
}

impl FfiBackend {
    fn new(callbacks: DbmCallbacks) -> Self {
        FfiBackend { callbacks, binary: RefCell::new(None), command_features: std::cell::Cell::new(None), usb: std::cell::Cell::new(None), simulator: std::cell::Cell::new(None) }
    }

    /// Deliver the buffered commands of the binary form (before any other callback and at the
    /// end of every `dbm_*` call, so that the order of commands and other callbacks is kept).
    fn flush(&self) {
        let mut binary = self.binary.borrow_mut();
        let Some(b) = binary.as_mut() else {
            return;
        };
        if b.writer.is_empty() {
            return;
        }
        let bytes = b.writer.as_bytes();
        (b.callback)(b.user, bytes.as_ptr(), bytes.len());
        b.writer.clear();
    }

    /// Log a command which the host declared as not handled, once per ID (phase 1 of the
    /// handled-commands bitset, TYPED_PROTOCOL_PLAN §2.5).
    fn check_handled(&self, b: &mut BinaryCommands, cmd: &Cmd) -> Option<String> {
        let id = cmd.id() as u16;
        if crate::daw::context::is_handled(&b.handled, id) || !b.warned.insert(id) {
            return None;
        }
        Some(format!("Command not handled by the host: {}", crate::protocol::bin::command_line(cmd).replace('\t', " ")))
    }

    fn collect_devices(&self, get: Option<extern "C" fn(*mut c_void, *mut c_void, DbmAddDevice)>) -> Vec<(i32, String)> {
        let mut list: Vec<(i32, String)> = Vec::new();
        let Some(get) = get else {
            return list;
        };
        get(self.callbacks.user, &mut list as *mut Vec<(i32, String)> as *mut c_void, add_device_to_list);
        list
    }
}

impl Backend for FfiBackend {
    fn command_features(&self) -> Option<u32> {
        self.command_features.get()
    }

    fn handles_command(&self, id: crate::protocol::CmdId) -> Option<bool> {
        self.binary.borrow().as_ref().map(|b| crate::daw::context::is_handled(&b.handled, id as u16))
    }

    /// Deliver the legacy form of the command (`process_no_arg`, `process_int_arg`, …), as before
    /// the command table: the text is byte-identical.
    fn send_command(&self, cmd: &Cmd) {
        let warning = {
            let mut binary = self.binary.borrow_mut();
            match binary.as_mut() {
                Some(b) => {
                    let warning = self.check_handled(b, cmd);
                    cmd.encode(&mut b.writer);
                    warning
                }
                None => None,
            }
        };
        if let Some(warning) = warning {
            self.log(&warning);
        }
        if self.binary.borrow().is_some() {
            return;
        }
        let Legacy { processor, command, arg } = cmd.legacy();
        let user = self.callbacks.user;
        let p = c_string(processor.id());
        match arg {
            Arg::None => {
                let c = c_string(&command);
                if let Some(f) = self.callbacks.process_no_arg {
                    f(user, p.as_ptr(), c.as_ptr());
                }
            }
            Arg::Str(value) => {
                let (c, v) = (c_string(&command), c_string(&value));
                if let Some(f) = self.callbacks.process_string_arg {
                    f(user, p.as_ptr(), c.as_ptr(), v.as_ptr());
                }
            }
            Arg::Strs(values) => {
                let c = c_string(&command);
                let strings: Vec<CString> = values.iter().map(|v| c_string(v)).collect();
                let pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
                if let Some(f) = self.callbacks.process_string_args {
                    f(user, p.as_ptr(), c.as_ptr(), pointers.as_ptr(), pointers.len() as c_int);
                }
            }
            Arg::Int(value) => {
                let c = c_string(&command);
                if let Some(f) = self.callbacks.process_int_arg {
                    f(user, p.as_ptr(), c.as_ptr(), value);
                }
            }
            Arg::Double(value) => {
                let c = c_string(&command);
                if let Some(f) = self.callbacks.process_double_arg {
                    f(user, p.as_ptr(), c.as_ptr(), value);
                }
            }
            Arg::Delay => {
                if let Some(f) = self.callbacks.delay_updates {
                    f(user, p.as_ptr());
                }
            }
            Arg::Enable(enable) => {
                if let Some(f) = self.callbacks.enable_updates {
                    f(user, p.as_ptr(), enable as c_int);
                }
            }
        }
    }

    fn process_midi_arg(&self, device_id: i32, status: i32, data1: i32, data2: i32) {
        self.flush();
        if let Some(f) = self.callbacks.process_midi_arg {
            f(self.callbacks.user, device_id, status, data1, data2);
        }
    }

    fn midi_inputs(&self) -> Vec<(i32, String)> {
        self.flush();
        self.collect_devices(self.callbacks.get_midi_inputs)
    }

    fn midi_outputs(&self) -> Vec<(i32, String)> {
        self.flush();
        self.collect_devices(self.callbacks.get_midi_outputs)
    }

    fn open_midi_input(&self, device_id: i32) -> bool {
        self.flush();
        self.callbacks.open_midi_input.is_some_and(|f| f(self.callbacks.user, device_id) != 0)
    }

    fn open_midi_output(&self, device_id: i32) -> bool {
        self.flush();
        self.callbacks.open_midi_output.is_some_and(|f| f(self.callbacks.user, device_id) != 0)
    }

    fn close_midi_input(&self, device_id: i32) {
        self.flush();
        if let Some(f) = self.callbacks.close_midi_input {
            f(self.callbacks.user, device_id);
        }
    }

    fn close_midi_output(&self, device_id: i32) {
        self.flush();
        if let Some(f) = self.callbacks.close_midi_output {
            f(self.callbacks.user, device_id);
        }
    }

    fn send_midi_data(&self, device_id: i32, data: &[u8]) {
        self.flush();
        if let Some(f) = self.callbacks.send_midi_data {
            f(self.callbacks.user, device_id, data.as_ptr(), data.len() as c_int);
        }
    }

    /// A write without a resolved device (the application resolves the writes of its instances,
    /// see [`crate::backend::UsbInstanceBackend`]): instance -1, the IDs 0.
    fn usb_bulk_out(&self, device_index: i32, endpoint: u8, data: &[u8]) -> bool {
        let target = crate::backend::UsbTarget { instance: -1, device_index, vendor_id: 0, product_id: 0, interface: 0, endpoint };
        self.usb_bulk_out_to(&target, data)
    }

    fn usb_bulk_out_to(&self, target: &crate::backend::UsbTarget, data: &[u8]) -> bool {
        let Some((callback, user)) = self.usb.get() else {
            return false;
        };
        self.flush();
        callback(user, target.instance, target.device_index, target.vendor_id, target.product_id, target.interface, target.endpoint, data.as_ptr(), data.len()) != 0
    }

    fn simulator_data_enabled(&self) -> bool {
        self.simulator.get().is_some()
    }

    fn simulator_data_out_to(&self, instance: i32, data: &[u8]) -> bool {
        let Some((callback, user)) = self.simulator.get() else {
            return false;
        };
        self.flush();
        callback(user, instance, data.as_ptr(), data.len()) != 0
    }

    fn set_note_input_filters(&self, device_id: i32, note_input_index: i32, filters: &[String]) {
        self.flush();
        let strings: Vec<CString> = filters.iter().map(|f| c_string(f)).collect();
        let pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        if let Some(f) = self.callbacks.set_note_input_filters {
            f(self.callbacks.user, device_id, note_input_index, pointers.as_ptr(), pointers.len() as c_int);
        }
    }

    fn set_note_input_key_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        self.flush();
        if let Some(f) = self.callbacks.set_note_input_key_translation_table {
            f(self.callbacks.user, device_id, note_input_index, table.as_ptr(), table.len() as c_int);
        }
    }

    fn set_note_input_velocity_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        self.flush();
        if let Some(f) = self.callbacks.set_note_input_velocity_translation_table {
            f(self.callbacks.user, device_id, note_input_index, table.as_ptr(), table.len() as c_int);
        }
    }

    fn log(&self, message: &str) {
        self.flush();
        let m = c_string(message);
        if let Some(f) = self.callbacks.log {
            f(self.callbacks.user, m.as_ptr());
        }
    }

    fn handles_log(&self) -> bool {
        self.callbacks.log.is_some()
    }
}

/// Holds the application. The application uses `Rc` internally and is therefore not `Send`; it
/// is only ever accessed while holding the mutex of its handle and none of its `Rc`s escape from
/// it, which makes moving it between the calling threads sound.
struct AppHolder(Option<App>, Option<Rc<FfiBackend>>);

// SAFETY: see the documentation of `AppHolder`.
unsafe impl Send for AppHolder {}

/// The interval of the flush timer in milliseconds (same as in the Java version).
const DEVICE_UPDATE_RATE: u64 = 30;

/// Flag for [`dbm_create`]: do not start the internal timer thread. The host must call
/// [`dbm_tick`] regularly (every 30ms is recommended) instead.
pub const DBM_FLAG_HOST_TICK: u32 = 1;

/// Flag for [`dbm_create_controller`]: the core may drive the controller's device itself
/// instead of through the host's MIDI ports: the NI Maschine Mk2 and JAM on Linux over
/// `libdbm_nihost.so`, when it is installed and no other process (dbm-nihost) drives the device
/// (the environment variable `DBM_MASCHINE_TRANSPORT=hid|midi|auto` overrides the choice). The
/// device's MIDI then never reaches the host's ports; the core plays the notes of its note
/// inputs into the host's note input with `process_midi_arg`. Call [`dbm_poll_devices`] more
/// often than [`dbm_tick`] for less delay of the pads. Without the flag: the host's ports.
pub const DBM_FLAG_DEVICE_TRANSPORT: u32 = 2;

/// The shared state of a handle (also referenced by its timer thread).
struct Inner {
    app: Mutex<AppHolder>,
    timer_running: AtomicBool,
}

impl Inner {
    /// Run a function with the application. Panics are caught to never unwind into the host.
    fn with_app<R>(&self, default: R, f: impl FnOnce(&mut App) -> R) -> R {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut guard = match self.app.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let result = guard.0.as_mut().map(f);
            // The end of the call: deliver the buffered commands of the binary form
            if let Some(ffi) = &guard.1 {
                ffi.flush();
            }
            result
        }));
        match result {
            Ok(Some(value)) => value,
            Ok(None) => default,
            Err(_) => {
                eprintln!("[DrivenByMoss] Panic in the Rust library caught.");
                default
            }
        }
    }
}

/// An instance of DrivenByMoss (opaque for the host). Created with [`dbm_create`], released
/// with [`dbm_destroy`]. Several instances can be used at the same time.
pub struct DbmApp {
    inner: Arc<Inner>,
    flags: u32,
    timer: Mutex<Option<JoinHandle<()>>>,
}

impl DbmApp {
    fn start_timer(&self) {
        if self.flags & DBM_FLAG_HOST_TICK != 0 || self.inner.timer_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let inner = self.inner.clone();
        let handle = std::thread::spawn(move || {
            while inner.timer_running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(DEVICE_UPDATE_RATE));
                inner.with_app((), |app| app.tick());
            }
        });
        if let Ok(mut timer) = self.timer.lock() {
            *timer = Some(handle);
        }
    }

    fn stop_timer(&self) {
        self.inner.timer_running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.timer.lock().ok().and_then(|mut timer| timer.take()) {
            let _ = handle.join();
        }
    }
}

/// Get the application of a handle.
///
/// # Safety
/// `app` must be null or a handle returned by [`dbm_create`] which was not destroyed.
unsafe fn handle<'a>(app: *const DbmApp) -> Option<&'a DbmApp> {
    unsafe { app.as_ref() }
}

/// Run a function with the application of a handle (nothing happens for a null handle).
///
/// # Safety
/// See [`handle`].
unsafe fn with_handle<R>(app: *const DbmApp, default: R, f: impl FnOnce(&mut App) -> R) -> R {
    match unsafe { handle(app) } {
        Some(app) => app.inner.with_app(default, f),
        None => default,
    }
}

/// The version of the library as a static string (e.g. "26.1.0"). Must not be freed.
#[no_mangle]
pub extern "C" fn dbm_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Create an instance. The callback table is copied; `callbacks->user` must stay valid until
/// [`dbm_destroy`]. `ini_path` is the folder of the configuration files, `app_version` the
/// version of the host application (e.g. "7.22/linux-x86_64"; the first two numbers count, 1.0 if
/// there are none). `flags` is a combination of the
/// `DBM_FLAG_*` values. Returns null if `callbacks` is null.
///
/// # Safety
/// `callbacks` must point to a valid callback table; the strings must be valid NUL terminated
/// strings (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_create(callbacks: *const DbmCallbacks, ini_path: *const c_char, app_version: *const c_char, flags: u32) -> *mut DbmApp {
    if callbacks.is_null() {
        return std::ptr::null_mut();
    }
    let callbacks = unsafe { *callbacks };
    let ini_path = unsafe { from_c(ini_path) };
    let app_version = unsafe { from_c(app_version) };
    new_handle(flags, || {
        let (major, minor) = parse_version(&app_version);
        let ffi = Rc::new(FfiBackend::new(callbacks));
        let backend: Rc<dyn Backend> = ffi.clone();
        Some((App::new(backend, &ini_path, major, minor), ffi))
    })
}

/// Create an instance which runs exactly one controller, for hosts which declare each
/// controller themselves and assign its MIDI ports (Bitwig). `controller_id` is the UUID of the
/// controller (see [`dbm_list_controllers`]); its MIDI input and output are the device IDs 0.
/// `config_path` is the folder of the controller's configuration file
/// (`DrivenByMoss-<hardware model>.config`, spaces and slashes as `-`). There is no main
/// configuration and no port discovery; `get_midi_inputs`/`get_midi_outputs` are not used.
/// Otherwise like [`dbm_create`]. Returns null if `callbacks` is null or the controller is not
/// supported.
///
/// # Safety
/// See [`dbm_create`].
#[no_mangle]
pub unsafe extern "C" fn dbm_create_controller(callbacks: *const DbmCallbacks, controller_id: *const c_char, config_path: *const c_char, app_version: *const c_char, flags: u32) -> *mut DbmApp {
    if callbacks.is_null() {
        return std::ptr::null_mut();
    }
    let callbacks = unsafe { *callbacks };
    let controller_id = unsafe { from_c(controller_id) };
    let config_path = unsafe { from_c(config_path) };
    let app_version = unsafe { from_c(app_version) };
    new_handle(flags, || {
        let ffi = Rc::new(FfiBackend::new(callbacks));
        let backend: Rc<dyn Backend> = ffi.clone();
        App::new_single(backend, &config_path, &controller_id, &app_version).map(|mut app| {
            if flags & DBM_FLAG_DEVICE_TRANSPORT != 0 {
                app.enable_device_transport(Rc::new(crate::nihost::SystemProvider));
            }
            (app, ffi)
        })
    })
}

/// Create the application with the given function and wrap it in a handle. Returns null if the
/// function returns `None` or panics.
fn new_handle(flags: u32, create: impl FnOnce() -> Option<(App, Rc<FfiBackend>)> + std::panic::UnwindSafe) -> *mut DbmApp {
    let (app, ffi) = match std::panic::catch_unwind(create) {
        Ok(Some(created)) => created,
        Ok(None) => return std::ptr::null_mut(),
        Err(_) => {
            eprintln!("[DrivenByMoss] Panic in the Rust library caught.");
            return std::ptr::null_mut();
        }
    };
    let inner = Arc::new(Inner { app: Mutex::new(AppHolder(Some(app), Some(ffi))), timer_running: AtomicBool::new(false) });
    Box::into_raw(Box::new(DbmApp { inner, flags, timer: Mutex::new(None) }))
}

/// The supported controllers as a JSON array: per controller its `uuid`, `vendor`, `model`,
/// `numMidiInPorts`, `numMidiOutPorts` and `discovery`, the port names for the automatic
/// detection per operating system (`windows`, `linux`, `mac`: lists of
/// `{"inputs": [...], "outputs": [...]}`). Release the string with [`dbm_free_string`].
#[no_mangle]
pub extern "C" fn dbm_list_controllers() -> *mut c_char {
    let json = std::panic::catch_unwind(crate::controller::catalog_json).unwrap_or_else(|_| "[]".to_string());
    c_string(&json).into_raw()
}

/// The model setup of the running controller as a JSON object: the numbers of tracks, scenes,
/// sends, devices, parameters, etc. that the controller uses (keys as in the Java `ModelSetup`,
/// e.g. `numTracks`). Hosts which create all their objects up front (Bitwig) create them from
/// it. `{}` until [`dbm_start_infrastructure`] started the controller, and for an instance of
/// [`dbm_create`] it is the first running controller's. Release the string with
/// [`dbm_free_string`].
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_get_model_setup(app: *mut DbmApp) -> *mut c_char {
    let json = unsafe { with_handle(app, "{}".to_string(), |app| app.model_setup_json()) };
    c_string(&json).into_raw()
}

/// Shutdown all controllers and release the instance. The handle must not be used afterwards.
/// Must not be called from within a callback.
///
/// # Safety
/// `app` must be null or a handle returned by [`dbm_create`] which was not destroyed.
#[no_mangle]
pub unsafe extern "C" fn dbm_destroy(app: *mut DbmApp) {
    if app.is_null() {
        return;
    }
    let app = unsafe { Box::from_raw(app) };
    app.stop_timer();
    app.inner.with_app((), |app| app.shutdown());
    let mut guard = match app.inner.app.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.0 = None;
}

/// Add an available device (e.g. a plugin) with its identifier.
///
/// # Safety
/// `app` must be a valid handle (or null); the strings must be valid NUL terminated strings (or
/// null).
#[no_mangle]
pub unsafe extern "C" fn dbm_add_device(app: *mut DbmApp, name: *const c_char, identifier: *const c_char) {
    let (name, identifier) = unsafe { (from_c(name), from_c(identifier)) };
    unsafe { with_handle(app, (), |app| app.add_device(&name, &identifier)) };
}

/// Start the controller instances and, unless `DBM_FLAG_HOST_TICK` was given, the internal
/// timer thread.
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_start_infrastructure(app: *mut DbmApp) {
    let Some(handle) = (unsafe { handle(app) }) else {
        return;
    };
    handle.inner.with_app((), |app| app.start_infrastructure());
    handle.start_timer();
}

/// Execute scheduled tasks and update the controllers. Must be called regularly (every 30ms is
/// recommended) if the instance was created with `DBM_FLAG_HOST_TICK`; harmless otherwise.
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_tick(app: *mut DbmApp) {
    unsafe { with_handle(app, (), |app| app.tick()) };
}

/// Read the devices which the core drives in-process (`DBM_FLAG_DEVICE_TRANSPORT`) and handle
/// their MIDI (buttons, pads; the notes of the note inputs go to the host's note input at once),
/// without the rest of a tick; [`dbm_tick`] does it as well. A host may call it every few
/// milliseconds for less delay of the pads. Returns the number of messages, -1 if the instance
/// drives no device in-process (then there is no need to call it; the host's MIDI ports are
/// used).
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_poll_devices(app: *mut DbmApp) -> c_int {
    unsafe { with_handle(app, -1, |app| app.poll_devices()) }
}

/// Run the instance on the time of the host instead of the clock, for hosts with a simulated
/// or offline time (tests, simulations): `time_ms` is the time in milliseconds since the
/// creation of the instance. After the first call the instance never reads the clock again, the
/// host calls it before each [`dbm_tick`] with the current time. A time before the last one is
/// taken as the last one. Use it with `DBM_FLAG_HOST_TICK`.
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_set_time(app: *mut DbmApp, time_ms: u64) {
    unsafe { with_handle(app, (), |app| app.set_time_ms(time_ms)) };
}

/// Update the model with data from the DAW: lines of `<address> <value>`.
///
/// # Safety
/// `app` must be a valid handle (or null); `data` must be a valid NUL terminated string (or
/// null).
#[no_mangle]
pub unsafe extern "C" fn dbm_update_model(app: *mut DbmApp, data: *const c_char) {
    let data = unsafe { from_c(data) };
    unsafe { with_handle(app, (), |app| app.update_model(&data)) };
}

/// The protocol identity of the library: the major version (the layout of the binary form),
/// the minor version (IDs added) and the hash of the tables (`DBM_PROTOCOL_*` in
/// `dbm_protocol.h`). Null pointers are skipped.
///
/// # Safety
/// The pointers must be null or valid for writing.
#[no_mangle]
pub unsafe extern "C" fn dbm_protocol_info(major: *mut u32, minor: *mut u32, hash: *mut u64) {
    unsafe {
        if let Some(major) = major.as_mut() {
            *major = u32::from(bin::VERSION_MAJOR);
        }
        if let Some(minor) = minor.as_mut() {
            *minor = u32::from(bin::VERSION_MINOR);
        }
        if let Some(hash) = hash.as_mut() {
            *hash = crate::protocol::protocol_hash();
        }
    }
}

/// Switch the commands of the instance to the binary form: from now on they are delivered to
/// `callback` as buffers of records instead of the text callbacks (`process_no_arg`, …). The
/// commands of a call are buffered and delivered before any other callback and at the end of
/// the call. Call it before [`dbm_start_infrastructure`] (its first commands are `refresh` and
/// `bank fxtrack 0`). `handled` is an optional bitset over the command IDs the host handles
/// (word i bit j = ID 0x8000 + 64*i + j), null for all: the library logs a command the host does
/// not handle once, with its text, and the command features of the host come from it (a feature
/// is supported if the host handles at least one of its commands; `/host/features` then only
/// counts for the value operations, TYPED_PROTOCOL_PLAN §6.6). It can be called again, e.g. once
/// the model setup is known, to update the callback and the handled commands. Returns 1, or 0
/// (and keeps the previous form) if `major` differs from the library's major version or
/// `callback` is null.
///
/// # Safety
/// `app` must be a valid handle (or null); `handled` must be null or point to `handled_words`
/// readable words; `user` must stay valid until [`dbm_destroy`].
#[no_mangle]
pub unsafe extern "C" fn dbm_set_command_callback(app: *mut DbmApp, major: u32, callback: Option<extern "C" fn(user: *mut c_void, buffer: *const u8, length: usize)>, user: *mut c_void, handled: *const u64, handled_words: usize) -> c_int {
    let Some(callback) = callback else {
        return 0;
    };
    if major != u32::from(bin::VERSION_MAJOR) {
        return 0;
    }
    let handled = if handled.is_null() { Vec::new() } else { unsafe { std::slice::from_raw_parts(handled, handled_words) }.to_vec() };
    let Some(app) = (unsafe { handle(app) }) else {
        return 0;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let guard = match app.inner.app.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(ffi) = &guard.1 else {
            return 0;
        };
        ffi.flush();
        ffi.command_features.set(Some(crate::daw::context::command_features(&handled)));
        let mut binary = ffi.binary.borrow_mut();
        match binary.as_mut() {
            // Called again: the new callback and handled commands, the warnings stay
            Some(b) => {
                b.callback = callback;
                b.user = user;
                b.handled = handled;
            }
            // Commands so far went to the text callbacks
            None => *binary = Some(BinaryCommands { callback, user, writer: Writer::new(), handled, warned: Default::default() }),
        }
        1
    }));
    result.unwrap_or(0)
}

/// Set the receiver of the USB bulk writes of the instance (e.g. the frames of the Push 2/3
/// display, see [`DbmUsbBulkOut`]); a null `callback` removes it (the writes then fail and
/// are dropped). Without a receiver the controllers run without their USB devices. `version`
/// must be [`DBM_USB_VERSION`]. Call it before [`dbm_start_infrastructure`] (it can be called
/// later and again, the next write goes to the new receiver). The callback runs on the thread of
/// the call into the library or on the internal timer thread, with the instance locked (see
/// [`DbmUsbBulkOut`]). Returns 1, or 0 (and keeps the previous receiver) if `version` differs
/// or `app` is null.
///
/// # Safety
/// `app` must be a valid handle (or null); `user` must stay valid until [`dbm_destroy`] or the
/// next call.
#[no_mangle]
pub unsafe extern "C" fn dbm_set_usb_callback(app: *mut DbmApp, version: u32, callback: Option<extern "C" fn(user: *mut c_void, instance: c_int, device_index: c_int, vendor_id: u16, product_id: u16, interface_number: u8, endpoint: u8, data: *const u8, length: usize) -> c_int>, user: *mut c_void) -> c_int {
    if version != DBM_USB_VERSION {
        return 0;
    }
    let Some(app) = (unsafe { handle(app) }) else {
        return 0;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let guard = match app.inner.app.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(ffi) = &guard.1 else {
            return 0;
        };
        ffi.usb.set(callback.map(|callback| (callback, user)));
        1
    }));
    result.unwrap_or(0)
}

/// Set the receiver of the simulator side channel of the instance (see [`DbmSimulatorData`]),
/// e.g. the encoder state of the Push 2/3 for the Push simulator; a null `callback` removes it.
/// Only a host which writes the USB data to a simulator (the socket of
/// `DBM_PUSH_DISPLAY_SOCKET`) registers it: the controllers build the data only while a receiver
/// is set, and it must never go to a real device. `version` must be [`DBM_SIMULATOR_VERSION`].
/// It can be called at any time. Returns 1, or 0 (and keeps the previous receiver) if `version`
/// differs or `app` is null.
///
/// # Safety
/// `app` must be a valid handle (or null); `user` must stay valid until [`dbm_destroy`] or the
/// next call.
#[no_mangle]
pub unsafe extern "C" fn dbm_set_simulator_callback(app: *mut DbmApp, version: u32, callback: Option<extern "C" fn(user: *mut c_void, instance: c_int, data: *const u8, length: usize) -> c_int>, user: *mut c_void) -> c_int {
    if version != DBM_SIMULATOR_VERSION {
        return 0;
    }
    let Some(app) = (unsafe { handle(app) }) else {
        return 0;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let guard = match app.inner.app.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(ffi) = &guard.1 else {
            return 0;
        };
        ffi.simulator.set(callback.map(|callback| (callback, user)));
        1
    }));
    result.unwrap_or(0)
}

/// The USB devices of the controllers besides MIDI (e.g. the display of the Push 2/3), for hosts
/// which claim them up front (Bitwig definitions, udev rules): a JSON object with the UUID of
/// each controller which has USB devices as key and the list of its devices as value, each
/// `{"vendorId": 10626, "productId": 6503, "interface": 0, "endpoint": 1}` (numbers in decimal;
/// the position is the `device_index` of [`DbmUsbBulkOut`]). If `controller_id` is not null or
/// empty, only that controller is listed. Release the string with [`dbm_free_string`].
///
/// # Safety
/// `controller_id` must be a valid NUL terminated string (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_list_usb_devices(controller_id: *const c_char) -> *mut c_char {
    let controller_id = unsafe { from_c(controller_id) };
    let json = std::panic::catch_unwind(|| usb_devices_json(&controller_id)).unwrap_or_else(|_| "{}".to_string());
    c_string(&json).into_raw()
}

/// The USB devices of the controllers as JSON, see [`dbm_list_usb_devices`] (an empty
/// `controller_id`: all controllers). Also printed by `dbm-catalog --usb`.
pub fn usb_devices_json(controller_id: &str) -> String {
    let mut json = crate::json::JsonWriter::new();
    json.begin_object();
    for factory in crate::controller::all_factories() {
        let uuid = factory.info().uuid;
        let devices = factory.usb_devices();
        if devices.is_empty() || (!controller_id.is_empty() && !uuid.eq_ignore_ascii_case(controller_id)) {
            continue;
        }
        json.key(&uuid).begin_array();
        for device in devices {
            json.begin_object();
            json.key("vendorId").number(i64::from(device.vendor_id));
            json.key("productId").number(i64::from(device.product_id));
            json.key("interface").number(i64::from(device.interface));
            json.key("endpoint").number(i64::from(device.endpoint));
            json.end_object();
        }
        json.end_array();
    }
    json.end_object();
    json.finish()
}

/// Update the model with a buffer of binary records (one buffer = one report), the binary form
/// of [`dbm_update_model`]. Both can be used on the same instance.
///
/// # Safety
/// `app` must be a valid handle (or null); `buffer` must point to `length` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn dbm_update_model_bin(app: *mut DbmApp, buffer: *const u8, length: usize) {
    if buffer.is_null() {
        return;
    }
    let data = unsafe { std::slice::from_raw_parts(buffer, length) };
    unsafe { with_handle(app, (), |app| app.update_model_bin(data)) };
}

/// The text of a binary buffer, one line per record: the report line of a message
/// (`/track/3/volume 0.5`) or the legacy line of a command (`double<TAB>track<TAB>3/volume<TAB>0.5`,
/// the kinds of the recordings). For logs, recordings, tests and tools. Release it with
/// [`dbm_free_string`].
///
/// # Safety
/// `buffer` must point to `length` readable bytes (or be null).
#[no_mangle]
pub unsafe extern "C" fn dbm_messages_to_text(buffer: *const u8, length: usize) -> *mut c_char {
    let data: &[u8] = if buffer.is_null() { &[] } else { unsafe { std::slice::from_raw_parts(buffer, length) } };
    let text = std::panic::catch_unwind(|| bin::to_text(data)).unwrap_or_default();
    c_string(&text).into_raw()
}

/// The binary buffer of a text (report lines and legacy command lines, see
/// [`dbm_messages_to_text`]); lines which are not in the tables are skipped. Writes the buffer to
/// `out` if it fits into `capacity` bytes and returns its size (call it with a null `out` to get
/// the size).
///
/// # Safety
/// `text` must be a valid NUL terminated string (or null); `out` must be null or point to
/// `capacity` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn dbm_messages_from_text(text: *const c_char, out: *mut u8, capacity: usize) -> usize {
    let text = unsafe { from_c(text) };
    let Ok(writer) = std::panic::catch_unwind(|| bin::from_text(&text, &mut |_| {})) else {
        return 0;
    };
    let bytes = writer.as_bytes();
    if !out.is_null() && bytes.len() <= capacity {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
    }
    bytes.len()
}

/// Display the application window (there is no UI yet, the log is printed instead).
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_display_window(app: *mut DbmApp) {
    unsafe { with_handle(app, (), |app| app.display_window()) };
}

/// Display the project settings window (no UI yet, prints the log).
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_display_project_window(app: *mut DbmApp) {
    unsafe { with_handle(app, (), |app| app.display_window()) };
}

/// Display the parameter mapping window (no UI yet, prints the log).
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_display_parameter_window(app: *mut DbmApp) {
    unsafe { with_handle(app, (), |app| app.display_window()) };
}

/// Restart all configured controllers.
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_restart_controllers(app: *mut DbmApp) {
    unsafe { with_handle(app, (), |app| app.restart_controllers()) };
}

/// Set the default initial settings for the document/project.
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_set_default_document_settings(app: *mut DbmApp) {
    unsafe { with_handle(app, (), |app| app.set_default_document_settings()) };
}

/// Get the document settings formatted to be stored in the project. The returned string must be
/// released with [`dbm_free_string`]. Returns an empty string for a null handle.
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_get_formatted_document_settings(app: *mut DbmApp) -> *mut c_char {
    let settings = unsafe { with_handle(app, String::new(), |app| app.formatted_document_settings()) };
    c_string(&settings).into_raw()
}

/// All settings of the controller in the order of their activation, for hosts which show them in
/// their own UI (Bitwig). The settings functions (`dbm_get_settings`, `dbm_set_setting`,
/// `dbm_take_setting_changes`, `dbm_take_setting_states`) address the first running controller
/// of the instance: a host with a settings UI runs one controller per instance
/// ([`dbm_create_controller`]). One line per setting with tab separated fields: ID, scope (`global`,
/// `document`), kind (`enum`, `integer`, `boolean`, `text`, `action`, `signal`), category, label,
/// default value, current value, then the options of an enumeration, the minimum and maximum of an
/// integer or the button text of a signal. An `action` value is the ID of a host action; a
/// `signal` has no value, the host sets it (to any value) when the button is pressed.
/// Empty until [`dbm_start_infrastructure`] started the controller. Release the result with
/// [`dbm_free_string`].
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_get_settings(app: *mut DbmApp) -> *mut c_char {
    let settings = unsafe { with_handle(app, String::new(), |app| app.settings()) };
    c_string(&settings).into_raw()
}

/// Set a setting from the UI of the host. The change is not reported back by
/// [`dbm_take_setting_changes`], which only returns the changes the controller made itself.
/// A changed global setting (of this or of the controller) is written to the controller's
/// configuration file about a second later (by the tick, the changes of that second
/// together) and when the controller stops; document settings are stored by the host.
///
/// # Safety
/// `app` must be a valid handle (or null); `value` must be a valid NUL terminated string (or
/// null).
#[no_mangle]
pub unsafe extern "C" fn dbm_set_setting(app: *mut DbmApp, id: c_int, value: *const c_char) {
    let value = unsafe { from_c(value) };
    unsafe { with_handle(app, (), |app| app.set_setting(id, &value)) };
}

/// The settings which the controller changed since the last call (not the ones set with
/// [`dbm_set_setting`]), one `ID<tab>value` per line.
/// Release the result with [`dbm_free_string`].
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_take_setting_changes(app: *mut DbmApp) -> *mut c_char {
    let changes = unsafe { with_handle(app, String::new(), |app| app.take_setting_changes()) };
    c_string(&changes).into_raw()
}

/// The settings whose visibility or enabled state changed since the last call, one
/// `ID<tab>visible<tab>enabled` per line (1 or 0). The first call after the start reports the
/// states which the controller set while it was created. A setting is also hidden while it has no
/// effect on the host: all it does needs a feature or commands the host lacks (e.g. "Stop
/// automation recording on knob release" without the automation commands in the handled
/// bitset of [`dbm_set_command_callback`]; a text host and an empty bitset have all commands).
/// Release the result with [`dbm_free_string`].
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_take_setting_states(app: *mut DbmApp) -> *mut c_char {
    let states = unsafe { with_handle(app, String::new(), |app| app.take_setting_states()) };
    c_string(&states).into_raw()
}

/// The help of the controls (doc/CONTROLLER_HELP_PLAN.md): the entries which changed since the
/// last call, one JSON object per line, of the first running controller. The fields: `id` (the
/// button's or control's ID name, `grid` for the pads, the mode's knobs or strips under their
/// own ID, e.g. `strips`), `label`, `bind` (`cc`, `note`, `pitchbend`, `program` or null),
/// `channel`, `control` (-1 without a binding), `pressed`, `presses` (the number of presses so
/// far, also counting a press released between two calls), `scope` (`global`, `view`, `mode`,
/// `modifier`: what the text depends on), `members` (a row of buttons with one text: their `id`,
/// `bind`, `channel`, `control`; the row's id is `First..Last`), `short` (at most 2 lines: what the
/// control does now) and `long` (the full text, may be empty). An entry whose help went away has
/// an empty `short`. The entries `@view` and `@mode` carry the names of the active view and
/// mode in `short`. Release the result with [`dbm_free_string`].
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_take_control_help(app: *mut DbmApp) -> *mut c_char {
    let help = unsafe { with_handle(app, String::new(), |app| app.take_control_help()) };
    c_string(&help).into_raw()
}

/// Forget the help entries handed out so far: the next [`dbm_take_control_help`] returns all of
/// them (e.g. after the simulator connected again).
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_reset_control_help(app: *mut DbmApp) {
    unsafe { with_handle(app, (), |app| app.reset_control_help()) };
}

/// The notifications since the last call, one per line. Release the result with
/// [`dbm_free_string`].
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_take_notifications(app: *mut DbmApp) -> *mut c_char {
    let notifications = unsafe { with_handle(app, String::new(), |app| app.take_notifications()) };
    c_string(&notifications).into_raw()
}

/// Release a string returned by the library.
///
/// # Safety
/// `text` must be a string returned by this library (or null) and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn dbm_free_string(text: *mut c_char) {
    if !text.is_null() {
        drop(unsafe { CString::from_raw(text) });
    }
}

/// Set the document settings from the project.
///
/// # Safety
/// `app` must be a valid handle (or null); `data` must be a valid NUL terminated string (or
/// null).
#[no_mangle]
pub unsafe extern "C" fn dbm_set_formatted_document_settings(app: *mut DbmApp, data: *const c_char) {
    let data = unsafe { from_c(data) };
    unsafe { with_handle(app, (), |app| app.set_formatted_document_settings(&data)) };
}

/// Handle a MIDI message received on a MIDI input opened with `open_midi_input`.
///
/// # Safety
/// `app` must be a valid handle (or null); `data` must point to `length` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn dbm_on_midi_event(app: *mut DbmApp, device_id: c_int, data: *const u8, length: c_int) {
    if data.is_null() || length <= 0 {
        return;
    }
    let message = unsafe { std::slice::from_raw_parts(data, length as usize) }.to_vec();
    unsafe { with_handle(app, (), |app| app.on_midi_event(device_id, &message)) };
}

/// Parse the major and minor version from a version string like "7.22/linux-x86_64".
fn parse_version(version: &str) -> (i32, i32) {
    let mut numbers = version.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty());
    let major = numbers.next().and_then(|s| s.parse().ok());
    let minor = numbers.next().and_then(|s| s.parse().ok());
    match (major, minor) {
        (Some(major), Some(minor)) => (major, minor),
        _ => (1, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::UsbInstanceBackend;
    use crate::controller::UsbMatcher;

    /// The calls of `usb_callback`: instance, device index, vendor, product, interface,
    /// endpoint, length, first byte.
    type UsbCall = (c_int, c_int, u16, u16, u8, u8, usize, u8);

    extern "C" fn usb_callback(user: *mut c_void, instance: c_int, device_index: c_int, vendor_id: u16, product_id: u16, interface_number: u8, endpoint: u8, data: *const u8, length: usize) -> c_int {
        // SAFETY: `user` is the vector of the test, which outlives the backend
        let calls = unsafe { &mut *(user as *mut Vec<UsbCall>) };
        let first = unsafe { *data };
        calls.push((instance, device_index, vendor_id, product_id, interface_number, endpoint, length, first));
        1
    }

    #[test]
    fn usb_writes_reach_the_callback_with_their_device() {
        // SAFETY: an all-null callback table is valid (null function pointers are ignored)
        let callbacks: DbmCallbacks = unsafe { std::mem::zeroed() };
        let ffi = Rc::new(FfiBackend::new(callbacks));
        let push2 = UsbMatcher { vendor_id: 0x2982, product_id: 0x1967, interface: 0, endpoint: 0x01 };
        let instance = UsbInstanceBackend::new(ffi.clone(), 2, vec![push2]);
        let frame = [0xFFu8, 0xCC, 0xAA, 0x88];

        // Without a callback the writes fail
        assert!(!instance.usb_bulk_out(0, 0x01, &frame));

        let mut calls: Vec<UsbCall> = Vec::new();
        ffi.usb.set(Some((usb_callback, &mut calls as *mut Vec<UsbCall> as *mut c_void)));
        assert!(instance.usb_bulk_out(0, 0x01, &frame));
        // A device index without a device is not written
        assert!(!instance.usb_bulk_out(1, 0x01, &frame));
        // Directly on the backend (no instance): instance -1 and no IDs
        assert!(ffi.usb_bulk_out(0, 0x01, &frame));
        ffi.usb.set(None);
        assert_eq!(calls, [(2, 0, 0x2982, 0x1967, 0, 0x01, 4, 0xFF), (-1, 0, 0, 0, 0, 0x01, 4, 0xFF)]);
    }

    extern "C" fn simulator_callback(user: *mut c_void, instance: c_int, data: *const u8, length: usize) -> c_int {
        // SAFETY: `user` is the vector of the test, which outlives the backend
        let calls = unsafe { &mut *(user as *mut Vec<(c_int, Vec<u8>)>) };
        calls.push((instance, unsafe { std::slice::from_raw_parts(data, length) }.to_vec()));
        1
    }

    #[test]
    fn simulator_data_reaches_its_callback_with_the_instance() {
        // SAFETY: an all-null callback table is valid (null function pointers are ignored)
        let callbacks: DbmCallbacks = unsafe { std::mem::zeroed() };
        let ffi = Rc::new(FfiBackend::new(callbacks));
        let instance = UsbInstanceBackend::new(ffi.clone(), 2, Vec::new());
        assert!(!instance.simulator_data_enabled());
        assert!(!instance.simulator_data_out(b"DBMV"));
        let mut calls: Vec<(c_int, Vec<u8>)> = Vec::new();
        ffi.simulator.set(Some((simulator_callback, &mut calls as *mut Vec<(c_int, Vec<u8>)> as *mut c_void)));
        assert!(instance.simulator_data_enabled());
        assert!(instance.simulator_data_out(b"DBMV"));
        // The USB callback is not involved
        assert!(!instance.usb_bulk_out(0, 0x01, b"x"));
        ffi.simulator.set(None);
        assert_eq!(calls, [(2, b"DBMV".to_vec())]);
        assert_eq!(unsafe { dbm_set_simulator_callback(std::ptr::null_mut(), DBM_SIMULATOR_VERSION, Some(simulator_callback), std::ptr::null_mut()) }, 0);
    }

    #[test]
    fn usb_callback_version_and_device_list() {
        assert_eq!(unsafe { dbm_set_usb_callback(std::ptr::null_mut(), DBM_USB_VERSION, Some(usb_callback), std::ptr::null_mut()) }, 0);
        assert!(usb_devices_json("").starts_with('{'));
        assert_eq!(usb_devices_json("no-such-controller"), "{}");
        // Every listed device has the four numbers
        let all = usb_devices_json("");
        assert_eq!(all.matches("\"vendorId\"").count(), all.matches("\"endpoint\"").count());
    }

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("7.22/linux-x86_64"), (7, 22));
        assert_eq!(parse_version("v6.81"), (6, 81));
        assert_eq!(parse_version("unknown"), (1, 0));
    }
}
