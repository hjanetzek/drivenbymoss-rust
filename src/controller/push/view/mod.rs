// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The views of the Push (former `controller.ableton.push.view`) and their registration (former
//! `PushControllerSetup.createViews`), plus the functions of the former `NoteRepeatSceneHelper`
//! and the drum pad selection which the drum views share.

pub mod chords_view;
pub mod drum64_view;
pub mod drum_lane_views;
pub mod drum_view;
pub mod drum_xox_view;
pub mod play_view;
pub mod poly_sequencer_view;
pub mod prg_change_view;
pub mod raindrops_view;
pub mod sequencer_view;
pub mod session_view;

use super::colors::{NOTE_REPEAT_LENGTH_HI, NOTE_REPEAT_LENGTH_OFF, NOTE_REPEAT_PERIOD_HI, NOTE_REPEAT_PERIOD_OFF};
use crate::daw::bank::BankItem;
use crate::daw::constants::Resolution;
use crate::framework::color_ids::abstract_session_view::{COLOR_SCENE, COLOR_SCENE_OFF, COLOR_SELECTED_SCENE};
use crate::framework::ctx::Ctx;
use crate::framework::feature::{View, BUTTON_COLOR_OFF};
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::mode::note_editor::ClipKey;
use crate::framework::view::base::{on_scene_button, scene_index};
use crate::framework::view::color_view::ColorView;
use crate::framework::view::scene_play_view::ScenePlayView;
use crate::framework::view::sequencer::clip_length_view::ClipLengthView;
use crate::midi::ButtonEvent;

/// Register all views (former `createViews`).
pub fn register_views(ctx: &mut Ctx) {
    ctx.register_view(Views::Play, play_view::PlayView::new_play());
    ctx.register_view(Views::Chords, chords_view::ChordsView::new());
    ctx.register_view(Views::Piano, play_view::PlayView::new_piano());
    let prg_change = prg_change_view::PrgChangeView::new(ctx);
    ctx.register_view(Views::PrgChange, prg_change);
    let clip_length = ClipLengthView::new(ctx, true);
    ctx.register_view(Views::ClipLength, clip_length);
    let color = ColorView::new(ctx);
    ctx.register_view(Views::Color, color);

    let session = session_view::SessionView::new(ctx);
    ctx.register_view(Views::Session, session);
    ctx.register_view(Views::Sequencer, sequencer_view::SequencerView::new());
    ctx.register_view(Views::PolySequencer, poly_sequencer_view::PolySequencerView::new(true));
    ctx.register_view(Views::Drum, drum_view::DrumView::new());
    let xox = drum_xox_view::DrumXoXView::new(ctx);
    ctx.register_view(Views::DrumXox, xox);
    ctx.register_view(Views::Drum4, drum_lane_views::Drum4View::new());
    ctx.register_view(Views::Drum8, drum_lane_views::Drum8View::new());
    ctx.register_view(Views::Raindrops, raindrops_view::RaindropsView::new());
    let scene_play = ScenePlayView::new(ctx);
    ctx.register_view(Views::ScenePlay, scene_play);

    ctx.register_view(Views::Drum64, drum64_view::Drum64View::new());
}

/// The key of the clip of the active view if it is a sequencer (former `instanceof
/// AbstractSequencerView && !(instanceof ClipLengthView)` + `getClip`), used by the page buttons.
pub fn sequencer_clip_key(view: &dyn View) -> Option<ClipKey> {
    view.sequencer_clip_key()
}

// ----------------------------------------------------------------------------------------------
// Former NoteRepeatSceneHelper

/// Former `NoteRepeatSceneHelper.getButtonColorID`.
pub fn note_repeat_button_color_id(ctx: &Ctx, button: ButtonId) -> &'static str {
    let Some(index) = scene_index(button) else {
        return BUTTON_COLOR_OFF;
    };
    let scene_index = 7 - index;
    // The configuration is the source of the note repeat settings
    if ctx.surface.is_shift_pressed() {
        return if Resolution::get_match(ctx.config.note_repeat_length().value()) == scene_index { NOTE_REPEAT_LENGTH_HI } else { NOTE_REPEAT_LENGTH_OFF };
    }
    if Resolution::get_match(ctx.config.note_repeat_period().value()) == scene_index {
        NOTE_REPEAT_PERIOD_HI
    } else {
        NOTE_REPEAT_PERIOD_OFF
    }
}

/// Former `NoteRepeatSceneHelper.handleNoteRepeatSelection`.
pub fn handle_note_repeat_selection(ctx: &mut Ctx, scene_index: usize) {
    ctx.surface.set_trigger_consumed(ButtonId::Repeat);
    // The configuration is the source of the note repeat settings (it updates the note input)
    let resolution = Resolution::ALL[scene_index.min(7)];
    if ctx.surface.is_shift_pressed() {
        ctx.config.set_note_repeat_length(resolution);
    } else {
        ctx.config.set_note_repeat_period(resolution);
    }
}

// ----------------------------------------------------------------------------------------------
// Shared scene button handling of the play views

/// The scene button colors of the play, chords and drum 64 views: the note repeat selection
/// while Repeat is pressed, otherwise the scenes.
pub fn scene_button_color_id(ctx: &Ctx, button: ButtonId) -> &'static str {
    let Some(index) = scene_index(button) else {
        return BUTTON_COLOR_OFF;
    };
    if ctx.surface.is_pressed(ButtonId::Repeat) {
        return note_repeat_button_color_id(ctx, button);
    }
    let scene = ctx.model.scene_bank().item(index);
    if scene.does_exist() {
        if scene.is_selected() {
            COLOR_SELECTED_SCENE
        } else {
            COLOR_SCENE
        }
    } else {
        COLOR_SCENE_OFF
    }
}

/// Former `onSceneButton` with the override of `handleSceneButtonCombinations`: Repeat + scene
/// button selects the note repeat period (length with Shift).
pub fn on_scene_button_with_repeat(ctx: &mut Ctx, button: ButtonId, event: ButtonEvent) {
    let Some(index) = scene_index(button) else {
        return;
    };
    if event == ButtonEvent::Long || !ctx.model.scene_bank().item(index).does_exist() {
        return;
    }
    if event == ButtonEvent::Down && ctx.surface.is_pressed(ButtonId::Repeat) {
        handle_note_repeat_selection(ctx, 7 - index);
        return;
    }
    on_scene_button(ctx, button, event);
}

/// Repeat + scene button of the drum views (former override of `onButton`). Returns true if
/// handled.
pub fn handle_drum_scene_repeat(ctx: &mut Ctx, button: ButtonId, event: ButtonEvent) -> bool {
    let Some(index) = scene_index(button) else {
        return false;
    };
    if event != ButtonEvent::Down || !ctx.surface.is_pressed(ButtonId::Repeat) {
        return false;
    }
    handle_note_repeat_selection(ctx, 7 - index);
    true
}

/// True if the mode is one of the layer modes (former `Modes.isLayerMode`).
pub fn is_layer_mode(mode: Option<Modes>) -> bool {
    mode.is_some_and(|m| (Modes::DeviceLayer..=Modes::DeviceLayerDetails).contains(&m))
}

/// The select button of the drum views (former `DrumView.handleSelectButton` and
/// `Drum64View.handleSelectButton`): align the drum pad bank of the cursor device with the one
/// of the primary drum device and select the pad. `device_size` is the page size of the primary
/// drum device (64 for the drum 64 view). Returns true if the pad was selected.
///
/// Gaps of the Rust model: `IDrumPad.enter` (re-enter the nested device of an already selected
/// pad) does not exist; `ICursorDevice.selectParent` does nothing on Reaper.
pub fn select_drum_pad(ctx: &mut Ctx, played_pad: i32, device_size: Option<usize>, check_browser: bool) -> bool {
    let played = played_pad.max(0) as usize;
    let primary = match device_size {
        Some(size) => ctx.model.drum_device_sized(size).or_else(|| ctx.model.drum_device()),
        None => ctx.model.drum_device(),
    };
    // Do we have drum pads?
    let Some(primary) = primary else {
        return false;
    };
    if !primary.has_drum_pads() {
        return false;
    }
    let primary_position = primary.position();
    let scroll_pos = primary.drum_pad_bank().scroll_position();
    let drum_pad_selected = primary.drum_pad_bank().item(played).is_selected();

    let is_nested = ctx.model.cursor_device().is_nested();
    if is_nested {
        // We have to move up to compare the main drum devices
        ctx.model.cursor_device_mut().select_parent();
    }

    // Can only scroll to the channel if the cursor device is the primary device
    if primary_position != ctx.model.cursor_device().position() {
        return false;
    }

    // Align the primary and cursor device drum bank view
    let cd_bank = ctx.model.cursor_device_mut().drum_pad_bank_mut();
    let page_size = cd_bank.page_size().max(1);
    let adjusted_page = played / page_size * page_size;
    cd_bank.scroll_to((scroll_pos.max(0) as usize) + adjusted_page, false);

    // Do not reselect
    if drum_pad_selected {
        // If the instrument of the pad was selected for editing, try to select it again (former
        // IDrumPad.enter, not available in the model)
        return false;
    }

    // Only activate layer mode if not one of the layer modes is already active and browser is
    // off
    let browser_active = check_browser && ctx.model.browser().is_some_and(|b| b.is_active());
    if !is_layer_mode(ctx.modes.active_id()) && !browser_active {
        ctx.set_active_mode(Modes::DeviceLayer);
    }
    let device = match device_size {
        Some(size) if ctx.model.drum_device_sized(size).is_some() => ctx.model.drum_device_sized_mut(size),
        _ => ctx.model.drum_device_mut(),
    };
    if let Some(device) = device {
        device.drum_pad_bank_mut().select_pad(played);
    }
    true
}
