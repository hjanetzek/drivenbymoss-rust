// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The C interface used by the C++ Reaper extension. The `dbm_*` functions replace the static
//! methods of the former Java `Controller` class; [`DbmCallbacks`] replaces the JNI `native`
//! methods of `MainApp`. See `include/drivenbymoss.h`.
//!
//! All calls are serialized with a global mutex. A background thread calls [`App::tick`] every
//! 30ms to execute scheduled tasks and to update the controllers (the former flush timer).

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::app::App;
use crate::backend::Backend;

/// Adds one MIDI device (ID and name) to a list, see `get_midi_inputs`.
pub type DbmAddDevice = extern "C" fn(list: *mut c_void, id: c_int, name: *const c_char);

/// The functions provided by the C++ extension. `user` is passed as the first argument to each
/// function.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DbmCallbacks {
    pub user: *mut c_void,
    pub process_no_arg: extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char),
    pub process_string_arg: extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, value: *const c_char),
    pub process_string_args:
        extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, values: *const *const c_char, count: c_int),
    pub process_int_arg: extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, value: c_int),
    pub process_double_arg: extern "C" fn(user: *mut c_void, processor: *const c_char, command: *const c_char, value: f64),
    pub delay_updates: extern "C" fn(user: *mut c_void, processor: *const c_char),
    pub enable_updates: extern "C" fn(user: *mut c_void, processor: *const c_char, enable: c_int),
    pub process_midi_arg: extern "C" fn(user: *mut c_void, device_id: c_int, status: c_int, data1: c_int, data2: c_int),
    pub get_midi_inputs: extern "C" fn(user: *mut c_void, list: *mut c_void, add: DbmAddDevice),
    pub get_midi_outputs: extern "C" fn(user: *mut c_void, list: *mut c_void, add: DbmAddDevice),
    pub open_midi_input: extern "C" fn(user: *mut c_void, device_id: c_int) -> c_int,
    pub open_midi_output: extern "C" fn(user: *mut c_void, device_id: c_int) -> c_int,
    pub close_midi_input: extern "C" fn(user: *mut c_void, device_id: c_int),
    pub close_midi_output: extern "C" fn(user: *mut c_void, device_id: c_int),
    pub send_midi_data: extern "C" fn(user: *mut c_void, device_id: c_int, data: *const u8, length: c_int),
    pub set_note_input_filters:
        extern "C" fn(user: *mut c_void, device_id: c_int, note_input_index: c_int, filters: *const *const c_char, count: c_int),
    pub set_note_input_key_translation_table:
        extern "C" fn(user: *mut c_void, device_id: c_int, note_input_index: c_int, table: *const c_int, count: c_int),
    pub set_note_input_velocity_translation_table:
        extern "C" fn(user: *mut c_void, device_id: c_int, note_input_index: c_int, table: *const c_int, count: c_int),
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
    fn collect_devices(&self, get: extern "C" fn(*mut c_void, *mut c_void, DbmAddDevice)) -> Vec<(i32, String)> {
        let mut list: Vec<(i32, String)> = Vec::new();
        get(self.callbacks.user, &mut list as *mut Vec<(i32, String)> as *mut c_void, add_device_to_list);
        list
    }
}

impl Backend for FfiBackend {
    fn process_no_arg(&self, processor: &str, command: &str) {
        let (p, c) = (c_string(processor), c_string(command));
        (self.callbacks.process_no_arg)(self.callbacks.user, p.as_ptr(), c.as_ptr());
    }

    fn process_string_arg(&self, processor: &str, command: &str, value: &str) {
        let (p, c, v) = (c_string(processor), c_string(command), c_string(value));
        (self.callbacks.process_string_arg)(self.callbacks.user, p.as_ptr(), c.as_ptr(), v.as_ptr());
    }

    fn process_string_args(&self, processor: &str, command: &str, values: &[&str]) {
        let (p, c) = (c_string(processor), c_string(command));
        let strings: Vec<CString> = values.iter().map(|v| c_string(v)).collect();
        let pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        (self.callbacks.process_string_args)(self.callbacks.user, p.as_ptr(), c.as_ptr(), pointers.as_ptr(), pointers.len() as c_int);
    }

    fn process_int_arg(&self, processor: &str, command: &str, value: i32) {
        let (p, c) = (c_string(processor), c_string(command));
        (self.callbacks.process_int_arg)(self.callbacks.user, p.as_ptr(), c.as_ptr(), value);
    }

    fn process_double_arg(&self, processor: &str, command: &str, value: f64) {
        let (p, c) = (c_string(processor), c_string(command));
        (self.callbacks.process_double_arg)(self.callbacks.user, p.as_ptr(), c.as_ptr(), value);
    }

    fn delay_updates(&self, processor: &str) {
        let p = c_string(processor);
        (self.callbacks.delay_updates)(self.callbacks.user, p.as_ptr());
    }

    fn enable_updates(&self, processor: &str, enable: bool) {
        let p = c_string(processor);
        (self.callbacks.enable_updates)(self.callbacks.user, p.as_ptr(), enable as c_int);
    }

    fn process_midi_arg(&self, device_id: i32, status: i32, data1: i32, data2: i32) {
        (self.callbacks.process_midi_arg)(self.callbacks.user, device_id, status, data1, data2);
    }

    fn midi_inputs(&self) -> Vec<(i32, String)> {
        self.collect_devices(self.callbacks.get_midi_inputs)
    }

    fn midi_outputs(&self) -> Vec<(i32, String)> {
        self.collect_devices(self.callbacks.get_midi_outputs)
    }

    fn open_midi_input(&self, device_id: i32) -> bool {
        (self.callbacks.open_midi_input)(self.callbacks.user, device_id) != 0
    }

    fn open_midi_output(&self, device_id: i32) -> bool {
        (self.callbacks.open_midi_output)(self.callbacks.user, device_id) != 0
    }

    fn close_midi_input(&self, device_id: i32) {
        (self.callbacks.close_midi_input)(self.callbacks.user, device_id);
    }

    fn close_midi_output(&self, device_id: i32) {
        (self.callbacks.close_midi_output)(self.callbacks.user, device_id);
    }

    fn send_midi_data(&self, device_id: i32, data: &[u8]) {
        (self.callbacks.send_midi_data)(self.callbacks.user, device_id, data.as_ptr(), data.len() as c_int);
    }

    fn set_note_input_filters(&self, device_id: i32, note_input_index: i32, filters: &[String]) {
        let strings: Vec<CString> = filters.iter().map(|f| c_string(f)).collect();
        let pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        (self.callbacks.set_note_input_filters)(self.callbacks.user, device_id, note_input_index, pointers.as_ptr(), pointers.len() as c_int);
    }

    fn set_note_input_key_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        (self.callbacks.set_note_input_key_translation_table)(self.callbacks.user, device_id, note_input_index, table.as_ptr(), table.len() as c_int);
    }

    fn set_note_input_velocity_translation_table(&self, device_id: i32, note_input_index: i32, table: &[i32]) {
        (self.callbacks.set_note_input_velocity_translation_table)(
            self.callbacks.user,
            device_id,
            note_input_index,
            table.as_ptr(),
            table.len() as c_int,
        );
    }
}

/// Holds the application. The application uses `Rc` internally and is therefore not `Send`; it
/// is only ever accessed while holding the global mutex and none of its `Rc`s escape from it,
/// which makes moving it between the calling threads sound.
struct AppHolder(Option<App>);

// SAFETY: see the documentation of `AppHolder`.
unsafe impl Send for AppHolder {}

static APP: Mutex<AppHolder> = Mutex::new(AppHolder(None));
static TIMER_RUNNING: AtomicBool = AtomicBool::new(false);
static TIMER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// The interval of the flush timer in milliseconds (same as in the Java version).
const DEVICE_UPDATE_RATE: u64 = 30;

/// Run a function with the application, if it is started. Panics are caught to never unwind into
/// the C++ code.
fn with_app<R>(default: R, f: impl FnOnce(&mut App) -> R) -> R {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut guard = match APP.lock() {
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

/// Start the application.
///
/// # Safety
/// `callbacks` must point to a valid callback table which stays valid until [`dbm_shutdown`];
/// the strings must be valid NUL terminated strings (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_startup(callbacks: *const DbmCallbacks, ini_path: *const c_char, app_version: *const c_char) {
    if callbacks.is_null() {
        return;
    }
    let callbacks = unsafe { *callbacks };
    let ini_path = unsafe { from_c(ini_path) };
    let app_version = unsafe { from_c(app_version) };
    let (major, minor) = parse_version(&app_version);
    let backend: Rc<dyn Backend> = Rc::new(FfiBackend { callbacks });
    let mut guard = match APP.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.0 = Some(App::new(backend, &ini_path, major, minor));
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

/// Add an available device (e.g. a plugin) with its identifier.
///
/// # Safety
/// The strings must be valid NUL terminated strings (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_add_device(name: *const c_char, identifier: *const c_char) {
    let (name, identifier) = unsafe { (from_c(name), from_c(identifier)) };
    with_app((), |app| app.add_device(&name, &identifier));
}

/// Start the controller instances and the flush timer.
#[no_mangle]
pub extern "C" fn dbm_start_infrastructure() {
    with_app((), |app| app.start_infrastructure());
    if !TIMER_RUNNING.swap(true, Ordering::SeqCst) {
        let handle = std::thread::spawn(|| {
            while TIMER_RUNNING.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(DEVICE_UPDATE_RATE));
                with_app((), |app| app.tick());
            }
        });
        if let Ok(mut timer) = TIMER.lock() {
            *timer = Some(handle);
        }
    }
}

/// Update the model with data from Reaper: lines of `<address> <value>`.
///
/// # Safety
/// `data` must be a valid NUL terminated string (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_update_model(data: *const c_char) {
    let data = unsafe { from_c(data) };
    with_app((), |app| app.update_model(&data));
}

/// Shutdown the application.
#[no_mangle]
pub extern "C" fn dbm_shutdown() {
    TIMER_RUNNING.store(false, Ordering::SeqCst);
    if let Some(handle) = TIMER.lock().ok().and_then(|mut timer| timer.take()) {
        let _ = handle.join();
    }
    with_app((), |app| app.shutdown());
    if let Ok(mut guard) = APP.lock() {
        guard.0 = None;
    }
}

/// Display the application window (the Rust version has no UI, the log is printed instead).
#[no_mangle]
pub extern "C" fn dbm_display_window() {
    with_app((), |app| app.display_window());
}

/// Display the project settings window (not available in the Rust version).
#[no_mangle]
pub extern "C" fn dbm_display_project_window() {
    with_app((), |app| app.display_window());
}

/// Display the parameter mapping window (not available in the Rust version).
#[no_mangle]
pub extern "C" fn dbm_display_parameter_window() {
    with_app((), |app| app.display_window());
}

/// Restart all configured controllers.
#[no_mangle]
pub extern "C" fn dbm_restart_controllers() {
    with_app((), |app| app.restart_controllers());
}

/// Set the default initial settings for the document/project.
#[no_mangle]
pub extern "C" fn dbm_set_default_document_settings() {
    with_app((), |app| app.set_default_document_settings());
}

/// Get the document settings formatted to be stored in the Reaper extension data. The returned
/// string must be released with [`dbm_free_string`].
#[no_mangle]
pub extern "C" fn dbm_get_formatted_document_settings() -> *mut c_char {
    let settings = with_app(String::new(), |app| app.formatted_document_settings());
    c_string(&settings).into_raw()
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

/// Set the document settings from the Reaper extension data.
///
/// # Safety
/// `data` must be a valid NUL terminated string (or null).
#[no_mangle]
pub unsafe extern "C" fn dbm_set_formatted_document_settings(data: *const c_char) {
    let data = unsafe { from_c(data) };
    with_app((), |app| app.set_formatted_document_settings(&data));
}

/// Handle a MIDI message received on a MIDI input of Reaper.
///
/// # Safety
/// `data` must point to `length` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn dbm_on_midi_event(device_id: c_int, data: *const u8, length: c_int) {
    if data.is_null() || length <= 0 {
        return;
    }
    let message = unsafe { std::slice::from_raw_parts(data, length as usize) }.to_vec();
    with_app((), |app| app.on_midi_event(device_id, &message));
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
