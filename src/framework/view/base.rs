// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Functions shared by all views (former `AbstractView` and `AbstractFeatureGroup`).

use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::daw::track::Track;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Views};
use crate::midi::ButtonEvent;
use crate::scale::{KeyManager, NoteMatrix, Scales};

/// The state every view has: its name and the key manager of the pad grid.
pub struct ViewBase {
    pub name: String,
    pub key_manager: KeyManager,
}

impl ViewBase {
    pub fn new(name: &str) -> Self {
        ViewBase { name: name.to_string(), key_manager: KeyManager::new() }
    }

    /// Set the note matrix of the key manager and, for a full matrix, the key translation table
    /// of the surface (former `delayedUpdateNoteMapping`). Java delayed this by 6ms to collapse
    /// several updates; here it is applied immediately.
    pub fn update_note_mapping(&mut self, ctx: &mut Ctx, matrix: NoteMatrix) {
        self.key_manager.set_note_matrix(matrix);
        let table = ctx.model.scales().translate_matrix_to_grid(&matrix);
        ctx.surface.set_key_translation_table(table.to_vec());
    }

    /// Block all notes from the pads (former `EMPTY_TABLE`).
    pub fn clear_note_mapping(&mut self, ctx: &mut Ctx) {
        self.update_note_mapping(ctx, Scales::empty_matrix());
    }

    /// Get the color ID of a pad from the key manager, replacing the octave color with the
    /// color of the track (former `getPadColor`).
    pub fn pad_color(&self, ctx: &Ctx, pad: i32, track: Option<&Track>) -> &'static str {
        replace_octave_color_with_track_color(track, self.key_manager.color(ctx.model.scales(), pad))
    }
}

/// Replace the octave color with the DAW color of the track (if any).
pub fn replace_octave_color_with_track_color(track: Option<&Track>, color_id: &'static str) -> &'static str {
    if color_id != Scales::SCALE_COLOR_OCTAVE {
        return color_id;
    }
    match track {
        Some(track) if track.does_exist() => DawColor::color_id(track.color()),
        _ => Scales::SCALE_COLOR_OCTAVE,
    }
}

/// True if the button is one of the 8 scene buttons.
pub fn is_scene_button(button: ButtonId) -> bool {
    button >= ButtonId::Scene1 && button <= ButtonId::Scene8
}

/// The index (0-7) of a scene button.
pub fn scene_index(button: ButtonId) -> Option<usize> {
    is_scene_button(button).then(|| button.ordinal() - ButtonId::Scene1.ordinal())
}

/// Default handling of a scene button: launch (or delete/duplicate in combination with these
/// buttons) the scene of the current track bank (former `AbstractView.onSceneButton`).
pub fn on_scene_button(ctx: &mut Ctx, button: ButtonId, event: ButtonEvent) {
    let Some(index) = scene_index(button) else {
        return;
    };
    if event == ButtonEvent::Long {
        return;
    }
    if !ctx.model.scene_bank().item(index).does_exist() {
        return;
    }
    let is_pressed = event == ButtonEvent::Down;
    if is_pressed {
        if ctx.is_button_combination(ButtonId::Delete) {
            ctx.model.scene_bank_mut().item_mut(index).remove();
            return;
        }
        if ctx.is_button_combination(ButtonId::Duplicate) {
            ctx.model.scene_bank_mut().item_mut(index).duplicate();
            return;
        }
        ctx.model.scene_bank_mut().item_mut(index).select();
        let name = ctx.model.scene_bank().item(index).name();
        ctx.notify(&name);
        if ctx.surface.is_select_pressed() {
            return;
        }
    }
    let is_alternative = ctx.surface.is_shift_pressed();
    ctx.model.scene_bank_mut().item_mut(index).launch(is_pressed, is_alternative);
}

/// Activate a view and store it as the preferred view of the cursor track.
pub fn activate_preferred_view(ctx: &mut Ctx, view: Views) {
    ctx.activate_preferred_view(view);
}
