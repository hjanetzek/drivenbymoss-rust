// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Arturia Beatstep and Beatstep Pro (former `BeatstepControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The pads are created with the sysex sender of the pad grid (`surface::create_pads`).
//! - The aftertouch command of the play view is created by the view.
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::colors::create_color_manager;
use super::command::KnobRowViewCommand;
use super::definition::{BEATSTEP_CHANNEL, BEATSTEP_KNOB_1, BEATSTEP_KNOB_9, BEATSTEP_KNOB_MAIN, BEATSTEP_SHIFT, SYSEX_ENABLE_SHIFT};
use super::surface;
use super::view::{BrowserView, DeviceView, DrumView, PlayView, SequencerView, SessionView, ShiftView, TrackView};
use crate::backend::Sender;
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_ON};
use crate::framework::command::{shared, FnTrigger};
use crate::framework::commands::channel::PlayPositionCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Views};
use crate::framework::setup_helpers::{self, BrowserObserver};
use crate::framework::surface::{ContinuousKind, Surface};
use crate::framework::value_changer::{RelativeEncoding, ValueChanger};
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The drum matrix: the 16 pads, the rest is off (former `DRUM_MATRIX`).
pub const DRUM_MATRIX: [i32; 64] = {
    let mut matrix = [-1; 64];
    let mut i = 0;
    while i < 16 {
        matrix[i] = i as i32;
        i += 1;
    }
    matrix
};

/// Create a Beatstep instance.
pub fn create(host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let colors = create_color_manager();
    let value_changer = ValueChanger::new(RelativeEncoding::OffsetBinary, 128, 1);

    // createScales
    let mut scales = Scales::new(&value_changer, 36, 52, 8, 2);
    scales.set_drum_matrix(DRUM_MATRIX.to_vec());
    scales.set_drum_note_end(52);

    // createModel
    let model_setup = ModelSetup::new();
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface: the pads and their aftertouch on MIDI channel 3
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let mut input = MidiInput::new(sender.clone(), Some(ports.input), &["82????", "92????", "A2????"])?;
    // Sequencer 1 is on channel 1
    let sequencer_input = input.create_note_input(&["90????", "80????"])?;
    // Block the Shift key
    let table: Vec<i32> = (0..128).map(|i| if i == BEATSTEP_SHIFT { -1 } else { i }).collect();
    if let Some(note_input) = input.note_input(sequencer_input) {
        note_input.set_key_translation_table(&table);
    }
    let surface = Surface::new(output, input, Some(surface::create_pad_grid(&colors)));

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    surface::create_pads(&mut ctx);

    Ok(InstanceParts { ctx, setup: Box::new(BeatstepSetup::default()) })
}

/// The Beatstep controller setup.
#[derive(Default)]
pub struct BeatstepSetup {
    browser_observer: BrowserObserver,
}

impl ControllerSetup for BeatstepSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_views(ctx);
        create_observers(ctx);
        register_trigger_commands(ctx);
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        // Enable Shift button to send MIDI Note 07
        ctx.surface.output.send_sysex(&SYSEX_ENABLE_SHIFT);
        ctx.set_active_view(Views::Track);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        // Model events (former track bank selection observer and the observers of the views)
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for event in &events {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                handle_track_change(ctx, *is_selected);
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

/// Former `handleTrackChange` with the overridden `recallLastView`, which does nothing: a view
/// change on a track change is confusing on the Beatstep since it leaves the track view.
fn handle_track_change(ctx: &mut Ctx, is_selected: bool) {
    if !is_selected {
        return;
    }
    setup_helpers::reset_drum_octave(ctx);
    setup_helpers::update_mode(ctx);
}

/// React on a changed setting (former `createScaleObservers` and
/// `registerDeactivatedItemsHandler`).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    if !setup_helpers::handle_scale_setting(ctx, id) {
        setup_helpers::handle_deactivated_items_setting(ctx, id);
    }
}

/// Register the views (former `createViews`).
pub fn create_views(ctx: &mut Ctx) {
    ctx.register_view(Views::Track, TrackView::new());
    ctx.register_view(Views::Device, DeviceView::new());
    ctx.register_view(Views::Play, PlayView::new());

    ctx.register_view(Views::Drum, DrumView::new());
    ctx.register_view(Views::Sequencer, SequencerView::new());
    ctx.register_view(Views::Session, SessionView::new());

    ctx.register_view(Views::Browser, BrowserView::new());
    ctx.register_view(Views::Shift, ShiftView::new());
}

/// Former `createObservers`: the view change listener (the others are in `handle_changes`).
fn create_observers(ctx: &mut Ctx) {
    ctx.views.add_change_listener(Rc::new(|ctx: &mut Ctx, _previous, _active| update_indication(ctx)));
}

/// Register the Shift button (former `registerTriggerCommands`). All buttons are bound to
/// notes (former `getTriggerBindType`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    let command = FnTrigger(|ctx: &mut Ctx, event, _velocity| {
        if event == ButtonEvent::Down {
            ctx.set_active_view(Views::Shift);
            return;
        }
        if event == ButtonEvent::Up {
            if ctx.views.is_active(Views::Shift) {
                ctx.restore_view();
            }
            // Red LED is turned off on button release, restore the correct color
            for note in 36..52 {
                surface::restore_pad_color(ctx, note);
            }
        }
    });
    // Without a supplier the light shows BUTTON_STATE_ON if not pressed and BUTTON_STATE_HI if
    // pressed (former addButton with the default color IDs)
    ctx.add_button(
        ButtonId::Shift,
        "Shift",
        Some(shared(command)),
        BindType::Note,
        0,
        BEATSTEP_SHIFT,
        Some(Rc::new(|ctx: &Ctx| ctx.colors.color_index(if ctx.surface.is_pressed(ButtonId::Shift) { BUTTON_STATE_HI } else { BUTTON_STATE_ON }))),
    );
}

/// Register the knobs (former `registerContinuousCommands`). The aftertouch command of the play
/// view is created with the view.
pub fn register_continuous_commands(ctx: &mut Ctx) {
    for i in 0..8 {
        ctx.add_continuous(
            ContinuousId::get(ContinuousId::Knob1, i),
            &format!("Knob {}", i + 1),
            ContinuousKind::RelativeKnob,
            Rc::new(RefCell::new(KnobRowViewCommand::new(i))),
            BindType::Cc,
            BEATSTEP_CHANNEL,
            BEATSTEP_KNOB_1 + i as i32,
        );
        ctx.add_continuous(
            ContinuousId::get(ContinuousId::DeviceKnob1, i),
            &format!("Knob {}", i + 9),
            ContinuousKind::RelativeKnob,
            Rc::new(RefCell::new(KnobRowViewCommand::new(i + 8))),
            BindType::Cc,
            BEATSTEP_CHANNEL,
            BEATSTEP_KNOB_9 + i as i32,
        );
    }

    ctx.add_continuous(
        ContinuousId::MasterKnob,
        "Master",
        ContinuousKind::RelativeKnob,
        Rc::new(RefCell::new(PlayPositionCommand::new())),
        BindType::Cc,
        BEATSTEP_CHANNEL,
        BEATSTEP_KNOB_MAIN,
    );
}

/// Set the indications of the controlled parameters (former `updateIndication`).
pub fn update_indication(ctx: &mut Ctx) {
    let is_track = ctx.views.is_active(Views::Track);
    let is_device = ctx.views.is_active(Views::Device);
    let is_effect = ctx.model.is_effect_track_bank_active();

    ctx.model.master_track_mut().set_volume_indication(!is_device);

    let selected = ctx.model.track_bank().selected_index();
    let tb = ctx.model.track_bank_mut();
    for i in 0..8.min(tb.page_size()) {
        let has_track_sel = selected == Some(i);
        let track = tb.item_mut(i);
        track.set_volume_indication(!is_effect && has_track_sel && !is_device);
        track.set_pan_indication(!is_effect && has_track_sel && !is_device);
        let send_bank = track.send_bank_mut();
        for j in 0..6.min(send_bank.page_size()) {
            send_bank.item_mut(j).set_indication(!is_effect && has_track_sel && is_track);
        }
    }

    if let Some(tbe) = ctx.model.effect_track_bank_mut() {
        let selected_fx = tbe.selected_index();
        for i in 0..8.min(tbe.page_size()) {
            let has_fx_track_sel = selected_fx == Some(i);
            let fx_track = tbe.item_mut(i);
            fx_track.set_volume_indication(is_effect && has_fx_track_sel && is_track);
            fx_track.set_pan_indication(is_effect && has_fx_track_sel && is_track);
        }
    }

    if let Some(parameter_bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
        for i in 0..8.min(parameter_bank.page_size()) {
            parameter_bank.item_mut(i).set_indication(is_device);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drum_matrix() {
        assert_eq!(DRUM_MATRIX[..16], (0..16).collect::<Vec<i32>>()[..]);
        assert!(DRUM_MATRIX[16..].iter().all(|n| *n == -1));
    }
}
