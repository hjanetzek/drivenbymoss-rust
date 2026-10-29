// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the C interface: the instances are driven only through the `dbm_*` functions and
//! `extern "C"` callbacks, exactly like a host does it.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::Mutex;

use drivenbymoss::ffi::*;

/// What the callbacks of one host instance recorded.
#[derive(Default)]
struct Recorder {
    commands: Vec<String>,
    midi_out: Vec<(i32, Vec<u8>)>,
    opened: Vec<i32>,
    inputs: Vec<(i32, String)>,
    outputs: Vec<(i32, String)>,
}

fn recorder<'a>(user: *mut c_void) -> std::sync::MutexGuard<'a, Recorder> {
    // SAFETY: `user` points to a `Mutex<Recorder>` which outlives the instance (see `Host`)
    unsafe { &*(user as *const Mutex<Recorder>) }.lock().unwrap()
}

fn s(text: *const c_char) -> String {
    if text.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    }
}

extern "C" fn no_arg(user: *mut c_void, processor: *const c_char, command: *const c_char) {
    recorder(user).commands.push(format!("{} {}", s(processor), s(command)));
}
extern "C" fn string_arg(user: *mut c_void, processor: *const c_char, command: *const c_char, value: *const c_char) {
    recorder(user).commands.push(format!("{} {} {}", s(processor), s(command), s(value)));
}
extern "C" fn string_args(user: *mut c_void, processor: *const c_char, command: *const c_char, _values: *const *const c_char, count: c_int) {
    recorder(user).commands.push(format!("{} {} [{count}]", s(processor), s(command)));
}
extern "C" fn int_arg(user: *mut c_void, processor: *const c_char, command: *const c_char, value: c_int) {
    recorder(user).commands.push(format!("{} {} {value}", s(processor), s(command)));
}
extern "C" fn double_arg(user: *mut c_void, processor: *const c_char, command: *const c_char, value: f64) {
    recorder(user).commands.push(format!("{} {} {value}", s(processor), s(command)));
}
extern "C" fn delay_updates(_user: *mut c_void, _processor: *const c_char) {}
extern "C" fn enable_updates(_user: *mut c_void, _processor: *const c_char, _enable: c_int) {}
extern "C" fn midi_arg(_user: *mut c_void, _device: c_int, _status: c_int, _data1: c_int, _data2: c_int) {}
extern "C" fn get_inputs(user: *mut c_void, list: *mut c_void, add: DbmAddDevice) {
    let inputs = recorder(user).inputs.clone();
    for (id, name) in inputs {
        let name = CString::new(name).unwrap();
        add(list, id, name.as_ptr());
    }
}
extern "C" fn get_outputs(user: *mut c_void, list: *mut c_void, add: DbmAddDevice) {
    let outputs = recorder(user).outputs.clone();
    for (id, name) in outputs {
        let name = CString::new(name).unwrap();
        add(list, id, name.as_ptr());
    }
}
extern "C" fn open(user: *mut c_void, device: c_int) -> c_int {
    recorder(user).opened.push(device);
    1
}
extern "C" fn close(_user: *mut c_void, _device: c_int) {}
extern "C" fn send_midi(user: *mut c_void, device: c_int, data: *const u8, length: c_int) {
    let data = unsafe { std::slice::from_raw_parts(data, length as usize) }.to_vec();
    recorder(user).midi_out.push((device, data));
}
extern "C" fn filters(_user: *mut c_void, _device: c_int, _index: c_int, _filters: *const *const c_char, _count: c_int) {}
extern "C" fn table(_user: *mut c_void, _device: c_int, _index: c_int, _table: *const c_int, _count: c_int) {}

/// A host with its recorder and an instance.
struct Host {
    recorder: Box<Mutex<Recorder>>,
    app: *mut DbmApp,
    dir: std::path::PathBuf,
}

impl Host {
    fn new(name: &str, flags: u32, with_launchpad: bool) -> Host {
        let dir = std::env::temp_dir().join(format!("dbm-ffi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut rec = Recorder::default();
        if with_launchpad {
            std::fs::write(
                dir.join("DrivenByMoss4Reaper.config"),
                "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadXControllerInstance\n",
            )
            .unwrap();
            rec.inputs.push((3, "Launchpad X LPX MIDI Out".into()));
            rec.outputs.push((5, "Launchpad X LPX MIDI In".into()));
        }
        let recorder = Box::new(Mutex::new(rec));
        let callbacks = DbmCallbacks {
            user: &*recorder as *const Mutex<Recorder> as *mut c_void,
            process_no_arg: Some(no_arg),
            process_string_arg: Some(string_arg),
            process_string_args: Some(string_args),
            process_int_arg: Some(int_arg),
            process_double_arg: Some(double_arg),
            delay_updates: Some(delay_updates),
            enable_updates: Some(enable_updates),
            process_midi_arg: Some(midi_arg),
            get_midi_inputs: Some(get_inputs),
            get_midi_outputs: Some(get_outputs),
            open_midi_input: Some(open),
            open_midi_output: Some(open),
            close_midi_input: Some(close),
            close_midi_output: Some(close),
            send_midi_data: Some(send_midi),
            set_note_input_filters: Some(filters),
            set_note_input_key_translation_table: Some(table),
            set_note_input_velocity_translation_table: Some(table),
        };
        let ini = CString::new(dir.to_str().unwrap()).unwrap();
        let version = CString::new("7.22/linux-x86_64").unwrap();
        let app = unsafe { dbm_create(&callbacks, ini.as_ptr(), version.as_ptr(), flags) };
        assert!(!app.is_null());
        Host { recorder, app, dir }
    }

    fn rec(&self) -> std::sync::MutexGuard<'_, Recorder> {
        self.recorder.lock().unwrap()
    }

    fn update(&self, data: &str) {
        let data = CString::new(data).unwrap();
        unsafe { dbm_update_model(self.app, data.as_ptr()) };
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        unsafe { dbm_destroy(self.app) };
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn version_is_a_static_string() {
    let version = unsafe { CStr::from_ptr(dbm_version()) }.to_str().unwrap();
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn null_callbacks_are_ignored() {
    let callbacks = DbmCallbacks {
        user: std::ptr::null_mut(),
        process_no_arg: None,
        process_string_arg: None,
        process_string_args: None,
        process_int_arg: None,
        process_double_arg: None,
        delay_updates: None,
        enable_updates: None,
        process_midi_arg: None,
        get_midi_inputs: None,
        get_midi_outputs: None,
        open_midi_input: None,
        open_midi_output: None,
        close_midi_input: None,
        close_midi_output: None,
        send_midi_data: None,
        set_note_input_filters: None,
        set_note_input_key_translation_table: None,
        set_note_input_velocity_translation_table: None,
    };
    unsafe {
        let app = dbm_create(&callbacks, std::ptr::null(), std::ptr::null(), DBM_FLAG_HOST_TICK);
        assert!(!app.is_null());
        dbm_start_infrastructure(app);
        dbm_tick(app);
        dbm_destroy(app);
    }
}

#[test]
fn null_arguments_are_safe() {
    unsafe {
        assert!(dbm_create(std::ptr::null(), std::ptr::null(), std::ptr::null(), 0).is_null());
        dbm_tick(std::ptr::null_mut());
        dbm_update_model(std::ptr::null_mut(), std::ptr::null());
        dbm_on_midi_event(std::ptr::null_mut(), 0, std::ptr::null(), 0);
        let settings = dbm_get_formatted_document_settings(std::ptr::null_mut());
        assert_eq!(CStr::from_ptr(settings).to_str().unwrap(), "");
        dbm_free_string(settings);
        dbm_free_string(std::ptr::null_mut());
        dbm_destroy(std::ptr::null_mut());
    }
}

#[test]
fn host_driven_ticking_with_a_launchpad() {
    let host = Host::new("tick", DBM_FLAG_HOST_TICK, true);
    unsafe { dbm_start_infrastructure(host.app) };
    assert_eq!(host.rec().opened, vec![3, 5]);
    assert!(host.rec().commands.iter().any(|c| c.starts_with("refresh")));

    host.update("track/count 1\ntrack/0/exists 1\ntrack/0/select 1\n");
    host.rec().midi_out.clear();
    unsafe { dbm_tick(host.app) };
    let pads = host.rec().midi_out.iter().filter(|(device, data)| *device == 5 && data[0] == 0x90).count();
    assert!(pads >= 64, "the pads are drawn on the host tick ({pads})");

    // No internal timer: nothing happens without a tick
    host.rec().midi_out.clear();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(host.rec().midi_out.is_empty());

    // A pad press (note on 0x90 11 127) is handled
    unsafe { dbm_on_midi_event(host.app, 3, [0x90u8, 11, 127].as_ptr(), 3) };

    let settings = unsafe { dbm_get_formatted_document_settings(host.app) };
    assert!(unsafe { CStr::from_ptr(settings) }.to_str().unwrap().starts_with("LAUNCHPAD_X=\""));
    unsafe { dbm_set_formatted_document_settings(host.app, settings) };
    unsafe { dbm_free_string(settings) };
}

#[test]
fn internal_timer_drives_the_instance() {
    let host = Host::new("timer", 0, true);
    unsafe { dbm_start_infrastructure(host.app) };
    host.update("track/count 1\ntrack/0/exists 1\n");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if host.rec().midi_out.iter().any(|(device, data)| *device == 5 && data[0] == 0x90) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    panic!("the timer thread did not flush the controller");
}

#[test]
fn instances_are_independent() {
    let a = Host::new("a", DBM_FLAG_HOST_TICK, true);
    let b = Host::new("b", DBM_FLAG_HOST_TICK, false);
    unsafe {
        dbm_start_infrastructure(a.app);
        dbm_start_infrastructure(b.app);
    }
    a.rec().midi_out.clear();
    unsafe {
        dbm_tick(a.app);
        dbm_tick(b.app);
    }
    assert!(!a.rec().midi_out.is_empty());
    assert!(b.rec().midi_out.is_empty(), "instance b has no controller");
    assert!(b.rec().opened.is_empty());
}

/// Compiles a small C program against the generated header with the system C and C++ compiler
/// (skipped if there is none).
#[test]
fn header_compiles_as_c_and_cpp() {
    let root = env!("CARGO_MANIFEST_DIR");
    for (compiler, args) in [("cc", vec!["-x", "c", "-std=c99"]), ("c++", vec!["-x", "c++", "-std=c++11"])] {
        let output = std::process::Command::new(compiler)
            .args(&args)
            .args(["-Wall", "-Wextra", "-Werror", "-fsyntax-only", "-I", &format!("{root}/include"), &format!("{root}/tests/c/header_check.c")])
            .output();
        match output {
            Ok(output) => assert!(output.status.success(), "{compiler}: {}", String::from_utf8_lossy(&output.stderr)),
            Err(_) => eprintln!("{compiler} not found, skipped"),
        }
    }
}
