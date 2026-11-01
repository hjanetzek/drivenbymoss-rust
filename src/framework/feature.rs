// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Feature groups: views (which control the pad grid) and modes (which control knobs and
//! button rows), and their managers (former `IView`, `IMode`, `FeatureGroupManager`).

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use super::command::AftertouchCommand;
use super::ctx::Ctx;
use super::ids::{ButtonId, Modes, Views};
use crate::daw::events::ModelEvent;
use crate::midi::ButtonEvent;

/// The color ID of a button which is off.
pub const BUTTON_COLOR_OFF: &str = "BUTTON_COLOR_OFF";
/// The color ID of a button which is on.
pub const BUTTON_COLOR_ON: &str = "BUTTON_COLOR_ON";
/// The color ID of a button which is on (second variant).
pub const BUTTON_COLOR2_ON: &str = "BUTTON_COLOR2_ON";
/// The color ID of a highlighted button.
pub const BUTTON_COLOR_HI: &str = "BUTTON_COLOR_HI";
/// The color ID of a highlighted button (second variant).
pub const BUTTON_COLOR2_HI: &str = "BUTTON_COLOR2_HI";

/// A view controls the pad grid and some buttons of a surface.
pub trait View: Any {
    fn name(&self) -> &str;

    fn on_activate(&mut self, _ctx: &mut Ctx) {}

    fn on_deactivate(&mut self, _ctx: &mut Ctx) {}

    /// Get the color index of a button, which is controlled by the view.
    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        ctx.colors.color_index(self.button_color_id(ctx, button))
    }

    /// Get the color ID of a button, which is controlled by the view.
    fn button_color_id(&self, _ctx: &Ctx, _button: ButtonId) -> &'static str {
        BUTTON_COLOR_OFF
    }

    /// Draw the pad grid.
    fn draw_grid(&mut self, ctx: &mut Ctx);

    /// A pad of the grid was pressed (velocity > 0) or released (velocity == 0).
    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32);

    fn on_grid_note_long_press(&mut self, _ctx: &mut Ctx, _note: i32) {}

    /// A button was pressed, which is handled by the view (e.g. scene buttons).
    fn on_button(&mut self, _ctx: &mut Ctx, _button: ButtonId, _event: ButtonEvent, _velocity: i32) {}

    /// Update all controls which are not part of the grid (e.g. the display of the active mode).
    fn update_control_surface(&mut self, ctx: &mut Ctx) {
        ctx.update_active_mode_display();
    }

    /// Update the key translation table, e.g. after the scale has changed. Also called when the
    /// view is activated (after [`View::on_activate`]). The default blocks all notes from the
    /// pads (former `AbstractView.updateNoteMapping` with `EMPTY_TABLE`), so that a view which
    /// does not play notes does not keep the table of the previous view.
    fn update_note_mapping(&mut self, ctx: &mut Ctx) {
        let table = ctx.model.scales().translate_matrix_to_grid(&crate::scale::Scales::empty_matrix());
        ctx.surface.set_key_translation_table(table.to_vec());
    }

    fn select_track(&mut self, ctx: &mut Ctx, index: usize) {
        ctx.model.current_track_bank_mut().item_mut(index).select();
    }

    /// Called for every event of the model, for all registered views (active or not); replaces
    /// the observers which the Java views register in their constructors. See
    /// [`Ctx::dispatch_model_events_to_views`].
    fn on_model_event(&mut self, _ctx: &mut Ctx, _event: &ModelEvent) {}

    /// Called for every changed setting, for all registered views (active or not); replaces the
    /// setting observers of the Java views. See [`Ctx::dispatch_setting_changes_to_views`].
    fn on_setting_changed(&mut self, _ctx: &mut Ctx, _id: crate::config::SettingId) {}

    /// The aftertouch handler of the view, if any.
    fn aftertouch_command(&mut self) -> Option<&mut Box<dyn AftertouchCommand>> {
        None
    }

    fn execute_aftertouch_command(&mut self, ctx: &mut Ctx, note: i32, value: i32) {
        if let Some(command) = self.aftertouch_command() {
            if note == -1 {
                command.on_channel_aftertouch(ctx, value);
            } else {
                command.on_poly_aftertouch(ctx, note, value);
            }
        }
    }

    /// The scroll states of the view, `None` if the view is not scrollable (former
    /// `IScrollableView.updateScrollStates`).
    fn scroll_states(&self, _ctx: &Ctx) -> Option<ScrollStates> {
        None
    }

    // Optional capabilities used by the commands of the controller setups (they replace the
    // Java `instanceof` checks on the active view). The defaults do nothing. For the
    // `TransposeView` functions see `as_transpose_view` below.

    /// Former `AbstractSequencerView.onLeft` (previous page of the clip).
    fn on_sequencer_left(&mut self, _ctx: &mut Ctx, _event: ButtonEvent) {}

    /// Former `AbstractSequencerView.onRight` (next page of the clip).
    fn on_sequencer_right(&mut self, _ctx: &mut Ctx, _event: ButtonEvent) {}

    /// Former `instanceof AbstractSequencerView` + `getResolutionIndex`: the index of the
    /// sequencer resolution (0-7), `None` if the view is not a sequencer.
    fn sequencer_resolution_index(&self, _ctx: &Ctx) -> Option<usize> {
        None
    }

    /// Former `instanceof AbstractSequencerView` + `setResolutionIndex`: set the index of the
    /// sequencer resolution (0-7). Returns false if the view is not a sequencer.
    fn set_sequencer_resolution_index(&mut self, _ctx: &mut Ctx, _index: i32) -> bool {
        false
    }

    /// Former `AbstractSessionView.isBirdsEyeActive`.
    fn is_birds_eye_active(&self) -> bool {
        false
    }

    /// Former `AbstractSessionView.setBirdsEyeActive`.
    fn set_birds_eye_active(&mut self, _is_active: bool) {}

    /// Former `AbstractShiftView.wasUsed`: true if a pad of the shift view was used.
    fn was_shift_view_used(&self) -> bool {
        false
    }

    /// The session alternative interface of the view, if it implements it (former
    /// `instanceof ISessionAlternative`).
    fn session_alternative(&self) -> Option<&dyn crate::daw::clip::SessionAlternative> {
        None
    }

    fn session_alternative_mut(&mut self) -> Option<&mut dyn crate::daw::clip::SessionAlternative> {
        None
    }

    /// Returns the view as a provider of virtual fader values, if it is one (e.g. the Launchpad
    /// fader views, former `instanceof AbstractFaderView`).
    fn as_virtual_fader_view(&self) -> Option<&dyn super::virtual_fader::VirtualFaderView> {
        None
    }

    /// The note editor of the view, if it provides one (former `instanceof INoteEditorMode` /
    /// `INoteEditor`, e.g. the Launchpad note edit view).
    fn note_editor(&self) -> Option<&super::mode::note_editor::NoteEditor> {
        None
    }

    fn note_editor_mut(&mut self) -> Option<&mut super::mode::note_editor::NoteEditor> {
        None
    }

    /// Returns the view as a transposable view, if it is one (former `instanceof TransposeView`).
    fn as_transpose_view(&self) -> Option<&dyn super::view::transpose_view::TransposeView> {
        None
    }

    fn as_transpose_view_mut(&mut self) -> Option<&mut dyn super::view::transpose_view::TransposeView> {
        None
    }

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Which directions a view can scroll to (former `ScrollStates`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollStates {
    pub can_scroll_left: bool,
    pub can_scroll_right: bool,
    pub can_scroll_up: bool,
    pub can_scroll_down: bool,
}

impl ScrollStates {
    pub fn set_all(&mut self, enable: bool) {
        self.can_scroll_left = enable;
        self.can_scroll_right = enable;
        self.can_scroll_up = enable;
        self.can_scroll_down = enable;
    }
}

/// A mode controls knobs/faders and button rows of a surface.
pub trait Mode: Any {
    fn name(&self) -> &str;

    fn on_activate(&mut self, _ctx: &mut Ctx) {}

    fn on_deactivate(&mut self, _ctx: &mut Ctx) {}

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        ctx.colors.color_index(self.button_color_id(ctx, button))
    }

    fn button_color_id(&self, _ctx: &Ctx, _button: ButtonId) -> &'static str {
        BUTTON_COLOR_OFF
    }

    fn update_display(&mut self, _ctx: &mut Ctx) {}

    fn on_knob_value(&mut self, _ctx: &mut Ctx, _index: usize, _value: i32) {}

    fn knob_value(&self, _ctx: &Ctx, _index: usize) -> i32 {
        0
    }

    fn knob_color(&self, _ctx: &Ctx, _index: usize) -> i32 {
        0
    }

    fn on_knob_touch(&mut self, _ctx: &mut Ctx, _index: usize, _is_touched: bool) {}

    fn is_any_knob_touched(&self) -> bool {
        false
    }

    fn on_button(&mut self, _ctx: &mut Ctx, _row: usize, _index: usize, _event: ButtonEvent) {}

    fn select_item(&mut self, _ctx: &mut Ctx, _index: usize) {}

    fn selected_item_name(&self, _ctx: &Ctx) -> Option<String> {
        None
    }

    fn select_previous_item(&mut self, _ctx: &mut Ctx) {}

    fn select_next_item(&mut self, _ctx: &mut Ctx) {}

    fn select_previous_item_page(&mut self, _ctx: &mut Ctx) {}

    fn select_next_item_page(&mut self, _ctx: &mut Ctx) {}

    fn select_item_page(&mut self, _ctx: &mut Ctx, _page: usize) {}

    fn has_previous_item(&self, _ctx: &Ctx) -> bool {
        false
    }

    fn has_next_item(&self, _ctx: &Ctx) -> bool {
        false
    }

    fn has_previous_item_page(&self, _ctx: &Ctx) -> bool {
        false
    }

    fn has_next_item_page(&self, _ctx: &Ctx) -> bool {
        false
    }

    /// The note editor of the mode, if it provides one (former `instanceof INoteEditorMode`).
    fn note_editor(&self) -> Option<&super::mode::note_editor::NoteEditor> {
        None
    }

    fn note_editor_mut(&mut self) -> Option<&mut super::mode::note_editor::NoteEditor> {
        None
    }

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Implements the `as_any` functions of the [`View`] and [`Mode`] traits.
#[macro_export]
macro_rules! impl_as_any {
    () => {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

/// Implements [`View::as_transpose_view`] and [`View::as_transpose_view_mut`] for a view which
/// implements [`super::view::transpose_view::TransposeView`] itself.
#[macro_export]
macro_rules! impl_transpose_view {
    () => {
        fn as_transpose_view(&self) -> Option<&dyn $crate::framework::view::transpose_view::TransposeView> {
            Some(self)
        }

        fn as_transpose_view_mut(&mut self) -> Option<&mut dyn $crate::framework::view::transpose_view::TransposeView> {
            Some(self)
        }
    };
}

/// Implements [`View::session_alternative`] and [`View::session_alternative_mut`] for a view
/// which implements [`crate::daw::clip::SessionAlternative`] itself.
#[macro_export]
macro_rules! impl_session_alternative {
    () => {
        fn session_alternative(&self) -> Option<&dyn $crate::daw::clip::SessionAlternative> {
            Some(self)
        }

        fn session_alternative_mut(&mut self) -> Option<&mut dyn $crate::daw::clip::SessionAlternative> {
            Some(self)
        }
    };
}

/// Implements [`View::as_virtual_fader_view`] for a view which implements
/// [`super::virtual_fader::VirtualFaderView`] itself.
#[macro_export]
macro_rules! impl_virtual_fader_view {
    () => {
        fn as_virtual_fader_view(&self) -> Option<&dyn $crate::framework::virtual_fader::VirtualFaderView> {
            Some(self)
        }
    };
}

/// Called when the active feature group changes (previous ID, new ID).
pub type ChangeListener<E> = Rc<dyn Fn(&mut Ctx, Option<E>, E)>;

/// Manages a set of feature groups of which one is active. Functions which need to call the
/// activation handlers of the feature groups are implemented on [`Ctx`] (e.g.
/// [`Ctx::set_active_view`]), since they require access to the whole context.
pub struct FeatureGroupManager<E: Copy + Eq + Hash, F: ?Sized> {
    pub(crate) groups: HashMap<E, Rc<RefCell<F>>>,
    pub(crate) active_id: Option<E>,
    pub(crate) previous_id: Option<E>,
    pub(crate) temporary_id: Option<E>,
    pub(crate) default_id: Option<E>,
    pub(crate) listeners: Vec<ChangeListener<E>>,
    preferred: HashMap<i32, E>,
}

impl<E: Copy + Eq + Hash, F: ?Sized> Default for FeatureGroupManager<E, F> {
    fn default() -> Self {
        FeatureGroupManager {
            groups: HashMap::new(),
            active_id: None,
            previous_id: None,
            temporary_id: None,
            default_id: None,
            listeners: Vec::new(),
            preferred: HashMap::new(),
        }
    }
}

impl<E: Copy + Eq + Hash, F: ?Sized> FeatureGroupManager<E, F> {
    pub fn register(&mut self, id: E, group: Rc<RefCell<F>>) {
        self.groups.insert(id, group);
    }

    pub fn get(&self, id: E) -> Option<Rc<RefCell<F>>> {
        self.groups.get(&id).cloned()
    }

    pub fn contains(&self, id: E) -> bool {
        self.groups.contains_key(&id)
    }

    pub fn is_temporary(&self) -> bool {
        self.temporary_id.is_some()
    }

    /// Get the ID of the active feature group (the temporary one, if set).
    pub fn active_id(&self) -> Option<E> {
        self.temporary_id.or(self.active_id)
    }

    pub fn active_id_ignore_temporary(&self) -> Option<E> {
        self.active_id
    }

    pub fn active(&self) -> Option<Rc<RefCell<F>>> {
        self.active_id().and_then(|id| self.get(id))
    }

    pub fn is_active(&self, id: E) -> bool {
        self.active_id() == Some(id)
    }

    pub fn is_any_active(&self, ids: &[E]) -> bool {
        self.active_id().is_some_and(|active| ids.contains(&active))
    }

    pub fn is_active_ignore_temporary(&self, ids: &[E]) -> bool {
        self.active_id.is_some_and(|active| ids.contains(&active))
    }

    pub fn set_default_id(&mut self, id: E) {
        self.default_id = Some(id);
    }

    pub fn previous_id(&self) -> Option<E> {
        self.previous_id
    }

    pub fn set_previous_id(&mut self, id: Option<E>) {
        self.previous_id = id;
    }

    pub fn previous(&self) -> Option<Rc<RefCell<F>>> {
        self.previous_id.and_then(|id| self.get(id))
    }

    pub fn add_change_listener(&mut self, listener: ChangeListener<E>) {
        self.listeners.push(listener);
    }

    /// Store the preferred view for a track position.
    pub fn set_preferred(&mut self, position: i32, id: E) {
        if position >= 0 {
            self.preferred.insert(position, id);
        }
    }

    pub fn preferred(&self, position: i32) -> Option<E> {
        if position >= 0 {
            self.preferred.get(&position).copied()
        } else {
            None
        }
    }
}

pub type ViewManager = FeatureGroupManager<Views, dyn View>;
pub type ModeManager = FeatureGroupManager<Modes, dyn Mode>;

/// Access to `Any` for the feature group trait objects.
pub trait AsAnyRef {
    fn as_any_ref(&self) -> &dyn Any;
    fn as_any_mut_ref(&mut self) -> &mut dyn Any;
}

impl AsAnyRef for dyn View {
    fn as_any_ref(&self) -> &dyn Any {
        self.as_any()
    }

    fn as_any_mut_ref(&mut self) -> &mut dyn Any {
        self.as_any_mut()
    }
}

impl AsAnyRef for dyn Mode {
    fn as_any_ref(&self) -> &dyn Any {
        self.as_any()
    }

    fn as_any_mut_ref(&mut self) -> &mut dyn Any {
        self.as_any_mut()
    }
}
