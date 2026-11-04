// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Probe: the allocations per tick of the Rust controllers (idle, after a model change, after a
//! MIDI input, per model line), counted by a global allocator. Not a regression test, run it by
//! hand:
//!
//! ```text
//! cargo test --release --no-default-features --test alloc_probe -- --ignored --nocapture
//! DBM_ALLOC_SITES="Maschine Mk2" cargo test --test alloc_probe -- --ignored --nocapture
//! ```
//!
//! `DBM_ALLOC_SITES` lists where the allocations of one idle tick of that controller come from
//! (needs debug info for the backtraces, so a debug build).
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::Backend;

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static TRACE: Cell<bool> = const { Cell::new(false) };
    static SITES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Record the first frame of this crate's code which called the allocator.
fn record_site() {
    if !TRACE.with(|t| t.replace(false)) {
        return;
    }
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    let site = trace
        .lines()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0].contains("drivenbymoss::") && !w[0].contains("alloc_probe"))
        .map(|w| format!("{} {}", w[0].trim(), w[1].trim()))
        .unwrap_or_else(|| "?".into());
    SITES.with(|s| s.borrow_mut().push(site));
    TRACE.with(|t| t.set(true));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        record_site();
        BYTES.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        record_site();
        BYTES.fetch_add(n, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Default)]
struct Null {
    midi_out: Cell<usize>,
}
impl Backend for Null {
    fn process_no_arg(&self, _: &str, _: &str) {}
    fn process_string_arg(&self, _: &str, _: &str, _: &str) {}
    fn process_string_args(&self, _: &str, _: &str, _: &[&str]) {}
    fn process_int_arg(&self, _: &str, _: &str, _: i32) {}
    fn process_double_arg(&self, _: &str, _: &str, _: f64) {}
    fn delay_updates(&self, _: &str) {}
    fn enable_updates(&self, _: &str, _: bool) {}
    fn process_midi_arg(&self, _: i32, _: i32, _: i32, _: i32) {}
    fn midi_inputs(&self) -> Vec<(i32, String)> {
        Vec::new()
    }
    fn midi_outputs(&self) -> Vec<(i32, String)> {
        Vec::new()
    }
    fn open_midi_input(&self, _: i32) -> bool {
        true
    }
    fn open_midi_output(&self, _: i32) -> bool {
        true
    }
    fn close_midi_input(&self, _: i32) {}
    fn close_midi_output(&self, _: i32) {}
    fn send_midi_data(&self, _: i32, _: &[u8]) {
        self.midi_out.set(self.midi_out.get() + 1);
    }
    fn set_note_input_filters(&self, _: i32, _: i32, _: &[String]) {}
    fn set_note_input_key_translation_table(&self, _: i32, _: i32, _: &[i32]) {}
    fn set_note_input_velocity_translation_table(&self, _: i32, _: i32, _: &[i32]) {}
}

const MODEL: &str = "/track/count 8\n/track/0/exists 1\n/track/0/name Synth\n/track/0/select 1\n/track/0/volume 0.5\n/track/1/exists 1\n/track/1/name Drums\n/track/2/exists 1\n/track/3/exists 1\n/play 0\n";

fn measure(f: impl FnOnce()) -> (usize, usize) {
    let (a, b) = (ALLOCS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
    f();
    (ALLOCS.load(Ordering::Relaxed) - a, BYTES.load(Ordering::Relaxed) - b)
}

/// Where the allocations of one idle tick come from (`DBM_ALLOC_SITES=<controller name>`).
fn trace_idle_tick(app: &mut App, tick: &mut dyn FnMut(&mut App)) {
    SITES.with(|s| s.borrow_mut().clear());
    TRACE.with(|t| t.set(true));
    tick(app);
    TRACE.with(|t| t.set(false));
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for site in SITES.with(|s| s.borrow().clone()) {
        *counts.entry(site).or_default() += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    for (site, count) in counts {
        println!("  {count:>3}  {site}");
    }
}

#[test]
#[ignore = "probe: run with cargo test --release --test alloc_probe -- --ignored --nocapture"]
fn allocations_per_tick() {
    let controllers = [
        ("Launch Control XL", "5AFA5045-FF92-4737-A091-5B371CD5E529", vec![0xB8u8, 0x0D, 0x40]),
        ("Launchpad X", "CD196CCF-DF98-4AB0-9ABC-F0F29A60ACED", vec![0x90, 0x51, 0x7F]),
        ("Maschine Mk2", "015E4908-C049-4808-A645-A7A44BEFE36B", vec![0xB0, 0x0E, 0x41]),
        ("Maschine JAM", "72F32A83-A697-446B-9016-64AA3F9476E4", vec![0xB0, 0x56, 0x7F]),
        ("Akai Fire", "75CC171F-325C-45B8-B304-DAE96756D5BA", vec![0x90, 0x36, 0x7F]),
        ("Mackie MCU", "5F10A0CD-F866-41C0-B16A-AEA16282B657", vec![0xB0, 0x10, 0x41]),
        ("Generic Flexi", "1B43F14F-CAAF-468A-9418-417C678653A0", vec![0xB0, 0x10, 0x40]),
    ];
    println!("\n{:<18} {:>14} {:>16} {:>16} {:>16}", "controller", "idle tick", "tick after vol", "midi in + tick", "1 model line");
    for (name, uuid, midi) in controllers {
        let dir = std::env::temp_dir().join(format!("dbm-alloc-{}-{}", name.replace(' ', "_"), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let backend = Rc::new(Null::default());
        let mut app = App::new_single(backend.clone(), dir.to_str().unwrap(), uuid, "probe").unwrap();
        let t0 = Instant::now();
        let mut now = t0;
        app.set_time(now);
        app.start_infrastructure();
        app.update_model(MODEL);
        for _ in 0..60 {
            now += Duration::from_millis(30);
            app.set_time(now);
            app.tick();
        }
        let mut tick = |app: &mut App| {
            now += Duration::from_millis(30);
            app.set_time(now);
            app.tick();
        };
        // Steady state: average of 50 idle ticks
        let (a, b) = measure(|| {
            for _ in 0..50 {
                tick(&mut app);
            }
        });
        let idle = (a / 50, b / 50);
        if std::env::var("DBM_ALLOC_SITES").is_ok_and(|n| n == name) {
            trace_idle_tick(&mut app, &mut tick);
        }
        let vol = measure(|| {
            app.update_model("/track/0/volume 0.6\n");
            tick(&mut app);
        });
        let line = measure(|| app.update_model("/track/0/volume 0.7\n"));
        tick(&mut app);
        let midi_in = measure(|| {
            app.on_midi_event(0, &midi);
            tick(&mut app);
        });
        println!(
            "{name:<18} {:>6} / {:>5}B {:>8} / {:>5}B {:>8} / {:>5}B {:>8} / {:>5}B   (midi out total {})",
            idle.0, idle.1, vol.0, vol.1, midi_in.0, midi_in.1, line.0, line.1, backend.midi_out.get()
        );
    }
}
