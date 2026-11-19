// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the other Push modes (note, frame, groove, quantize, accent, scales, scale layout,
//! fixed, ribbon, view select, automation, transport, markers, session, session view select,
//! note repeat) and the configuration modes on the Push 1 (text display) and Push 2/3 (graphics
//! display). The modes are driven directly (`on_button`, `on_knob_value`) so that the tests do
//! not depend on the button registration.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use drivenbymoss::app::App;
use drivenbymoss::backend::{RecordedCalls, RecordingBackend};
use drivenbymoss::config::push as config;
use drivenbymoss::framework::ctx::Ctx;
use drivenbymoss::framework::graphics::Component;
use drivenbymoss::framework::ids::{ButtonId, Modes};
use drivenbymoss::midi::ButtonEvent;
use drivenbymoss::protocol::Cmd;

const IN: i32 = 3;
const OUT: i32 = 5;

const TRACKS: &str = "track/count 2\ntrack/0/exists 1\ntrack/0/name Drums\ntrack/0/select 1\ntrack/0/volume 0.5\ntrack/0/volumeStr 0 dB\ntrack/1/exists 1\ntrack/1/name Bass\n\
track/0/slot/count 2\ntrack/0/slot/0/exists 1\ntrack/0/slot/0/hasContent 1\ntrack/0/slot/0/name Intro\nscene/count 2\nscene/0/exists 1\nscene/0/name Verse\nscene/1/exists 1\n\
marker/count 2\nmarker/0/exists 1\nmarker/0/name Start\nmarker/1/exists 1\nmarker/1/name Chorus\nplay 0\n";

/// The modes of this module (and the configuration modes, which depend on the model).
const MISC_MODES: [Modes; 16] = [
    Modes::Note,
    Modes::Frame,
    Modes::Groove,
    Modes::RecArm,
    Modes::Accent,
    Modes::Scales,
    Modes::ScaleLayout,
    Modes::Fixed,
    Modes::Ribbon,
    Modes::ViewSelect,
    Modes::Automation,
    Modes::Transport,
    Modes::Markers,
    Modes::Session,
    Modes::SessionViewSelect,
    Modes::RepeatNote,
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Model {
    Push1,
    Push2,
    Push3,
}

impl Model {
    fn class(self) -> &'static str {
        match self {
            Model::Push1 => "de.mossgrabers.reaper.controller.ableton.push.Push1ControllerInstance",
            Model::Push2 => "de.mossgrabers.reaper.controller.ableton.push.Push2ControllerInstance",
            Model::Push3 => "de.mossgrabers.reaper.controller.ableton.push.Push3ControllerInstance",
        }
    }

    fn port(self) -> &'static str {
        match self {
            Model::Push1 => "Ableton Push User Port",
            Model::Push2 => "Ableton Push 2 Live Port",
            Model::Push3 => "Ableton Push 3 Live Port",
        }
    }

    fn file(self) -> &'static str {
        match self {
            Model::Push1 => "Push 1",
            Model::Push2 => "Push 2",
            Model::Push3 => "Push 3",
        }
    }

    fn configuration_modes(self) -> Vec<Modes> {
        match self {
            Model::Push1 => vec![Modes::Configuration],
            Model::Push2 | Model::Push3 => vec![Modes::Info, Modes::Setup, Modes::Configuration, Modes::Audio],
        }
    }
}

fn start(model: Model) -> (App, Rc<RecordingBackend>) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("dbm-test-push-misc-{}-{count}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DrivenByMoss4Reaper.config"), format!("CONTROLLER_INSTANCE0={}\n", model.class())).unwrap();
    let port = model.port();
    std::fs::write(dir.join(format!("DrivenByMoss4Reaper-{}.config", model.file())), format!("MIDI_INPUT0={port}\nMIDI_OUTPUT0={port}\n")).unwrap();
    let backend = Rc::new(RecordingBackend { inputs: vec![(IN, port.into())], outputs: vec![(OUT, port.into())], ..Default::default() });
    let mut app = App::new(backend.clone(), dir.to_str().unwrap(), 7, 22);
    assert_eq!(app.instance_count(), 1);
    app.start_infrastructure();
    app.update_model(TRACKS);
    std::thread::sleep(Duration::from_millis(1050));
    app.tick();
    (app, backend)
}

fn ctx(app: &mut App) -> &mut Ctx {
    app.instance_ctx(0).expect("instance runs")
}

/// Activate a mode and run the deferred tasks.
fn activate(app: &mut App, mode: Modes) {
    ctx(app).set_active_mode(mode);
    app.tick();
}

/// Run the deferred tasks (mode switches requested by a mode).
fn run_tasks(app: &mut App) {
    let ctx = ctx(app);
    let now = ctx.host.now() + Duration::from_millis(10);
    ctx.run_due_tasks(now);
}

fn button(app: &mut App, row: usize, index: usize, event: ButtonEvent) {
    let ctx = ctx(app);
    let mode = ctx.active_mode().expect("a mode is active");
    mode.borrow_mut().on_button(ctx, row, index, event);
}

fn click(app: &mut App, row: usize, index: usize) {
    button(app, row, index, ButtonEvent::Down);
    button(app, row, index, ButtonEvent::Up);
}

fn knob(app: &mut App, index: usize, value: i32) {
    let ctx = ctx(app);
    let mode = ctx.active_mode().expect("a mode is active");
    mode.borrow_mut().on_knob_value(ctx, index, value);
}

fn button_color(app: &mut App, button: ButtonId) -> i32 {
    ctx(app).mode_button_color(button)
}

/// The components which the active mode adds to the graphics display.
fn components(app: &mut App) -> Vec<Component> {
    let ctx = ctx(app);
    ctx.update_active_mode_display();
    let display = ctx.graphics_display.as_mut().expect("graphics display");
    let components = display.components().to_vec();
    // Clear the components as the flush does
    display.render(Instant::now());
    components
}

/// The texts of the top and bottom menus of the option columns.
fn option_texts(components: &[Component]) -> Vec<(String, String)> {
    components
        .iter()
        .filter_map(|c| match c {
            Component::Options(o) => Some((o.header.text.clone(), o.footer.text.clone())),
            _ => None,
        })
        .collect()
}

/// The 4 rows of the Push 1 display.
fn text_rows(app: &mut App) -> Vec<String> {
    let ctx = ctx(app);
    ctx.update_active_mode_display();
    let display = ctx.text_display.as_mut().expect("text display");
    display.flush();
    (0..4).map(|row| display.current_line(row).unwrap_or_default().to_string()).collect()
}

#[test]
fn all_modes_are_registered() {
    for model in [Model::Push1, Model::Push2, Model::Push3] {
        let (mut app, _) = start(model);
        let ctx = ctx(&mut app);
        for mode in MISC_MODES.iter().copied().chain(model.configuration_modes()) {
            assert!(ctx.modes.contains(mode), "{model:?}: {mode:?}");
        }
        if model == Model::Push1 {
            assert!(!ctx.modes.contains(Modes::Info));
            assert!(!ctx.modes.contains(Modes::Audio));
        }
    }
}

#[test]
fn every_mode_draws_8_columns_on_push2() {
    let (mut app, _) = start(Model::Push2);
    for mode in MISC_MODES.iter().copied().chain(Model::Push2.configuration_modes()) {
        activate(&mut app, mode);
        let components = components(&mut app);
        assert_eq!(components.len(), 8, "{mode:?}: {components:?}");
    }
}

#[test]
fn fixed_mode() {
    let (mut app, backend) = start(Model::Push2);
    activate(&mut app, Modes::Session);
    activate(&mut app, Modes::Fixed);
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts[0].1, "1 Beat");
    assert_eq!(texts[7].1, "32 Bars");
    // The selected length is highlighted
    let on = button_color(&mut app, ButtonId::Row11);
    let hi = button_color(&mut app, ButtonId::Row13);
    assert_ne!(on, hi);

    click(&mut app, 0, 4);
    assert_eq!(ctx(&mut app).config.new_clip_length(), 4);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Session), "the mode is restored");

    // Second row: create a clip with a temporary length (needs a cursor track)
    activate(&mut app, Modes::Fixed);
    backend.take_calls();
    click(&mut app, 1, 2);
    assert_eq!(ctx(&mut app).config.new_clip_length(), 4, "the length is not stored");
}

#[test]
fn fixed_mode_push1_display() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::Fixed);
    let rows = text_rows(&mut app);
    assert!(rows[0].starts_with("1 Beat"), "{rows:?}");
    assert!(rows[1].starts_with("Create Clip (leng"), "{rows:?}");
    assert!(rows[2].starts_with("New Clip Length:"), "{rows:?}");
}

#[test]
fn ribbon_mode() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Ribbon);
    click(&mut app, 0, 4);
    assert_eq!(config::ribbon_mode(&ctx(&mut app).config), config::RIBBON_MODE_FADER);
    click(&mut app, 1, 3);
    assert_eq!(config::ribbon_mode_cc_val(&ctx(&mut app).config), 7);
    click(&mut app, 1, 6);
    assert_eq!(config::ribbon_note_repeat(&ctx(&mut app).config), config::NOTE_REPEAT_PERIOD);
    knob(&mut app, 0, 1);
    assert_eq!(config::ribbon_mode_cc_val(&ctx(&mut app).config), 8);
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts[0].0, "8");
    assert_eq!(texts[4].1, "Fader");
}

#[test]
fn scales_mode() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Scales);
    // First row 1-6: the base notes C, G, D, A, E, B
    click(&mut app, 0, 2);
    assert_eq!(ctx(&mut app).model.scales().scale_offset_index(), 1);
    assert_eq!(ctx(&mut app).config.scale_base(), "G");
    // Second row 1-6: F, Bb, ...
    click(&mut app, 1, 2);
    assert_eq!(ctx(&mut app).model.scales().scale_offset_index(), 7);
    // Chromatic
    let was_chromatic = ctx(&mut app).model.scales().is_chromatic();
    click(&mut app, 1, 7);
    assert_ne!(ctx(&mut app).model.scales().is_chromatic(), was_chromatic);
    // Next scale (Push 2: first button)
    let scale = ctx(&mut app).model.scales().scale();
    click(&mut app, 0, 0);
    assert_ne!(ctx(&mut app).model.scales().scale(), scale);
    let components = components(&mut app);
    assert!(matches!(components[0], Component::List { .. }));
}

#[test]
fn scale_layout_mode() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::ScaleLayout);
    click(&mut app, 0, 7);
    let layout = ctx(&mut app).model.scales().scale_layout();
    assert_eq!(layout.ordinal() % 2, 1, "vertical");
    assert_eq!(ctx(&mut app).config.scale_layout(), layout.name());
    let rows = text_rows(&mut app);
    assert!(rows[3].contains("Vertical"), "{rows:?}");
}

#[test]
fn quantize_and_groove_modes() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::RecArm);
    let amount = ctx(&mut app).config.quantize_amount();
    knob(&mut app, 7, 127);
    assert_eq!(ctx(&mut app).config.quantize_amount(), amount - 1);
    // Groove is temporary
    click(&mut app, 1, 1);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Groove));
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts[0].0, "Quantize");
    assert_eq!(texts[1].0, "Groove");
    // Back to quantize
    click(&mut app, 1, 0);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::RecArm));
}

#[test]
fn accent_mode() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::Accent);
    ctx(&mut app).config.set_fixed_accent_value(100);
    knob(&mut app, 7, 1);
    assert_eq!(ctx(&mut app).config.fixed_accent_value(), 101);
    knob(&mut app, 7, 127);
    knob(&mut app, 7, 127);
    assert_eq!(ctx(&mut app).config.fixed_accent_value(), 99);
    let rows = text_rows(&mut app);
    assert!(rows[0].ends_with("Accent  "), "{rows:?}");
}

#[test]
fn metronome_mode() {
    let (mut app, backend) = start(Model::Push2);
    activate(&mut app, Modes::Transport);
    backend.take_calls();
    click(&mut app, 0, 2);
    click(&mut app, 0, 5);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::IniFilePrerollMeasures { value: 2 })), "{calls:?}");
    assert!(calls.has_cmd(|c| matches!(c, Cmd::TransportTogglePrerollMetronome {})), "{calls:?}");
    let components = components(&mut app);
    assert!(matches!(components[7], Component::Parameter(_)));
    let texts = option_texts(&components);
    assert_eq!(texts[0].1, "None");
    assert_eq!(texts[3].1, "4 Bars");
}

#[test]
fn marker_mode() {
    let (mut app, backend) = start(Model::Push2);
    activate(&mut app, Modes::Markers);
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts[0].1, "Start");
    assert_eq!(texts[1].1, "Chorus");
    assert_eq!(texts[7].0, "Launch");
    backend.take_calls();
    click(&mut app, 0, 1);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::MarkerAction { marker: 1, .. })), "{calls:?}");
    click(&mut app, 1, 0);
    assert!(backend.take_calls().has_cmd(|c| matches!(c, Cmd::MarkerAdd {})));
    // Select instead of launch
    click(&mut app, 1, 6);
    let texts = option_texts(&components(&mut app));
    assert!(texts[6].0 == "Select");
}

#[test]
fn automation_mode() {
    let (mut app, backend) = start(Model::Push1);
    activate(&mut app, Modes::Automation);
    let rows = text_rows(&mut app);
    assert!(rows[1].starts_with("Automation Mode:"), "{rows:?}");
    backend.take_calls();
    click(&mut app, 0, 1);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::AutomationMode { .. })), "{calls:?}");
}

#[test]
fn frame_mode() {
    let (mut app, backend) = start(Model::Push1);
    activate(&mut app, Modes::Frame);
    let rows = text_rows(&mut app);
    assert!(rows[0].starts_with("Layouts:"), "{rows:?}");
    backend.take_calls();
    click(&mut app, 0, 1);
    let calls = backend.take_calls();
    assert!(calls.has_cmd(|c| matches!(c, Cmd::ApplicationLayout { .. })), "{calls:?}");
}

#[test]
fn session_mode() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Session);
    let components = components(&mut app);
    assert_eq!(components.len(), 8);
    assert!(components.iter().all(|c| matches!(c, Component::ClipList(_))), "{components:?}");
    // Scenes
    config::set_scene_view(&mut ctx(&mut app).config);
    let components = self::components(&mut app);
    assert!(components.iter().all(|c| matches!(c, Component::SceneList(_))), "{components:?}");
    // Upper / lower rows
    button(&mut app, 1, 0, ButtonEvent::Down);
    let hi = button_color(&mut app, ButtonId::Row21);
    let on = button_color(&mut app, ButtonId::Row22);
    assert_ne!(hi, on);
}

#[test]
fn session_mode_push1_display() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::Session);
    let rows = text_rows(&mut app);
    // The selected track 0 with the clip "Intro" in the first slot
    assert!(rows[0].contains("Intro"), "{rows:?}");
}

#[test]
fn session_view_select_mode() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Session);
    ctx(&mut app).set_temporary_mode(Modes::SessionViewSelect);
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts[5].1, "Scenes/Clips");
    click(&mut app, 0, 1);
    assert!(ctx(&mut app).config.is_flip_session());
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Session));
    ctx(&mut app).set_temporary_mode(Modes::SessionViewSelect);
    click(&mut app, 0, 2);
    assert!(config::is_scenes_clip_view_selected(&ctx(&mut app).config));
    click(&mut app, 0, 6);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Markers));
}

#[test]
fn view_select_mode() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Session);
    activate(&mut app, Modes::ViewSelect);
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts.len(), 8);
    assert_eq!(texts[0].0.is_empty(), !ctx(&mut app).views.contains(drivenbymoss::framework::ids::Views::Sequencer));
    click(&mut app, 0, 4);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Session), "restored");
}

#[test]
fn note_repeat_mode() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::RepeatNote);
    let components = components(&mut app);
    assert!(matches!(components[1], Component::List { .. }));
    let period = ctx(&mut app).config.note_repeat_period();
    // The knob is slowed down
    for _ in 0..8 {
        knob(&mut app, 0, 1);
    }
    assert_ne!(ctx(&mut app).config.note_repeat_period(), period);
}

#[test]
fn note_mode_without_notes() {
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Note);
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts.len(), 8);
    let (mut app1, _) = start(Model::Push1);
    activate(&mut app1, Modes::Note);
    let rows = text_rows(&mut app1);
    assert!(rows[1].contains("Please selecta note"), "{rows:?}");
    // The note editor is available for the sequencers
    let mode = ctx(&mut app).modes.get(Modes::Note).unwrap();
    assert!(mode.borrow().note_editor().is_some());
}

#[test]
fn configuration_modes() {
    let (mut app, _) = start(Model::Push1);
    activate(&mut app, Modes::Configuration);
    let threshold = config::pad_threshold_push1(&ctx(&mut app).config);
    knob(&mut app, 0, 1);
    assert_eq!(config::pad_threshold_push1(&ctx(&mut app).config), threshold + 1);
    let rows = text_rows(&mut app);
    assert!(rows[0].starts_with("Pad Threshold"), "{rows:?}");

    let (mut app, _) = start(Model::Push3);
    activate(&mut app, Modes::Setup);
    let brightness = config::display_brightness(&ctx(&mut app).config);
    knob(&mut app, 1, 127);
    assert!(config::display_brightness(&ctx(&mut app).config) < brightness);
    assert_eq!(components(&mut app).len(), 8);
    // The MPE page
    click(&mut app, 1, 2);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Configuration));
    knob(&mut app, 1, 127);
    assert!(!ctx(&mut app).config.is_mpe_enabled());
    // The audio page
    click(&mut app, 1, 3);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Audio));
    knob(&mut app, 0, 1);
    assert_eq!(config::pedal1(&ctx(&mut app).config), 1);

    // Push 2 has only info and setup
    let (mut app, _) = start(Model::Push2);
    activate(&mut app, Modes::Info);
    let texts = option_texts(&components(&mut app));
    assert_eq!(texts[2].0, " ");
    click(&mut app, 1, 3);
    run_tasks(&mut app);
    assert!(ctx(&mut app).modes.is_active(Modes::Info));
}

/// Activate every mode, press every button of the two rows and turn and touch every knob.
#[test]
fn touch_everything() {
    for model in [Model::Push1, Model::Push2, Model::Push3] {
        let (mut app, _) = start(model);
        for mode in MISC_MODES.iter().copied().chain(model.configuration_modes()) {
            for row in 0..2 {
                for index in 0..8 {
                    activate(&mut app, mode);
                    for event in [ButtonEvent::Down, ButtonEvent::Long, ButtonEvent::Up] {
                        button(&mut app, row, index, event);
                    }
                    for id in [ButtonId::get(ButtonId::Row11, index), ButtonId::get(ButtonId::Row21, index)] {
                        button_color(&mut app, id);
                    }
                    app.tick();
                }
            }
            activate(&mut app, mode);
            for index in 0..8 {
                let ctx = ctx(&mut app);
                let active = ctx.active_mode().unwrap();
                let mut active = active.borrow_mut();
                active.on_knob_touch(ctx, index, true);
                active.on_knob_value(ctx, index, 1);
                active.on_knob_value(ctx, index, 127);
                active.knob_value(ctx, index);
                active.on_knob_touch(ctx, index, false);
                active.update_display(ctx);
            }
            app.tick();
        }
    }
}
