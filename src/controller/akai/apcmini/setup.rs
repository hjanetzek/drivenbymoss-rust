// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the APC mini Mk1 and Mk2 (former `APCminiControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The pads are created here (not with `Ctx::create_pads`) since the Mk2 adds the pad
//!   brightness to the MIDI channel of the pad LEDs.
//! - The shift view switches the views after the pad event (the shift view is borrowed while it
//!   handles the pad).
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::colors::{create_mk1_color_manager, create_mk2_color_manager, mk1};
use super::definition::{ApcMiniButton, ApcMiniDefinition};
use super::surface::{self as apc, ApcMiniState, APC_KNOB_MASTER_LEVEL, APC_KNOB_TRACK_LEVEL1};
use super::view::{as_mini_view, as_mini_view_mut, ApcMiniBrowserView, DrumView, PlayView, RaindropsView, SequencerView, SessionView, ShiftView};
use crate::backend::Sender;
use crate::config::{apcmini as config, Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_ON};
use crate::framework::command::{shared, FnTrigger};
use crate::framework::commands::mode::KnobRowModeCommand;
use crate::framework::commands::view::{ToggleShiftViewCommand, ViewButtonCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::mode::device::ParameterMode;
use crate::framework::mode::parameter_mode::sequential_controls;
use crate::framework::mode::track::{TrackPanMode, TrackSendMode, TrackVolumeMode};
use crate::framework::parameterprovider::ParamRef;
use crate::framework::setup_helpers::{self, BrowserObserver};
use crate::framework::surface::{ContinuousKind, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The labels of the track buttons.
const ROW_NAMES: [&str; 8] =
    ["Track 1\nUp", "Track 2\nDown", "Track 3\nLeft", "Track 4\nRight", "Track 5\nVolume", "Track 6\nPan", "Track 7\nSend", "Track 8\nDevice"];

/// The labels of the scene buttons.
const COL_NAMES: [&str; 8] =
    ["Scene 1\n Clip Stop", "Scene 2\nSolo", "Scene 3\nRec Arm", "Scene 4\nMute", "Scene 5\nSelect", "Scene 6\n-", "Scene 7\n-", "Scene 8\nStop All Clips"];

/// The modes of the fader control setting (former `FADER_CTRL_MODES`).
pub fn fader_ctrl_mode(option: &str) -> Option<Modes> {
    Some(match option {
        "Volume" => Modes::Volume,
        "Pan" => Modes::Pan,
        "Send 1" => Modes::Send1,
        "Send 2" => Modes::Send2,
        "Send 3" => Modes::Send3,
        "Send 4" => Modes::Send4,
        "Send 5" => Modes::Send5,
        "Send 6" => Modes::Send6,
        "Send 7" => Modes::Send7,
        "Send 8" => Modes::Send8,
        "Device" => Modes::DeviceParams,
        _ => return None,
    })
}

/// The faders (former `FADER_IDS`).
fn fader_ids() -> Vec<ContinuousId> {
    sequential_controls(ContinuousId::Fader1, 8)
}

/// Create an APC mini instance.
pub fn create(definition: ApcMiniDefinition, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let colors = if definition.has_rgb_colors() { create_mk2_color_manager() } else { create_mk1_color_manager() };
    let value_changer = ValueChanger::twos_complement(128, 1);

    // createScales (former APCminiScales)
    let scales = Scales::new(&value_changer, 36, 100, 8, 8).with_grid_translator(Scales::offset_grid_translator(36, 100));

    // createModel
    let model_setup = ModelSetup::new();
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &[])?;
    let surface = Surface::new(output, input, Some(apc::create_pad_grid(&colors)));

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    ctx.ext = Box::new(ApcMiniState::new(definition));
    apc::install_trigger_sender(&mut ctx);
    apc::create_pads(&mut ctx);

    Ok(InstanceParts { ctx, setup: Box::new(ApcMiniSetup::default()) })
}

/// The APC mini controller setup.
#[derive(Default)]
pub struct ApcMiniSetup {
    browser_observer: BrowserObserver,
}

impl ControllerSetup for ApcMiniSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx);
        create_views(ctx);
        register_trigger_commands(ctx);
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        ctx.set_active_mode(Modes::Volume);
        let view = if ctx.config.should_start_with_session_view() { Views::Session } else { ctx.config.startup_view() };
        ctx.set_active_view(view);
        ctx.schedule(1000, |ctx| ctx.surface.grid().force_flush());
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

        self.browser_observer.check_view(ctx, Views::Browser);
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        ctx.turn_off_lights();
    }
}

/// React on a changed setting (former `createObservers`).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    if setup_helpers::handle_scale_setting(ctx, id) || setup_helpers::handle_deactivated_items_setting(ctx, id) {
        return;
    }
    match id {
        config::FADER_CTRL => {
            if let Some(mode) = fader_ctrl_mode(&config::fader_ctrl(&ctx.config)) {
                ctx.set_active_mode(mode);
            }
        }
        config::SOFT_KEYS => {
            let soft_keys = config::soft_keys(&ctx.config);
            if let Some(index) = config::SOFT_KEYS_OPTIONS.iter().position(|o| *o == soft_keys) {
                apc::set_track_state(ctx, index);
            }
        }
        // Former surface::forceFlush
        config::PAD_BRIGHTNESS => ctx.surface.clear_cache(),
        _ => {}
    }
}

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    let faders = fader_ids();
    let mode = TrackVolumeMode::new_with_controls(ctx, true, faders.clone());
    ctx.register_mode(Modes::Volume, mode);
    let mode = TrackPanMode::new_with_controls(ctx, true, faders.clone());
    ctx.register_mode(Modes::Pan, mode);
    for i in 0..8 {
        let mode = TrackSendMode::new_with_controls(i as i32, ctx, true, faders.clone());
        ctx.register_mode(Modes::get(Modes::Send1, i), mode);
    }
    let mode = ParameterMode::new(ctx, true, faders);
    ctx.register_mode(Modes::DeviceParams, mode);
    ctx.modes.set_default_id(Modes::Volume);
}

/// Register the views (former `createViews`).
pub fn create_views(ctx: &mut Ctx) {
    let definition = apc::definition(ctx);
    let use_daw_colors = definition.has_rgb_colors();
    ctx.register_view(Views::Play, PlayView::new(use_daw_colors));
    ctx.register_view(Views::Shift, ShiftView::new(definition));
    ctx.register_view(Views::Browser, ApcMiniBrowserView::new());
    ctx.register_view(Views::Session, SessionView::new(use_daw_colors));
    ctx.register_view(Views::Sequencer, SequencerView::new(use_daw_colors, definition));
    ctx.register_view(Views::Drum, DrumView::new(use_daw_colors, definition));
    ctx.register_view(Views::Raindrops, RaindropsView::new(use_daw_colors, definition));
}

/// The color of a track button from the active view (former track button supplier). Track
/// buttons are only red!
fn track_button_color(ctx: &Ctx, index: usize) -> i32 {
    let Some(view) = ctx.active_view() else {
        return mk1::APC_COLOR_BLACK;
    };
    let Ok(view) = view.try_borrow() else {
        return mk1::APC_COLOR_BLACK;
    };
    match as_mini_view(&*view) {
        Some(mini_view) => {
            if mini_view.track_button_color(ctx, index) > 0 {
                mk1::APC_COLOR_RED
            } else {
                0
            }
        }
        None => mk1::APC_COLOR_BLACK,
    }
}

/// Forward a track button to the active view (former `TrackSelectCommand`).
fn on_select_track(ctx: &mut Ctx, index: usize, event: ButtonEvent) {
    let Some(view) = ctx.active_view() else {
        return;
    };
    let Ok(mut view) = view.try_borrow_mut() else {
        return;
    };
    if let Some(mini_view) = as_mini_view_mut(&mut *view) {
        mini_view.on_select_track(ctx, index, event);
    }
}

/// Register the commands of all buttons (former `registerTriggerCommands`). All buttons are
/// bound to notes (former `getTriggerBindType`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    let definition = apc::definition(ctx);

    // Without a supplier the light shows BUTTON_STATE_ON if not pressed and BUTTON_STATE_HI if
    // pressed (former addButton with the default color IDs)
    ctx.add_button(
        ButtonId::Shift,
        "Shift",
        Some(shared(ToggleShiftViewCommand::new())),
        BindType::Note,
        0,
        definition.button_id(ApcMiniButton::Shift),
        Some(Rc::new(|ctx: &Ctx| ctx.colors.color_index(if ctx.surface.is_pressed(ButtonId::Shift) { BUTTON_STATE_HI } else { BUTTON_STATE_ON }))),
    );

    for i in 0..8 {
        let scene = ButtonId::get(ButtonId::Scene1, i);
        ctx.add_button(
            scene,
            COL_NAMES[i],
            Some(shared(ViewButtonCommand::new(scene))),
            BindType::Note,
            0,
            definition.button_id(ApcMiniButton::Scene(i)),
            Some(Rc::new(move |ctx: &Ctx| ctx.button_color_from_active_view(scene))),
        );
        ctx.add_button(
            ButtonId::get(ButtonId::RowSelect1, i),
            ROW_NAMES[i],
            Some(shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| on_select_track(ctx, i, event)))),
            BindType::Note,
            0,
            definition.button_id(ApcMiniButton::Track(i)),
            Some(Rc::new(move |ctx: &Ctx| track_button_color(ctx, i))),
        );
    }
}

/// Register the faders (former `registerContinuousCommands`).
pub fn register_continuous_commands(ctx: &mut Ctx) {
    let master = ctx.surface.create_continuous(ContinuousId::FaderMaster, "Master", ContinuousKind::Fader);
    master.command = None;
    ctx.surface.bind_continuous(ContinuousId::FaderMaster, BindType::Cc, 0, APC_KNOB_MASTER_LEVEL);
    ctx.surface.bind_continuous_parameter(ContinuousId::FaderMaster, Some(ParamRef::MasterVolume));

    for i in 0..8 {
        ctx.add_continuous(
            ContinuousId::get(ContinuousId::Fader1, i),
            &format!("Fader {}", i + 1),
            ContinuousKind::Fader,
            Rc::new(RefCell::new(KnobRowModeCommand::new(i))),
            BindType::Cc,
            0,
            APC_KNOB_TRACK_LEVEL1 + i as i32,
        );
    }
}
