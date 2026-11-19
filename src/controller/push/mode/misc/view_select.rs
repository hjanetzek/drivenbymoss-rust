// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The selection of the note views (former `NoteViewSelectMode`, registered as VIEW_SELECT) and
//! of the session views (former `SessionViewSelectMode`).

use super::{forward_base, restore_mode_later, row_index, set_active_mode_later};
use crate::config::push as config;
use crate::controller::push::display1::SELECT_ARROW;
use crate::controller::push::mode::PushDisplayMode;
use crate::daw::util::optimize_name;
use crate::framework::ctx::Ctx;
use crate::framework::display::TextDisplay;
use crate::framework::feature::{Mode, BUTTON_COLOR_HI, BUTTON_COLOR_OFF, BUTTON_COLOR_ON};
use crate::framework::graphics::GraphicDisplay;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::framework::mode::parameter_mode::ParameterModeBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

/// The views of the first row (bottom of the display).
pub const NOTE_VIEWS: [Option<Views>; 8] = [Some(Views::Play), Some(Views::Chords), Some(Views::Piano), Some(Views::Drum64), None, None, Some(Views::ClipLength), Some(Views::PrgChange)];
/// The views of the second row (top of the display).
pub const NOTE_VIEWS_TOP: [Option<Views>; 8] =
    [Some(Views::Sequencer), Some(Views::PolySequencer), Some(Views::Raindrops), None, Some(Views::Drum), Some(Views::Drum4), Some(Views::Drum8), Some(Views::DrumXox)];

/// The name of a registered view.
fn view_name(ctx: &Ctx, id: Views) -> Option<String> {
    let view = ctx.views.get(id)?;
    let name = view.try_borrow().ok()?.name().to_string();
    Some(name)
}

fn arrow(selected: bool) -> String {
    if selected {
        SELECT_ARROW.to_string()
    } else {
        String::new()
    }
}

/// Select a note view with the two button rows.
pub struct NoteViewSelectMode {
    base: ParameterModeBase,
}

impl NoteViewSelectMode {
    pub fn new() -> Self {
        NoteViewSelectMode { base: super::base("Note View Select") }
    }

    fn activate_view(ctx: &mut Ctx, view: Option<Views>) {
        if let Some(view) = view {
            ctx.activate_preferred_view(view);
        }
        restore_mode_later(ctx);
    }

    fn cell_text(ctx: &Ctx, view: Views) -> String {
        match view_name(ctx, view) {
            Some(name) => format!("{}{}", arrow(ctx.views.is_active(view)), optimize_name(&name, 8)),
            None => String::new(),
        }
    }
}

impl Default for NoteViewSelectMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for NoteViewSelectMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_block(1, 0, "Sequence:");
        display.set_block(2, 0, "Play:");
        display.set_block(2, 3, "Tools:");
        for i in 0..NOTE_VIEWS.len() {
            if let Some(view) = NOTE_VIEWS[i] {
                display.set_cell(3, i, &Self::cell_text(ctx, view));
            }
            if let Some(view) = NOTE_VIEWS_TOP[i] {
                display.set_cell(0, i, &Self::cell_text(ctx, view));
            }
        }
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        for i in 0..NOTE_VIEWS.len() {
            let menu_bottom_name = NOTE_VIEWS[i].and_then(|v| view_name(ctx, v)).unwrap_or_default();
            let menu_top_name = NOTE_VIEWS_TOP[i].and_then(|v| view_name(ctx, v)).unwrap_or_default();
            let is_menu_bottom_selected = NOTE_VIEWS[i].is_some_and(|v| ctx.views.is_active(v));
            let is_menu_top_selected = NOTE_VIEWS_TOP[i].is_some_and(|v| ctx.views.is_active(v));
            let (title_top, title_bottom) = match i {
                0 => ("Sequence", "Play"),
                6 => ("", "Tools"),
                _ => ("", ""),
            };
            display.add_option_element(title_top, &menu_top_name, is_menu_top_selected, title_bottom, &menu_bottom_name, is_menu_bottom_selected, false);
        }
    }
}

impl Mode for NoteViewSelectMode {
    forward_base!();

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if event != ButtonEvent::Up || index >= 8 {
            return;
        }
        let view = if row == 0 { NOTE_VIEWS[index] } else { NOTE_VIEWS_TOP[index] };
        Self::activate_view(ctx, view);
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let view = if let Some(index) = row_index(0, button) {
            NOTE_VIEWS[index]
        } else if let Some(index) = row_index(1, button) {
            NOTE_VIEWS_TOP[index]
        } else {
            return BUTTON_COLOR_OFF;
        };
        match view {
            None => BUTTON_COLOR_OFF,
            Some(view) => {
                if ctx.views.is_active(view) {
                    BUTTON_COLOR_HI
                } else {
                    BUTTON_COLOR_ON
                }
            }
        }
    }

    impl_as_any!();
}

const SESSION_VIEWS: [Option<Views>; 5] = [Some(Views::Session), Some(Views::Session), Some(Views::ScenePlay), None, None];
const SESSION_VIEW_NAMES: [&str; 5] = ["Clips", "Flipped", "Scenes", "", ""];

/// Configure the session: the function of the pads and what the display shows.
pub struct SessionViewSelectMode {
    base: ParameterModeBase,
}

impl SessionViewSelectMode {
    pub fn new() -> Self {
        SessionViewSelectMode { base: super::base("Session View") }
    }

    fn is_selected(ctx: &Ctx, index: usize) -> bool {
        let active_view = SESSION_VIEWS[index].is_some_and(|v| ctx.views.is_active(v));
        match index {
            0 => active_view && !ctx.config.is_flip_session(),
            1 => active_view && ctx.config.is_flip_session(),
            _ => active_view,
        }
    }
}

impl Default for SessionViewSelectMode {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDisplayMode for SessionViewSelectMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        display.set_cell(0, 3, " Session");
        display.set_block(0, 2, "configuration");
        display.set_block(2, 0, "Pads are:");
        for i in 0..SESSION_VIEWS.len() {
            if SESSION_VIEWS[i].is_some() {
                display.set_cell(3, i, &format!("{}{}", arrow(Self::is_selected(ctx, i)), SESSION_VIEW_NAMES[i]));
            }
        }
        let modes = &ctx.modes;
        display.set_cell(2, 5, " Display");
        display.set_cell(2, 6, "shows:");
        display.set_cell(3, 5, &format!(" {}Clips", arrow(modes.is_active_ignore_temporary(&[Modes::Session]))));
        display.set_cell(3, 6, &format!("{}Markers", arrow(modes.is_active_ignore_temporary(&[Modes::Markers]))));
        display.set_cell(3, 7, &format!("{}Off", arrow(!modes.is_active_ignore_temporary(&[Modes::Session, Modes::Markers]))));
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        for i in 0..SESSION_VIEWS.len() {
            let is_menu_bottom_selected = SESSION_VIEWS[i].is_some() && Self::is_selected(ctx, i);
            display.add_option_element(if i == 0 { "Session configuration" } else { "" }, "", false, if i == 0 { "Pads are" } else { "" }, SESSION_VIEW_NAMES[i], is_menu_bottom_selected, false);
        }
        let modes = &ctx.modes;
        display.add_option_element("", "", false, "Display shows", "Scenes/Clips", modes.is_active_ignore_temporary(&[Modes::Session]), false);
        display.add_option_element("", "", false, "", "Markers", modes.is_active_ignore_temporary(&[Modes::Markers]), false);
        display.add_option_element("", "", false, "", "Off", !modes.is_active_ignore_temporary(&[Modes::Session, Modes::Markers]), false);
    }
}

impl Mode for SessionViewSelectMode {
    forward_base!();

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        if row != 0 || event != ButtonEvent::Up {
            return;
        }
        match index {
            0 | 1 => {
                config::set_flip_session(&mut ctx.config, index == 1);
                if let Some(view) = SESSION_VIEWS[index] {
                    ctx.set_active_view(view);
                }
                restore_mode_later(ctx);
            }
            2 => {
                config::set_scene_view(&mut ctx.config);
                restore_mode_later(ctx);
            }
            5 => set_active_mode_later(ctx, Modes::Session),
            6 => set_active_mode_later(ctx, Modes::Markers),
            7 => {
                let mode = config::mixer_mode(&ctx.config);
                set_active_mode_later(ctx, mode);
            }
            _ => {}
        }
    }

    fn button_color_id(&self, ctx: &Ctx, button: ButtonId) -> &'static str {
        let Some(index) = row_index(0, button) else {
            return BUTTON_COLOR_OFF;
        };
        if index < SESSION_VIEWS.len() {
            if SESSION_VIEWS[index].is_none() {
                return BUTTON_COLOR_OFF;
            }
            return if Self::is_selected(ctx, index) { BUTTON_COLOR_HI } else { BUTTON_COLOR_ON };
        }
        let modes = &ctx.modes;
        let selected = match index {
            5 => modes.is_active_ignore_temporary(&[Modes::Session]),
            6 => modes.is_active_ignore_temporary(&[Modes::Markers]),
            7 => !modes.is_active_ignore_temporary(&[Modes::Session, Modes::Markers]),
            _ => return BUTTON_COLOR_OFF,
        };
        if selected {
            BUTTON_COLOR_HI
        } else {
            BUTTON_COLOR_ON
        }
    }

    impl_as_any!();
}
