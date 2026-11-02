// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Akai Fire (former `FireControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The display is rendered and sent in `flush` after the active mode added its components
//!   (Java: the modes call `display.send ()` in `updateDisplay`).
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::colors::{create_color_manager, BUTTON_STATE_HI2, BUTTON_STATE_ON2};
use super::command::*;
use super::mode::{FireBrowserMode, FireChannelMode, FireNoteMode, FireParameterMode, FireUserMode};
use super::surface::{self as fire, *};
use super::view::{self as fire_view, Drum4View, Drum64View, MixView, PlayView, PolySequencerView, SequencerView, SessionView, ShiftView};
use crate::backend::Sender;
use crate::config::{fire as config, Configuration, SettingId};
use crate::controller::akai::{bool_light, state_light, ON_HI};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::bank::BankItem;
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::command::{shared, FnTrigger, SharedTrigger};
use crate::framework::commands::mode::KnobRowModeCommand;
use crate::framework::commands::transport::{MetronomeCommand, PlayCommand};
use crate::framework::commands::view::{ToggleShiftViewCommand, ViewButtonCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::setup_helpers::{self, BrowserObserver};
use crate::framework::surface::{ContinuousKind, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};

/// The knob modes of the BANK button (former `MODES`).
const MODES: [Modes; 4] = [Modes::DeviceLayer, Modes::Track, Modes::DeviceParams, Modes::User];

/// The default colors of a light with an integer state (former `addButton` with an
/// `IntSupplier` and without color IDs).
const OFF_ON_HI: &[&str] = &[BUTTON_STATE_OFF, BUTTON_STATE_ON, BUTTON_STATE_HI];

/// Create a Fire instance.
pub fn create(host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let colors = create_color_manager();
    let value_changer = ValueChanger::twos_complement(1024, 10);
    let scales = fire::create_scales(&value_changer);
    let upper_bound = value_changer.upper_bound();

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.enable_device(crate::daw::constants::DeviceId::FirstInstrument);
    model_setup.wants_focused_parameter = true;
    model_setup.set_num_tracks(16);
    model_setup.set_num_scenes(4);
    model_setup.set_num_sends(6);
    model_setup.set_num_drum_pad_layers(4);
    model_setup.num_device_layers = 4;
    model_setup.num_filter_column_entries = 3;
    model_setup.set_num_results(3);
    model_setup.set_has_full_flat_track_list(true);
    model_setup.set_additional_drum_devices(vec![64, 16]);
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &["80????", "90????"])?;
    let surface = Surface::new(output, input, Some(fire::create_pad_grid(&colors)));

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    ctx.ext = Box::new(FireState::default());
    ctx.graphics_display = Some(fire::create_display(upper_bound));
    fire::install_trigger_sender(&mut ctx);
    ctx.create_pads();
    fire::configure_leds(&mut ctx);

    Ok(InstanceParts { ctx, setup: Box::new(FireSetup::default()) })
}

/// The Fire controller setup.
#[derive(Default)]
pub struct FireSetup {
    browser_observer: BrowserObserver,
    mode_select_command: Option<Rc<RefCell<FireModeCommand>>>,
    last_view: Option<Views>,
}

impl ControllerSetup for FireSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx);
        create_views(ctx);
        self.mode_select_command = Some(register_trigger_commands(ctx));
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let view = if ctx.config.should_start_with_session_view() { Views::Session } else { ctx.config.startup_view() };
        // The XoX drum view is not ported
        let view = if ctx.views.get(view).is_some() { view } else { Views::Play };
        ctx.set_active_view(view);
        ctx.set_active_mode(Modes::Track);
        if let Some(command) = &self.mode_select_command {
            command.borrow_mut().multi_select.activate_mode(ctx, Modes::Track);
        }
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
        ctx.update_active_mode_display();
        fire::send_display(ctx);
        fire::flush_pads(ctx);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        // Model events (former track bank selection observer and the observers of the views)
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for event in &events {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                setup_helpers::handle_track_change(ctx, *is_selected);
            }
        }

        let changed = ctx.config.take_changed_settings();
        for id in &changed {
            handle_setting_change(ctx, *id);
        }
        ctx.dispatch_setting_changes_to_views(&changed);

        self.browser_observer.check_mode(ctx, Modes::Browser);

        // Former onViewChange
        let active_view = ctx.views.active_id();
        if active_view != self.last_view {
            self.last_view = active_view;
            if let Some(display) = ctx.graphics_display.as_mut() {
                display.cancel_notification();
            }
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        fire::shutdown(ctx);
        ctx.turn_off_lights();
    }
}

/// React on a changed setting (former `createObservers`).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    if setup_helpers::handle_scale_setting(ctx, id) || setup_helpers::handle_deactivated_items_setting(ctx, id) || setup_helpers::handle_note_repeat_setting(ctx, id) {
        return;
    }
    if id == config::PAD_BRIGHTNESS || id == config::PAD_SATURATION {
        fire::configure_leds(ctx);
    }
}

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    let mode = FireChannelMode::layer(ctx);
    ctx.register_mode(Modes::DeviceLayer, mode);
    let mode = FireChannelMode::layer_mixer(ctx);
    ctx.register_mode(Modes::DeviceLayerVolume, mode);
    let mode = FireChannelMode::track(ctx);
    ctx.register_mode(Modes::Track, mode);
    let mode = FireChannelMode::track_mixer(ctx);
    ctx.register_mode(Modes::Volume, mode);
    let mode = FireParameterMode::new(ctx);
    ctx.register_mode(Modes::DeviceParams, mode);
    let mode = FireUserMode::new(ctx);
    ctx.register_mode(Modes::User, mode);
    ctx.register_mode(Modes::Browser, FireBrowserMode::new());

    // Note mode needs the ALT button to exist
    ctx.add_button(ButtonId::Alt, "ALT", Some(shared(FnTrigger(|ctx: &mut Ctx, _event, _velocity| on_alt(ctx)))), BindType::Note, 0, FIRE_ALT, None);
    ctx.register_mode(Modes::Note, FireNoteMode::new());
    ctx.modes.set_default_id(Modes::Track);
}

/// Register the views (former `createViews`).
pub fn create_views(ctx: &mut Ctx) {
    ctx.register_view(Views::Sequencer, SequencerView::new());
    ctx.register_view(Views::PolySequencer, PolySequencerView::new(true));
    ctx.register_view(Views::Play, PlayView::new());
    ctx.register_view(Views::Piano, PlayView::new_piano());
    ctx.register_view(Views::Drum4, Drum4View::new());
    ctx.register_view(Views::Drum64, Drum64View::new());
    ctx.register_view(Views::Session, SessionView::new());
    ctx.register_view(Views::Mix, MixView);
    ctx.register_view(Views::Shift, ShiftView::new());
}

fn add(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, note: i32, supplier: Option<crate::framework::surface::LightSupplier>) {
    let supplier = supplier.unwrap_or_else(|| bool_light(move |ctx| ctx.surface.is_pressed(id)));
    ctx.add_button(id, label, Some(command), BindType::Note, 0, note, Some(supplier));
}

/// Register the commands of all buttons and the LEDs (former `registerTriggerCommands`). All
/// buttons are bound to notes (former `getTriggerBindType`), the LEDs are set with CC.
pub fn register_trigger_commands(ctx: &mut Ctx) -> Rc<RefCell<FireModeCommand>> {
    // Knob Modes
    let mode_select_command = Rc::new(RefCell::new(FireModeCommand::new(&MODES)));
    add(ctx, ButtonId::BankRight, "BANK", mode_select_command.clone(), FIRE_BANK, None);
    for (i, mode) in MODES.iter().copied().enumerate() {
        ctx.surface.create_light(
            Some(led(i)),
            None,
            Rc::new(move |ctx: &Ctx| if ctx.modes.active_id() == Some(mode) { i as i32 } else { 5 }),
            Rc::new(|ctx: &mut Ctx, color| {
                if color < 5 {
                    ctx.surface.output.send_cc_ex(0, CC_KNOB_MODE_LEDS, color);
                }
            }),
        );
    }

    // Transport
    add(ctx, ButtonId::Play, "PLAY", shared(PlayCommand::with_select_button(ButtonId::Alt)), FIRE_PLAY, Some(bool_light(|ctx| ctx.model.transport().is_playing())));
    add(ctx, ButtonId::Stop, "STOP", shared(FireStopCommand::default()), FIRE_STOP, Some(bool_light(|ctx| !ctx.model.transport().is_playing())));
    add(
        ctx,
        ButtonId::Record,
        "REC",
        shared(FireRecordCommand::new()),
        FIRE_REC,
        Some(state_light(&[BUTTON_STATE_ON, BUTTON_STATE_HI, BUTTON_STATE_ON2, BUTTON_STATE_HI2], |ctx| {
            if is_record_shifted(ctx) {
                if ctx.model.is_launcher_overdub() {
                    3
                } else {
                    2
                }
            } else {
                i32::from(ctx.model.transport().is_recording())
            }
        })),
    );
    add(
        ctx,
        ButtonId::Metronome,
        "PATTERN/\nSONG",
        shared(MetronomeCommand::new(false)),
        FIRE_PATTERN,
        Some(state_light(&[BUTTON_STATE_ON2, BUTTON_STATE_HI2], |ctx| {
            let transport = ctx.model.transport();
            i32::from(if ctx.surface.is_shift_pressed() { transport.is_metronome_ticks_on() } else { transport.is_metronome_on() })
        })),
    );

    // Views + state toggles
    const VIEW_COLORS: &[&str] = &[BUTTON_STATE_ON2, BUTTON_STATE_HI2, BUTTON_STATE_ON, BUTTON_STATE_HI];
    for (id, label, kind, note) in [
        (ButtonId::Sequencer, "STEP", ViewSelect::Step, FIRE_STEP),
        (ButtonId::Note, "NOTE", ViewSelect::Play, FIRE_NOTE),
        (ButtonId::Drum, "DRUM", ViewSelect::Drum, FIRE_DRUM),
    ] {
        add(ctx, id, label, shared(FireViewSelectCommand::new(kind)), note, Some(state_light(VIEW_COLORS, move |ctx| FireViewSelectCommand::view_activation_color(ctx, kind))));
    }
    add(ctx, ButtonId::Session, "PERFORM", shared(SessionSelectCommand::new()), FIRE_PERFORM, Some(state_light(VIEW_COLORS, SessionSelectCommand::view_activation_color)));
    add(
        ctx,
        ButtonId::Shift,
        "SHIFT",
        shared(ToggleShiftViewCommand::new()),
        FIRE_SHIFT,
        Some(state_light(&[BUTTON_STATE_ON2, BUTTON_STATE_HI2], |ctx| i32::from(ctx.views.is_active(Views::Shift) || ctx.surface.is_shift_pressed()))),
    );
    add(ctx, ButtonId::Select, "SELECT", shared(FireSelectButtonCommand), SELECT, None);
    add(ctx, ButtonId::Browse, "BROWSER", shared(FireBrowserCommand::new()), FIRE_BROWSER, Some(bool_light(|ctx| ctx.modes.is_active(Modes::Browser))));

    // Navigation
    add(ctx, ButtonId::ArrowLeft, "GRID\nLEFT", shared(FireViewButtonCommand::new(ButtonId::ArrowLeft)), FIRE_GRID_LEFT, None);
    add(ctx, ButtonId::ArrowRight, "GRID\nRIGHT", shared(FireViewButtonCommand::new(ButtonId::ArrowRight)), FIRE_GRID_RIGHT, None);
    for (id, label, note, is_up) in [(ButtonId::ArrowUp, "PATTERN\nUP", FIRE_PAT_UP, true), (ButtonId::ArrowDown, "PATTERN\nDOWN", FIRE_PAT_DOWN, false)] {
        let command = FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
            if event != ButtonEvent::Up {
                return;
            }
            if is_alt_pressed(ctx) {
                if is_up {
                    ctx.model.application_mut().redo();
                    ctx.notify("Redo");
                } else {
                    ctx.model.application_mut().undo();
                    ctx.notify("Undo");
                }
                return;
            }
            select_slot(ctx, is_up);
        });
        add(ctx, id, label, shared(command), note, None);
    }

    for i in 0..4 {
        let scene = ButtonId::get(ButtonId::Scene1, i);
        add(ctx, scene, &format!("SOLO{}", i + 1), shared(ViewButtonCommand::new(scene)), FIRE_SOLO_1 + i as i32, Some(state_light(OFF_ON_HI, move |ctx| ctx.button_color_from_active_view(scene))));
        ctx.surface.create_light(
            Some(led(4 + i)),
            None,
            Rc::new(move |ctx: &Ctx| fire_view::solo_button_color(ctx, i)),
            Rc::new(move |ctx: &mut Ctx, color| ctx.surface.output.send_cc_ex(0, CC_SOLO_LEDS + i as i32, color)),
        );
    }
    let _ = ON_HI;
    mode_select_command
}

/// Select the next or previous slot of the cursor track (former `selectNextItem` and
/// `selectPreviousItem` of the slot bank; the page is not scrolled).
fn select_slot(ctx: &mut Ctx, is_next: bool) {
    let slot_bank = ctx.model.cursor_track().slot_bank();
    let page_size = slot_bank.page_size();
    let index = match slot_bank.selected_item().map(|slot| slot.index().max(0) as usize % page_size.max(1)) {
        Some(selected) if is_next => selected + 1,
        Some(selected) => selected.wrapping_sub(1),
        None => 0,
    };
    if index < page_size {
        ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index).select();
    }
}

/// Register the knobs (former `registerContinuousCommands`).
pub fn register_continuous_commands(ctx: &mut Ctx) {
    for i in 0..4 {
        let id = ContinuousId::get(ContinuousId::Knob1, i);
        ctx.add_continuous(id, &format!("Knob {i}"), ContinuousKind::RelativeKnob, Rc::new(RefCell::new(KnobRowModeCommand::new(i))), BindType::Cc, 0, CC_VOLUME + i as i32);
        // Former KnobRowTouchModeCommand
        let touch = FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
            if event == ButtonEvent::Long {
                return;
            }
            let is_touched = event == ButtonEvent::Down;
            with_active_mode(ctx, |mode, ctx| mode.on_knob_touch(ctx, i, is_touched));
        });
        if let Some(control) = ctx.surface.continuous_mut(id) {
            control.touch_command = Some(shared(touch));
        }
        ctx.surface.input.bind_touch(id, BindType::Note, 0, TOUCH_VOLUME + i as i32);
    }
    ctx.add_continuous(ContinuousId::ViewSelection, "Select", ContinuousKind::RelativeKnob, Rc::new(RefCell::new(SelectKnobCommand::default())), BindType::Cc, 0, CC_SELECT);
}
