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

/// The backend which forwards all calls to the C++ callbacks.
pub struct FfiBackend {
    callbacks: DbmCallbacks,
}

impl FfiBackend {
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
    fn process_no_arg(&self, processor: &str, command: &str) {
        let (p, c) = (c_string(processor), c_string(command));
        if let Some(f) = self.callbacks.process_no_arg {
            f(self.callbacks.user, p.as_ptr(), c.as_ptr());
        }
    }

    fn process_string_arg(&self, processor: &str, command: &str, value: &str) {
        let (p, c, v) = (c_string(processor), c_string(command), c_string(value));
        if let Some(f) = self.callbacks.process_string_arg {
            f(self.callbacks.user, p.as_ptr(), c.as_ptr(), v.as_ptr());
        }
    }

    fn process_string_args(&self, processor: &str, command: &str, values: &[&str]) {
        let (p, c) = (c_string(processor), c_string(command));
        let strings: Vec<CString> = values.iter().map(|v| c_string(v)).collect();
        let pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        if let Some(f) = self.callbacks.process_string_args {
            f(self.callbacks.user, p.as_ptr(), c.as_ptr(), pointers.as_ptr(), pointers.len() as c_int);
        }
    }

    fn process_int_arg(&self, processor: &str, command: &str, value: i32) {
        let (p, c) = (c_string(processor), c_string(command));
        if let Some(f) = self.callbacks.process_int_arg {
            f(self.callbacks.user, p.as_ptr(), c.as_ptr(), value);
        }
    }

    fn process_double_arg(&self, processor: &str, command: &str, value: f64) {
        let (p, c) = (c_string(processor), c_string(command));
        if let Some(f) = self.callbacks.process_double_arg {
            f(self.callbacks.user, p.as_ptr(), c.as_ptr(), value);
        }
    }

    fn delay_updates(&self, processor: &str) {
        let p = c_string(processor);
        if let Some(f) = self.callbacks.delay_updates {
            f(self.callbacks.user, p.as_ptr());
        }
    }

    fn enable_updates(&self, processor: &str, enable: bool) {
        let p = c_string(processor);
        if let Some(f) = self.callbacks.enable_updates {
            f(self.callbacks.user, p.as_ptr(), enable as c_int);
        }
    }

    fn process_midi_arg(&self, device_id: i32, status: i32, data1: i32, data2: i32) {
        if let Some(f) = self.callbacks.process_midi_arg {
            f(self.callbacks.user, device_id, status, data1, data2);
        }
    }

    fn midi_inputs(&self) -> Vec<(i32, String)> {
        self.collect_devices(self.callbacks.get_midi_inputs)
    }

    fn midi_outputs(&self) -> Vec<(i32, String)> {
        self.collect_devices(self.callbacks.get_midi_outputs)
    }

    fn open_midi_input(&self, device_id: i32) -> bool {
        self.callbacks.open_midi_input.is_some_and(|f| f(self.callbacks.user, device_id) != 0)
    }

    fn open_midi_output(&self, device_id: i32) -> bool {
        self.callbacks.open_midi_output.is_some_and(|f| f(self.callbacks.user, device_id) != 0)
    }

    fn close_midi_input(&self, device_id: i32) {
        if let Some(f) = self.callbacks.close_midi_input {
            f(self.callbacks.user, device_id);
        }
    }

    fn close_midi_output(&self, device_id: i32) {
        if let Some(f) = self.callbacks.close_midi_output {
            f(self.callbacks.user, device_id);
        }
    }

    fn send_midi_data(&self, device_id: i32, data: &[u8]) {
        if let Some(f) = self.callbacks.send_midi_data {
            f(self.callbacks.user, device_id, data.as_ptr(), data.len() as c_int);
        }
    }

    fn set_note_input_filters(&self, device_id: i32, note_input_index: i32, filters: &[String]) {
        let strings: Vec<CString> = filters.iter().map(|f| c_string(f)).collect();
        let pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        if let Some(f) = self.callbacks.set_note_input_filters {
            f(self.callbacks.user, device_id, note_input_index, pointers.as_ptr(), pointers.len() as c_int);
        }
    }

    fn set_note_input_key_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        if let Some(f) = self.callbacks.set_note_input_key_translation_table {
            f(self.callbacks.user, device_id, note_input_index, table.as_ptr(), table.len() as c_int);
        }
    }

    fn set_note_input_velocity_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        if let Some(f) = self.callbacks.set_note_input_velocity_translation_table {
            f(self.callbacks.user, device_id, note_input_index, table.as_ptr(), table.len() as c_int);
        }
    }

    fn log(&self, message: &str) {
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
struct AppHolder(Option<App>);

// SAFETY: see the documentation of `AppHolder`.
unsafe impl Send for AppHolder {}

/// The interval of the flush timer in milliseconds (same as in the Java version).
const DEVICE_UPDATE_RATE: u64 = 30;

/// Flag for [`dbm_create`]: do not start the internal timer thread. The host must call
/// [`dbm_tick`] regularly (every 30ms is recommended) instead.
pub const DBM_FLAG_HOST_TICK: u32 = 1;

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
            guard.0.as_mut().map(f)
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
        let backend: Rc<dyn Backend> = Rc::new(FfiBackend { callbacks });
        Some(App::new(backend, &ini_path, major, minor))
    })
}

/// Create an instance which runs exactly one controller, for hosts which declare each
/// controller themselves and assign its MIDI ports (Bitwig). `controller_id` is the UUID of the
/// controller (see [`dbm_list_controllers`]); its MIDI input and output are the device IDs 0.
/// `config_path` is the folder of the controller's configuration file. There is no main
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
        let backend: Rc<dyn Backend> = Rc::new(FfiBackend { callbacks });
        App::new_single(backend, &config_path, &controller_id, &app_version)
    })
}

/// Create the application with the given function and wrap it in a handle. Returns null if the
/// function returns `None` or panics.
fn new_handle(flags: u32, create: impl FnOnce() -> Option<App> + std::panic::UnwindSafe) -> *mut DbmApp {
    let app = match std::panic::catch_unwind(create) {
        Ok(Some(app)) => app,
        Ok(None) => return std::ptr::null_mut(),
        Err(_) => {
            eprintln!("[DrivenByMoss] Panic in the Rust library caught.");
            return std::ptr::null_mut();
        }
    };
    let inner = Arc::new(Inner { app: Mutex::new(AppHolder(Some(app))), timer_running: AtomicBool::new(false) });
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
/// states which the controller set while it was created. Release the result with
/// [`dbm_free_string`].
///
/// # Safety
/// `app` must be a valid handle (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_take_setting_states(app: *mut DbmApp) -> *mut c_char {
    let states = unsafe { with_handle(app, String::new(), |app| app.take_setting_states()) };
    c_string(&states).into_raw()
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
    use super::parse_version;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("7.22/linux-x86_64"), (7, 22));
        assert_eq!(parse_version("v6.81"), (6, 81));
        assert_eq!(parse_version("unknown"), (1, 0));
    }
}
