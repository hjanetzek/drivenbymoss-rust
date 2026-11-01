// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Macros which forward functions of the [`View`](super::feature::View),
//! [`Mode`](super::feature::Mode) and [`TransposeView`](super::view::transpose_view::TransposeView)
//! traits to a field (the ported Java base class, see the shape section of
//! `doc/RUST_FORWARDING_PLAN.md`):
//!
//! ```ignore
//! impl View for SequencerView {
//!     // AbstractSequencerView
//!     crate::forward_view!(base.seq => name, button_color_id, update_control_surface);
//!     // AbstractNoteSequencerView
//!     crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);
//!     ...
//! }
//! ```
//!
//! The target is a field path relative to `self`; it needs a method with the same name and
//! signature, inherent or from a trait in scope. Only the listed functions are forwarded, an
//! unknown function name is a compile error.

/// Forwards the listed functions of the [`View`](crate::framework::feature::View) trait to a
/// field, see the [module documentation](self).
#[macro_export]
macro_rules! forward_view {
    (@fun name, [$($f:ident).+]) => {
        fn name(&self) -> &str {
            self.$($f).+.name()
        }
    };
    (@fun on_activate, [$($f:ident).+]) => {
        fn on_activate(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.on_activate(ctx);
        }
    };
    (@fun on_deactivate, [$($f:ident).+]) => {
        fn on_deactivate(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.on_deactivate(ctx);
        }
    };
    (@fun button_color, [$($f:ident).+]) => {
        fn button_color(&self, ctx: &$crate::framework::ctx::Ctx, button: $crate::framework::ids::ButtonId) -> i32 {
            self.$($f).+.button_color(ctx, button)
        }
    };
    (@fun button_color_id, [$($f:ident).+]) => {
        fn button_color_id(&self, ctx: &$crate::framework::ctx::Ctx, button: $crate::framework::ids::ButtonId) -> &'static str {
            self.$($f).+.button_color_id(ctx, button)
        }
    };
    (@fun draw_grid, [$($f:ident).+]) => {
        fn draw_grid(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.draw_grid(ctx);
        }
    };
    (@fun on_grid_note, [$($f:ident).+]) => {
        fn on_grid_note(&mut self, ctx: &mut $crate::framework::ctx::Ctx, note: i32, velocity: i32) {
            self.$($f).+.on_grid_note(ctx, note, velocity);
        }
    };
    (@fun on_grid_note_long_press, [$($f:ident).+]) => {
        fn on_grid_note_long_press(&mut self, ctx: &mut $crate::framework::ctx::Ctx, note: i32) {
            self.$($f).+.on_grid_note_long_press(ctx, note);
        }
    };
    (@fun on_button, [$($f:ident).+]) => {
        fn on_button(&mut self, ctx: &mut $crate::framework::ctx::Ctx, button: $crate::framework::ids::ButtonId, event: $crate::midi::ButtonEvent, velocity: i32) {
            self.$($f).+.on_button(ctx, button, event, velocity);
        }
    };
    (@fun update_control_surface, [$($f:ident).+]) => {
        fn update_control_surface(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.update_control_surface(ctx);
        }
    };
    (@fun update_note_mapping, [$($f:ident).+]) => {
        fn update_note_mapping(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.update_note_mapping(ctx);
        }
    };
    (@fun select_track, [$($f:ident).+]) => {
        fn select_track(&mut self, ctx: &mut $crate::framework::ctx::Ctx, index: usize) {
            self.$($f).+.select_track(ctx, index);
        }
    };
    (@fun on_model_event, [$($f:ident).+]) => {
        fn on_model_event(&mut self, ctx: &mut $crate::framework::ctx::Ctx, event: &$crate::daw::events::ModelEvent) {
            self.$($f).+.on_model_event(ctx, event);
        }
    };
    (@fun on_setting_changed, [$($f:ident).+]) => {
        fn on_setting_changed(&mut self, ctx: &mut $crate::framework::ctx::Ctx, id: $crate::config::SettingId) {
            self.$($f).+.on_setting_changed(ctx, id);
        }
    };
    (@fun aftertouch_command, [$($f:ident).+]) => {
        fn aftertouch_command(&mut self) -> Option<&mut Box<dyn $crate::framework::command::AftertouchCommand>> {
            self.$($f).+.aftertouch_command()
        }
    };
    (@fun execute_aftertouch_command, [$($f:ident).+]) => {
        fn execute_aftertouch_command(&mut self, ctx: &mut $crate::framework::ctx::Ctx, note: i32, value: i32) {
            self.$($f).+.execute_aftertouch_command(ctx, note, value);
        }
    };
    (@fun scroll_states, [$($f:ident).+]) => {
        fn scroll_states(&self, ctx: &$crate::framework::ctx::Ctx) -> Option<$crate::framework::feature::ScrollStates> {
            self.$($f).+.scroll_states(ctx)
        }
    };
    (@fun on_sequencer_left, [$($f:ident).+]) => {
        fn on_sequencer_left(&mut self, ctx: &mut $crate::framework::ctx::Ctx, event: $crate::midi::ButtonEvent) {
            self.$($f).+.on_sequencer_left(ctx, event);
        }
    };
    (@fun on_sequencer_right, [$($f:ident).+]) => {
        fn on_sequencer_right(&mut self, ctx: &mut $crate::framework::ctx::Ctx, event: $crate::midi::ButtonEvent) {
            self.$($f).+.on_sequencer_right(ctx, event);
        }
    };
    (@fun sequencer_resolution_index, [$($f:ident).+]) => {
        fn sequencer_resolution_index(&self, ctx: &$crate::framework::ctx::Ctx) -> Option<usize> {
            self.$($f).+.sequencer_resolution_index(ctx)
        }
    };
    (@fun set_sequencer_resolution_index, [$($f:ident).+]) => {
        fn set_sequencer_resolution_index(&mut self, ctx: &mut $crate::framework::ctx::Ctx, index: i32) -> bool {
            self.$($f).+.set_sequencer_resolution_index(ctx, index)
        }
    };
    (@fun is_birds_eye_active, [$($f:ident).+]) => {
        fn is_birds_eye_active(&self) -> bool {
            self.$($f).+.is_birds_eye_active()
        }
    };
    (@fun set_birds_eye_active, [$($f:ident).+]) => {
        fn set_birds_eye_active(&mut self, is_active: bool) {
            self.$($f).+.set_birds_eye_active(is_active);
        }
    };
    (@fun was_shift_view_used, [$($f:ident).+]) => {
        fn was_shift_view_used(&self) -> bool {
            self.$($f).+.was_shift_view_used()
        }
    };
    (@fun note_editor, [$($f:ident).+]) => {
        fn note_editor(&self) -> Option<&$crate::framework::mode::note_editor::NoteEditor> {
            self.$($f).+.note_editor()
        }
    };
    (@fun note_editor_mut, [$($f:ident).+]) => {
        fn note_editor_mut(&mut self) -> Option<&mut $crate::framework::mode::note_editor::NoteEditor> {
            self.$($f).+.note_editor_mut()
        }
    };
    (@list $path:tt $($fun:ident)+) => {
        $( $crate::forward_view!(@fun $fun, $path); )+
    };
    ($($f:ident).+ => $($fun:ident),+ $(,)?) => {
        $crate::forward_view!(@list [$($f).+] $($fun)+);
    };
}

/// Forwards the listed functions of the [`Mode`](crate::framework::feature::Mode) trait to a
/// field, see the [module documentation](self).
#[macro_export]
macro_rules! forward_mode {
    (@fun name, [$($f:ident).+]) => {
        fn name(&self) -> &str {
            self.$($f).+.name()
        }
    };
    (@fun on_activate, [$($f:ident).+]) => {
        fn on_activate(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.on_activate(ctx);
        }
    };
    (@fun on_deactivate, [$($f:ident).+]) => {
        fn on_deactivate(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.on_deactivate(ctx);
        }
    };
    (@fun button_color, [$($f:ident).+]) => {
        fn button_color(&self, ctx: &$crate::framework::ctx::Ctx, button: $crate::framework::ids::ButtonId) -> i32 {
            self.$($f).+.button_color(ctx, button)
        }
    };
    (@fun button_color_id, [$($f:ident).+]) => {
        fn button_color_id(&self, ctx: &$crate::framework::ctx::Ctx, button: $crate::framework::ids::ButtonId) -> &'static str {
            self.$($f).+.button_color_id(ctx, button)
        }
    };
    (@fun update_display, [$($f:ident).+]) => {
        fn update_display(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.update_display(ctx);
        }
    };
    (@fun on_knob_value, [$($f:ident).+]) => {
        fn on_knob_value(&mut self, ctx: &mut $crate::framework::ctx::Ctx, index: usize, value: i32) {
            self.$($f).+.on_knob_value(ctx, index, value);
        }
    };
    (@fun knob_value, [$($f:ident).+]) => {
        fn knob_value(&self, ctx: &$crate::framework::ctx::Ctx, index: usize) -> i32 {
            self.$($f).+.knob_value(ctx, index)
        }
    };
    (@fun knob_color, [$($f:ident).+]) => {
        fn knob_color(&self, ctx: &$crate::framework::ctx::Ctx, index: usize) -> i32 {
            self.$($f).+.knob_color(ctx, index)
        }
    };
    (@fun on_knob_touch, [$($f:ident).+]) => {
        fn on_knob_touch(&mut self, ctx: &mut $crate::framework::ctx::Ctx, index: usize, is_touched: bool) {
            self.$($f).+.on_knob_touch(ctx, index, is_touched);
        }
    };
    (@fun is_any_knob_touched, [$($f:ident).+]) => {
        fn is_any_knob_touched(&self) -> bool {
            self.$($f).+.is_any_knob_touched()
        }
    };
    (@fun on_button, [$($f:ident).+]) => {
        fn on_button(&mut self, ctx: &mut $crate::framework::ctx::Ctx, row: usize, index: usize, event: $crate::midi::ButtonEvent) {
            self.$($f).+.on_button(ctx, row, index, event);
        }
    };
    (@fun select_item, [$($f:ident).+]) => {
        fn select_item(&mut self, ctx: &mut $crate::framework::ctx::Ctx, index: usize) {
            self.$($f).+.select_item(ctx, index);
        }
    };
    (@fun selected_item_name, [$($f:ident).+]) => {
        fn selected_item_name(&self, ctx: &$crate::framework::ctx::Ctx) -> Option<String> {
            self.$($f).+.selected_item_name(ctx)
        }
    };
    (@fun select_previous_item, [$($f:ident).+]) => {
        fn select_previous_item(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.select_previous_item(ctx);
        }
    };
    (@fun select_next_item, [$($f:ident).+]) => {
        fn select_next_item(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.select_next_item(ctx);
        }
    };
    (@fun select_previous_item_page, [$($f:ident).+]) => {
        fn select_previous_item_page(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.select_previous_item_page(ctx);
        }
    };
    (@fun select_next_item_page, [$($f:ident).+]) => {
        fn select_next_item_page(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.select_next_item_page(ctx);
        }
    };
    (@fun select_item_page, [$($f:ident).+]) => {
        fn select_item_page(&mut self, ctx: &mut $crate::framework::ctx::Ctx, page: usize) {
            self.$($f).+.select_item_page(ctx, page);
        }
    };
    (@fun has_previous_item, [$($f:ident).+]) => {
        fn has_previous_item(&self, ctx: &$crate::framework::ctx::Ctx) -> bool {
            self.$($f).+.has_previous_item(ctx)
        }
    };
    (@fun has_next_item, [$($f:ident).+]) => {
        fn has_next_item(&self, ctx: &$crate::framework::ctx::Ctx) -> bool {
            self.$($f).+.has_next_item(ctx)
        }
    };
    (@fun has_previous_item_page, [$($f:ident).+]) => {
        fn has_previous_item_page(&self, ctx: &$crate::framework::ctx::Ctx) -> bool {
            self.$($f).+.has_previous_item_page(ctx)
        }
    };
    (@fun has_next_item_page, [$($f:ident).+]) => {
        fn has_next_item_page(&self, ctx: &$crate::framework::ctx::Ctx) -> bool {
            self.$($f).+.has_next_item_page(ctx)
        }
    };
    (@fun note_editor, [$($f:ident).+]) => {
        fn note_editor(&self) -> Option<&$crate::framework::mode::note_editor::NoteEditor> {
            self.$($f).+.note_editor()
        }
    };
    (@fun note_editor_mut, [$($f:ident).+]) => {
        fn note_editor_mut(&mut self) -> Option<&mut $crate::framework::mode::note_editor::NoteEditor> {
            self.$($f).+.note_editor_mut()
        }
    };
    (@list $path:tt $($fun:ident)+) => {
        $( $crate::forward_mode!(@fun $fun, $path); )+
    };
    ($($f:ident).+ => $($fun:ident),+ $(,)?) => {
        $crate::forward_mode!(@list [$($f).+] $($fun)+);
    };
}

/// Forwards the listed functions of the
/// [`TransposeView`](crate::framework::view::transpose_view::TransposeView) trait to a field, see
/// the [module documentation](self).
#[macro_export]
macro_rules! forward_transpose_view {
    (@fun on_octave_down, [$($f:ident).+]) => {
        fn on_octave_down(&mut self, ctx: &mut $crate::framework::ctx::Ctx, event: $crate::midi::ButtonEvent) {
            self.$($f).+.on_octave_down(ctx, event);
        }
    };
    (@fun on_octave_up, [$($f:ident).+]) => {
        fn on_octave_up(&mut self, ctx: &mut $crate::framework::ctx::Ctx, event: $crate::midi::ButtonEvent) {
            self.$($f).+.on_octave_up(ctx, event);
        }
    };
    (@fun is_octave_up_button_on, [$($f:ident).+]) => {
        fn is_octave_up_button_on(&self, ctx: &$crate::framework::ctx::Ctx) -> bool {
            self.$($f).+.is_octave_up_button_on(ctx)
        }
    };
    (@fun is_octave_down_button_on, [$($f:ident).+]) => {
        fn is_octave_down_button_on(&self, ctx: &$crate::framework::ctx::Ctx) -> bool {
            self.$($f).+.is_octave_down_button_on(ctx)
        }
    };
    (@fun reset_octave, [$($f:ident).+]) => {
        fn reset_octave(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($f).+.reset_octave(ctx);
        }
    };
    (@list $path:tt $($fun:ident)+) => {
        $( $crate::forward_transpose_view!(@fun $fun, $path); )+
    };
    ($($f:ident).+ => $($fun:ident),+ $(,)?) => {
        $crate::forward_transpose_view!(@list [$($f).+] $($fun)+);
    };
}

#[cfg(test)]
mod tests {
    //! Dummy types which use every arm of the macros once, so that each arm is compiled.

    use crate::framework::ctx::Ctx;
    use crate::framework::feature::{Mode, View};
    use crate::framework::view::transpose_view::TransposeView;
    use crate::midi::ButtonEvent;

    struct InnerView;

    impl View for InnerView {
        fn name(&self) -> &str {
            "Inner"
        }

        fn draw_grid(&mut self, _ctx: &mut Ctx) {}

        fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {}

        crate::impl_as_any!();
    }

    impl TransposeView for InnerView {
        fn on_octave_down(&mut self, _ctx: &mut Ctx, _event: ButtonEvent) {}

        fn on_octave_up(&mut self, _ctx: &mut Ctx, _event: ButtonEvent) {}

        fn is_octave_up_button_on(&self, _ctx: &Ctx) -> bool {
            true
        }

        fn is_octave_down_button_on(&self, _ctx: &Ctx) -> bool {
            false
        }

        fn reset_octave(&mut self, _ctx: &mut Ctx) {}
    }

    struct Wrapped {
        inner: InnerView,
    }

    /// Two levels deep, to test field paths.
    struct OuterView {
        base: Wrapped,
    }

    impl View for OuterView {
        crate::forward_view!(base.inner => name, on_activate, on_deactivate, button_color,
            button_color_id, draw_grid, on_grid_note, on_grid_note_long_press, on_button,
            update_control_surface, update_note_mapping, select_track, on_model_event,
            on_setting_changed, aftertouch_command, execute_aftertouch_command, scroll_states,
            on_sequencer_left, on_sequencer_right, sequencer_resolution_index,
            set_sequencer_resolution_index, is_birds_eye_active, set_birds_eye_active,
            was_shift_view_used, note_editor, note_editor_mut);

        crate::impl_transpose_view!();
        crate::impl_as_any!();
    }

    impl TransposeView for OuterView {
        crate::forward_transpose_view!(base.inner => on_octave_down, on_octave_up,
            is_octave_up_button_on, is_octave_down_button_on, reset_octave);
    }

    struct InnerMode;

    impl Mode for InnerMode {
        fn name(&self) -> &str {
            "Inner"
        }

        fn knob_value(&self, _ctx: &Ctx, index: usize) -> i32 {
            index as i32 * 2
        }

        crate::impl_as_any!();
    }

    struct OuterMode {
        inner: InnerMode,
    }

    impl Mode for OuterMode {
        crate::forward_mode!(inner => name, on_activate, on_deactivate, button_color,
            button_color_id, update_display, on_knob_value, knob_value, knob_color,
            on_knob_touch, is_any_knob_touched, on_button, select_item, selected_item_name,
            select_previous_item, select_next_item, select_previous_item_page,
            select_next_item_page, select_item_page, has_previous_item, has_next_item,
            has_previous_item_page, has_next_item_page, note_editor, note_editor_mut);

        crate::impl_as_any!();
    }

    #[test]
    fn forwards_to_the_field() {
        let view = OuterView { base: Wrapped { inner: InnerView } };
        assert_eq!(view.name(), "Inner");
        assert!(view.as_transpose_view().is_some());
        let mode = OuterMode { inner: InnerMode };
        assert_eq!(mode.name(), "Inner");
        assert!(mode.note_editor().is_none());
    }
}
