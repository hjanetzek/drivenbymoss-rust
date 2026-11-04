// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Lua engine: loading from the configuration folder, a button with a light, the
//! error policy, the limits, the validity of `ctx` and re-entrancy.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::app::App;
use crate::backend::{BackendCall, RecordingBackend};
use crate::controller::find_factory;
use crate::framework::ids::Modes;

const IN: i32 = 1;
const OUT: i32 = 2;

/// A controller with one button which toggles its light.
const ONE_BUTTON: &str = r#"
-- A test controller: one button which toggles its light
local lit = false
return {
    info = { vendor = "Test", model = "One Button", uuid = "8E0A4E0C-0000-4000-8000-00000000B001", class_name = "test.OneButtonControllerInstance" },
    discovery = function(os) return { { inputs = { "One Button" }, outputs = { "One Button" } } } end,
    settings = { "midi_edit_channel" },
    create = function(ctx)
        ctx.surface:button("Play", {
            label = "Play",
            command = function(ctx, event, velocity)
                if event == "down" then lit = not lit end
            end,
            light = function(ctx) return lit and 127 or 0 end,
            midi = { "cc", 0, 20 },
        })
    end,
}
"#;

fn temp_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-lua-{name}-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(super::super::USER_FOLDER)).unwrap();
    dir
}

/// Start the controller of the script (written into the user folder of a new configuration
/// folder) with the given class name.
fn start(name: &str, script: &str, class_name: &str, port: &str) -> (App, Rc<RecordingBackend>) {
    let dir = temp_dir(name);
    std::fs::write(dir.join(super::super::USER_FOLDER).join(format!("{name}.lua")), script).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={class_name}\n")).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, port.into())], outputs: vec![(OUT, port.into())], ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    app.set_time_ms(0);
    app.start_infrastructure();
    // The startup is delayed
    app.set_time_ms(1100);
    app.tick();
    (app, backend)
}

fn errors(backend: &RecordingBackend) -> Vec<String> {
    backend.logs.borrow().iter().filter(|l| l.starts_with("ERROR")).cloned().collect()
}

fn midi_out(calls: &[BackendCall]) -> Vec<Vec<u8>> {
    calls.iter().filter_map(|c| if let BackendCall::SendMidi(OUT, d) = c { Some(d.clone()) } else { None }).collect()
}

#[test]
fn one_button_lights_up() {
    let (mut app, backend) = start("one_button", ONE_BUTTON, "test.OneButtonControllerInstance", "One Button");
    assert_eq!(errors(&backend), Vec::<String>::new());
    let calls = backend.take_calls();
    // The light is off at the start
    assert!(midi_out(&calls).contains(&vec![0xB0, 20, 0]), "{calls:?}");
    app.on_midi_event(IN, &[0xB0, 20, 127]);
    app.on_midi_event(IN, &[0xB0, 20, 0]);
    app.tick();
    assert_eq!(midi_out(&backend.take_calls()), vec![vec![0xB0, 20, 127]]);
    app.on_midi_event(IN, &[0xB0, 20, 127]);
    app.tick();
    assert_eq!(midi_out(&backend.take_calls()), vec![vec![0xB0, 20, 0]]);
    assert_eq!(errors(&backend), Vec::<String>::new());

    // The factory: info, discovery and settings
    let factory = find_factory("8e0a4e0c-0000-4000-8000-00000000b001").expect("the script is found");
    assert_eq!(factory.info().to_string(), "Test One Button");
    assert_eq!(factory.java_class_name(), "test.OneButtonControllerInstance");
    assert_eq!(factory.midi_discovery_pairs(crate::controller::OperatingSystem::Mac), vec![(vec!["One Button".to_string()], vec!["One Button".to_string()])]);
    assert!(factory.create_configuration().is_setting_active(crate::config::SettingId::MIDI_EDIT_CHANNEL));
    assert!(crate::controller::catalog_json().contains("One Button"));
}

#[test]
fn the_launch_control_xl_is_built_in() {
    let factory = find_factory(LCXL_LUA_UUID).expect("built in");
    assert_eq!(factory.info().to_string(), "Novation Launch Control XL (Lua)");
    assert_eq!(factory.midi_discovery_pairs(crate::controller::OperatingSystem::Windows).len(), 20);
}

/// The UUID of the Lua Launch Control XL.
const LCXL_LUA_UUID: &str = "4E926F54-1B67-4363-B971-D248C52F175D";

const BROKEN: &str = r#"
local calls = 0
return {
    info = { vendor = "Test", model = "Broken", uuid = "8E0A4E0C-0000-4000-8000-00000000B002", class_name = "test.BrokenControllerInstance" },
    discovery = function() return { { inputs = { "Broken" }, outputs = { "Broken" } } } end,
    create = function(ctx)
        ctx.surface:button("Play", {
            command = function(ctx, event) local x = nil; x.y = 1 end,
            light = function(ctx) calls = calls + 1; return ctx.no_such_field.x end,
            midi = { "cc", 0, 20 },
        })
        ctx.surface:button("Stop", {
            -- Changes are not allowed in a light
            light = function(ctx) ctx:notify("x"); return 0 end,
            midi = { "cc", 0, 21 },
        })
        ctx.surface:button("Record", {
            command = function(ctx, event)
                if event ~= "down" then return end
                -- An endless loop
                while true do end
            end,
        })
        ctx.surface:button("Loop", {
            command = function(ctx, event)
                if event ~= "down" then return end
                local t = {}
                for i = 1, 100000000 do t[i] = string.rep("x", 1000) .. i end
            end,
        })
        -- A mode which switches its own mode manager
        ctx:register_mode("Volume", { name = "Self", on_button = function(self, ctx) ctx:set_active_mode("Pan") end })
        ctx:register_mode("Pan", { name = "Other" })
        ctx:set_active_mode("Volume")
        ctx.surface:button("Row11", { command = ctx:command("ButtonRowMode", { row = 0, index = 0 }) })
        -- ctx is kept and used outside of its call
        saved_ctx = ctx
        ctx.surface:button("Undo", { command = function(c, event) if event == "down" then saved_ctx:notify("x") end end })
    end,
    startup = function(ctx) ctx.host:println("started " .. tostring(ctx:is_valid()) .. " " .. tostring(saved_ctx:is_valid())) end,
}
"#;

#[test]
fn errors_are_logged_and_the_function_is_disabled() {
    let (mut app, backend) = start("broken", BROKEN, "test.BrokenControllerInstance", "Broken");
    let errors_at_start = errors(&backend);
    // The light failed on the flush (the startup of the instance flushes)
    assert!(errors_at_start.iter().any(|e| e.contains("broken.lua:9:")), "{errors_at_start:?}");
    // A changed context in a light: the location is the line of the call
    assert!(errors_at_start.iter().any(|e| e.contains("broken.lua:14:") && e.contains("read-only")), "{errors_at_start:?}");
    // The kept ctx is valid only in its call
    assert!(backend.logs.borrow().iter().any(|l| l.contains("started true false")), "{:?}", backend.logs.borrow());

    for _ in 0..5 {
        app.tick();
    }
    let all = errors(&backend);
    let light_errors = all.iter().filter(|e| e.contains("broken.lua:9:")).count();
    assert_eq!(light_errors, 3, "{all:?}");
    assert!(all.iter().any(|e| e.contains("light of button Play is disabled")), "{all:?}");

    // The command fails, the instance continues
    backend.logs.borrow_mut().clear();
    app.on_midi_event(IN, &[0xB0, 20, 127]);
    assert!(errors(&backend).iter().any(|e| e.contains("broken.lua:8:") && e.contains("nil")), "{:?}", errors(&backend));

    backend.logs.borrow_mut().clear();
    {
        let ctx = app.instance_ctx(0).unwrap();
        // The endless loop is stopped
        ctx.trigger_button(crate::framework::ids::ButtonId::Record, crate::midi::ButtonEvent::Down, 1.0);
        // The memory is limited
        ctx.trigger_button(crate::framework::ids::ButtonId::Loop, crate::midi::ButtonEvent::Down, 1.0);
        // Re-entrancy: a Lua error, not a panic, and the mode stays active
        ctx.trigger_button(crate::framework::ids::ButtonId::Row11, crate::midi::ButtonEvent::Down, 1.0);
        assert!(ctx.modes.is_active(Modes::Volume));
        // ctx used outside of its call
        ctx.trigger_button(crate::framework::ids::ButtonId::Undo, crate::midi::ButtonEvent::Down, 1.0);
    }
    let all = errors(&backend);
    assert!(all.iter().any(|e| e.contains("instruction limit")), "{all:?}");
    assert!(all.iter().any(|e| e.contains("out of memory") || e.contains("memory")), "{all:?}");
    assert!(all.iter().any(|e| e.contains("broken.lua:32:") && e.contains("is running")), "{all:?}");
    assert!(all.iter().any(|e| e.contains("broken.lua:38:") && e.contains("outside of the call")), "{all:?}");
    // The instance still works
    app.tick();
}

#[test]
fn a_script_with_a_syntax_error_is_not_listed() {
    let dir = temp_dir("syntax");
    std::fs::write(dir.join(super::super::USER_FOLDER).join("syntax.lua"), "return { info = ").unwrap();
    super::super::set_config_folder(&dir);
    let sources = super::super::sources();
    assert!(sources.iter().any(|s| s.name == "syntax.lua"));
    assert!(super::super::factories().iter().all(|f| f.info().hardware_model != "Syntax"));
}

const VIEWS_AND_MODES: &str = r#"
local drawn = 0
return {
    info = { vendor = "Test", model = "Views", uuid = "8E0A4E0C-0000-4000-8000-00000000B003", class_name = "test.ViewsControllerInstance" },
    discovery = function() return { { inputs = { "Views" }, outputs = { "Views" } } } end,
    colors = { indices = { BUTTON_COLOR_OFF = 0, BUTTON_COLOR_ON = 1 } },
    model = function(config) return { num_sends = 2, full_flat_track_list = config.master_tracks_included } end,
    create = function(ctx)
        ctx:register_view("Play", { name = "Lua Play", draw_grid = function(self, ctx) drawn = drawn + 1 end, on_grid_note = function() end })
        ctx:register_view("Session", ctx:create_view("ControlOnly"))
        ctx:set_active_view("Play")
        local base = ctx:create_mode("TrackVolume", { controls = { "Knob1", "Knob2", "Knob3", "Knob4", "Knob5", "Knob6", "Knob7", "Knob8" } })
        ctx:register_mode("Volume", { base = base, forward = { "on_activate", "on_deactivate", "knob_value" },
            button_color = function(self, ctx, button) return button == "Row11" and 5 or 0 end })
        ctx:register_mode("Pan", ctx:create_mode("TrackPan", { controls = { "Knob1", "Knob2", "Knob3", "Knob4", "Knob5", "Knob6", "Knob7", "Knob8" } }))
        ctx:set_active_mode("Volume")
        ctx.surface:continuous("Knob1", { kind = "absolute_knob", midi = { "cc", 0, 1 } })
        ctx.surface:continuous("Knob2", { kind = "absolute_knob", command = ctx:command("KnobRowMode", { index = 1 }), midi = { "cc", 0, 2 } })
    end,
    flush = function(ctx) ctx:flush_surface() end,
    state = function() return drawn end,
}
"#;

#[test]
fn views_and_modes() {
    let (mut app, backend) = start("views", VIEWS_AND_MODES, "test.ViewsControllerInstance", "Views");
    assert_eq!(errors(&backend), Vec::<String>::new());
    app.update_model("track/count 1\ntrack/0/exists 1\n");
    app.tick();
    app.on_midi_event(IN, &[0xB0, 1, 127]);
    assert!(backend.take_calls().iter().any(|c| matches!(c, BackendCall::DoubleArg(p, cmd, v) if p == "track" && cmd == "0/volume" && *v == 1.0)));
    let ctx = app.instance_ctx(0).unwrap();
    assert_eq!(ctx.mode_button_color(crate::framework::ids::ButtonId::Row11), 5);
    assert_eq!(ctx.active_view().unwrap().borrow().name(), "Lua Play");
    assert!(super::super::eval(ctx, "return controller.state()").unwrap().parse::<i32>().unwrap() >= 2);
    ctx.set_active_mode(Modes::Pan);
    assert!(ctx.modes.is_active(Modes::Pan));
    assert_eq!(errors(&backend), Vec::<String>::new());
}

/// doc/LUA_API.md is generated from the bindings.
#[test]
fn lua_api_is_up_to_date() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/LUA_API.md");
    let generated = super::api_reference();
    assert!(!generated.starts_with("Error"), "{generated}");
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(current == generated, "doc/LUA_API.md is out of date: cargo test --lib -- --ignored write_lua_api");
}

#[test]
#[ignore]
fn write_lua_api() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/LUA_API.md");
    std::fs::write(path, super::api_reference()).unwrap();
}

const REENTRANT_BASE: &str = r#"
local mode = {}
local parameter = {
    does_exist = function(self, ctx) return true end,
    value = function(self, ctx) return 0 end,
    set_normalized_value = function(self, ctx, value)
        -- The base of the mode runs (it calls this parameter): a Lua error, not a panic
        mode.base:bind_controls(ctx)
    end,
    change_value = function(self, ctx, control) end,
}
return {
    info = { vendor = "Test", model = "Reentrant", uuid = "8E0A4E0C-0000-4000-8000-00000000B004", class_name = "test.ReentrantControllerInstance" },
    discovery = function() return { { inputs = { "Reentrant" }, outputs = { "Reentrant" } } } end,
    create = function(ctx)
        mode.base = ctx:create_mode("Parameter", { name = "P", controls = { "Knob1" }, provider = ctx:provider("fixed", { ctx:parameter("custom", parameter) }) })
        ctx:register_mode("Volume", mode)
        ctx:set_active_mode("Volume")
        ctx.surface:button("Play", { command = function(ctx, event)
            if event == "down" then mode.base:on_knob_value(ctx, 0, 64) end
        end })
    end,
}
"#;

#[test]
fn reentrant_calls_into_a_base_are_lua_errors() {
    let (mut app, backend) = start("reentrant", REENTRANT_BASE, "test.ReentrantControllerInstance", "Reentrant");
    assert_eq!(errors(&backend), Vec::<String>::new());
    let ctx = app.instance_ctx(0).unwrap();
    ctx.trigger_button(crate::framework::ids::ButtonId::Play, crate::midi::ButtonEvent::Down, 1.0);
    let all = errors(&backend);
    assert!(all.iter().any(|e| e.contains("reentrant.lua:8:") && e.contains("re-entrant")), "{all:?}");
    // The outer call went on
    assert_eq!(all.len(), 1, "{all:?}");
}
