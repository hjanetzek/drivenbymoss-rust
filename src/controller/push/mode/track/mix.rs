// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The mix modes of the Push (former `TrackMode`, `VolumeMode`, `PanMode`, `CrossfadeMode` and
//! `SendMode`): one type with the kind of the mode, since they only differ in their parameter
//! provider and their display.

use crate::framework::ctx::Ctx;
use crate::framework::display::{Format, TextDisplay};
use crate::framework::feature::Mode;
use crate::framework::graphics::{GraphicDisplay, SendData, GRID_ELEMENT_CHANNEL_CROSSFADER, GRID_ELEMENT_CHANNEL_PAN, GRID_ELEMENT_CHANNEL_VOLUME};
use crate::framework::ids::ButtonId;
use crate::framework::parameterprovider::{provider, FnParameterProvider, PanParameterProvider, ParamRef, SendParameterProvider, SharedProvider, VolumeParameterProvider};
use crate::impl_as_any;
use crate::midi::ButtonEvent;

use super::super::super::parameterprovider::{is_send, toggle_send_enabled, PushTrackParameterProvider};
use super::super::{update_display, PushDisplayMode};
use super::{format_volume, set_cell_value, update_type, AbstractTrackMode};

/// The kind of a mix mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixKind {
    /// The selected track with volume, panorama and sends (former `TrackMode`).
    Track,
    /// The volumes of the tracks (former `VolumeMode`).
    Volume,
    /// The panoramas of the tracks (former `PanMode`).
    Pan,
    /// The crossfade modes of the tracks (former `CrossfadeMode`).
    Crossfade,
    /// The send with the index of the tracks (former `SendMode`).
    Send(usize),
}

/// A mix mode (former `TrackMode`, `VolumeMode`, `PanMode`, `CrossfadeMode`, `SendMode`).
pub struct MixMode {
    pub track: AbstractTrackMode,
    pub kind: MixKind,
}

/// The texts of the crossfade modes on the Push 1 (former `CROSSFADE_TEXT`).
fn crossfade_text(value: &str) -> &'static str {
    match value {
        "A" => "A",
        "B" => "       B",
        "AB" => "   <> ",
        _ => "",
    }
}

impl MixMode {
    pub fn new(ctx: &Ctx, kind: MixKind) -> Self {
        let (name, parameter_provider): (&str, SharedProvider) = match kind {
            MixKind::Track => ("Track", provider(PushTrackParameterProvider)),
            MixKind::Volume => ("Volume", provider(VolumeParameterProvider::new(ctx))),
            MixKind::Pan => ("Panning", provider(PanParameterProvider::new(ctx))),
            // Divergence: the Rust model has no crossfade mode parameters of the tracks (former
            // CrossfadeParameterProvider), the knobs do nothing
            MixKind::Crossfade => ("Crossfade", provider(FnParameterProvider::new(8, |_: &Ctx, _| ParamRef::Empty))),
            MixKind::Send(index) => ("Send", provider(SendParameterProvider::new(ctx, index as i32, 0))),
        };
        MixMode { track: AbstractTrackMode::new(ctx, name, Some(parameter_provider)), kind }
    }

    /// Track mode, Push 1.
    fn update_track_display1(&self, ctx: &Ctx, display: &mut TextDisplay) {
        let tb = ctx.model.current_track_bank();
        match tb.selected_item() {
            None => {
                display.set_row(1, "                     Please selecta track...                        ");
            }
            Some(t) => {
                display.set_cell(0, 0, "Volume").set_cell(1, 0, &t.volume_str_limited(8)).set_cell(2, 0, &format_volume(ctx, t.volume(), t.vu()));
                display.set_cell(0, 1, "Pan").set_cell(1, 1, &t.pan_str_limited(8));
                set_cell_value(ctx, display, 2, 1, t.pan(), Format::Pan);

                let send_start = 2;
                let send_count = 6;
                let send_bank = t.send_bank();
                for i in 0..send_count {
                    let pos = send_start + i;
                    let send = send_bank.item(i);
                    if send.does_exist() {
                        display.set_cell(0, pos, send.name()).set_cell(1, pos, &send.displayed_value_limited(8));
                        set_cell_value(ctx, display, 2, pos, send.value(), Format::Value);
                    }
                }
            }
        }
        self.track.draw_row4(ctx, display);
    }

    /// Track mode, Push 2/3.
    fn update_track_display2(&mut self, ctx: &Ctx, display: &mut GraphicDisplay) {
        let tb = ctx.model.current_track_bank();
        let selected_track = tb.selected_item();

        // Get the index at which to draw the Sends element
        let selected_index = selected_track.map_or(-1, |t| t.index());
        let mut sends_index = selected_track.map_or(-1, |t| t.index() + 1);
        if sends_index == 8 {
            sends_index = 6;
        }

        self.track.update_menu_items(ctx, 0);

        let vc = ctx.model.value_changer();
        let is_pinned = ctx.model.is_cursor_track_pinned();
        let enable_vu_meters = ctx.config.is_enable_vu_meters();
        for i in 0..8 {
            let t = tb.item(i);

            // The menu item
            let (top_menu, top_menu_selected) = self.track.menu[i].clone();

            // Channel info
            let bottom_menu = if t.does_exist() { t.name().to_string() } else { String::new() };
            let bottom_menu_color = t.color();
            let is_bottom_menu_on = t.is_selected();

            if t.is_selected() {
                let crossfade_mode = self.track.crossfade_mode_as_number(ctx, t);
                let vu_r = vc.to_display_value(if enable_vu_meters { t.vu_right() } else { 0 });
                let vu_l = vc.to_display_value(if enable_vu_meters { t.vu_left() } else { 0 });
                display.add_channel_element(
                    &top_menu,
                    top_menu_selected,
                    &bottom_menu,
                    Some(update_type(t)),
                    bottom_menu_color,
                    is_bottom_menu_on,
                    vc.to_display_value(t.volume()),
                    vc.to_display_value(t.modulated_volume()),
                    &if self.track.base.is_knob_touched(0) { t.volume_str_limited(8) } else { String::new() },
                    vc.to_display_value(t.pan()),
                    vc.to_display_value(t.modulated_pan()),
                    &if self.track.base.is_knob_touched(1) { t.pan_str_limited(8) } else { String::new() },
                    vu_l,
                    vu_r,
                    t.is_mute(),
                    t.is_solo(),
                    t.is_rec_arm(),
                    t.is_activated(),
                    crossfade_mode,
                    is_pinned,
                );
            } else if sends_index == i as i32 {
                let sel_track = (selected_index >= 0).then(|| tb.item(selected_index as usize));
                let mut send_data = Vec::with_capacity(4);
                for j in 0..4 {
                    if let Some(sel_track) = sel_track {
                        let send = sel_track.send_bank().item(j);
                        let exists = send.does_exist();
                        let text = if exists && self.track.base.is_knob_touched(4 + j) { send.displayed_value_limited(8) } else { String::new() };
                        send_data.push(SendData::new(
                            send.is_enabled(),
                            send.name(),
                            &text,
                            vc.to_display_value(if exists { send.value() } else { 0 }),
                            vc.to_display_value(if exists { send.modulated_value() } else { 0 }),
                            true,
                        ));
                        continue;
                    }
                    send_data.push(SendData::new(false, "", "", 0, 0, true));
                }
                let is_send_active = sel_track.is_none_or(|t| t.is_activated());
                display.add_sends_element(&top_menu, top_menu_selected, &bottom_menu, Some(update_type(t)), bottom_menu_color, is_bottom_menu_on, &send_data, true, is_send_active, t.is_activated());
            } else {
                display.add_channel_selector_element(&top_menu, top_menu_selected, &bottom_menu, Some(update_type(t)), bottom_menu_color, is_bottom_menu_on, t.is_activated());
            }
        }
    }

    /// Send mode, Push 2/3.
    fn update_send_display2(&mut self, ctx: &Ctx, display: &mut GraphicDisplay, send_index: usize) {
        self.track.update_track_menu(ctx, 5 + (send_index % 4) as i32);

        let tb = ctx.model.current_track_bank();
        let vc = ctx.model.value_changer();
        for i in 0..8 {
            let t = tb.item(i);
            let send_bank = t.send_bank();
            let mut send_data = Vec::with_capacity(4);
            for j in 0..4 {
                let send = send_bank.item(j);
                let exists = send.does_exist();
                let text = if exists && send_index == j && self.track.base.is_knob_touched(i) { send.displayed_value_limited(8) } else { String::new() };
                send_data.push(SendData::new(
                    send.is_enabled(),
                    if exists { send.name() } else { "" },
                    &text,
                    vc.to_display_value(if exists { send.value() } else { -1 }),
                    vc.to_display_value(if exists { send.modulated_value() } else { -1 }),
                    send_index == j,
                ));
            }
            let (top_menu, is_top_menu_on) = self.track.menu[i].clone();
            let name = if t.does_exist() { t.name().to_string() } else { String::new() };
            display.add_sends_element(&top_menu, is_top_menu_on, &name, Some(update_type(t)), t.color(), t.is_selected(), &send_data, false, t.is_activated(), t.is_activated());
        }
    }
}

impl PushDisplayMode for MixMode {
    fn update_display1(&mut self, ctx: &mut Ctx, display: &mut TextDisplay) {
        let ctx = &*ctx;
        let tb = ctx.model.current_track_bank();
        match self.kind {
            MixKind::Track => {
                self.update_track_display1(ctx, display);
                return;
            }
            MixKind::Volume => {
                for i in 0..8 {
                    let t = tb.item(i);
                    display.set_cell(0, i, if t.does_exist() { "Volume" } else { "" }).set_cell(1, i, &t.volume_str_limited(8));
                    if t.does_exist() {
                        display.set_cell(2, i, &format_volume(ctx, t.volume(), t.vu()));
                    }
                }
            }
            MixKind::Pan => {
                for i in 0..8 {
                    let t = tb.item(i);
                    display.set_cell(0, i, if t.does_exist() { "Pan" } else { "" }).set_cell(1, i, &t.pan_str_limited(8));
                    if t.does_exist() {
                        set_cell_value(ctx, display, 2, i, t.pan(), Format::Pan);
                    }
                }
            }
            MixKind::Crossfade => {
                for i in 0..8 {
                    let t = tb.item(i);
                    if !t.does_exist() {
                        continue;
                    }
                    display.set_cell(0, i, "Crossfdr");
                    // The crossfade mode parameter of a track is not in the model
                    if let Some(parameter) = t.crossfade_parameter() {
                        display.set_cell(1, i, crossfade_text(&parameter.displayed_value()));
                        set_cell_value(ctx, display, 2, i, parameter.value(), Format::Pan);
                    }
                }
            }
            MixKind::Send(send_index) => {
                for i in 0..8 {
                    let t = tb.item(i);
                    if t.does_exist() {
                        let send = t.send_bank().item(send_index);
                        display.set_cell(0, i, send.name());
                        display.set_cell(1, i, &send.displayed_value_limited(8));
                        set_cell_value(ctx, display, 2, i, send.value(), Format::Value);
                    }
                }
            }
        }
        self.track.draw_row4(ctx, display);
    }

    fn update_display2(&mut self, ctx: &mut Ctx, display: &mut GraphicDisplay) {
        let ctx = &*ctx;
        match self.kind {
            MixKind::Track => self.update_track_display2(ctx, display),
            MixKind::Volume => self.track.update_channel_display(ctx, display, GRID_ELEMENT_CHANNEL_VOLUME, true, false),
            MixKind::Pan => self.track.update_channel_display(ctx, display, GRID_ELEMENT_CHANNEL_PAN, false, true),
            MixKind::Crossfade => self.track.update_channel_display(ctx, display, GRID_ELEMENT_CHANNEL_CROSSFADER, false, false),
            MixKind::Send(send_index) => self.update_send_display2(ctx, display, send_index),
        }
    }
}

impl Mode for MixMode {
    fn name(&self) -> &str {
        self.track.base.name()
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.track.base.on_activate(ctx);
    }

    fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.track.base.on_deactivate(ctx);
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        self.track.button_color(ctx, button)
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        update_display(self, ctx);
    }

    fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.track.base.knob_value(ctx, index)
    }

    fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.track.on_knob_touch(ctx, index, is_touched);

        // TrackMode and SendMode: Shift+Select+touch toggles the send
        if matches!(self.kind, MixKind::Track | MixKind::Send(_)) && is_touched && ctx.surface.is_shift_pressed() && ctx.surface.is_select_pressed() {
            let parameter = self.track.base.parameter(ctx, index);
            if is_send(&parameter) {
                ctx.surface.set_trigger_consumed(ButtonId::Select);
                toggle_send_enabled(ctx, &parameter);
            }
        }
    }

    fn is_any_knob_touched(&self) -> bool {
        self.track.base.is_any_knob_touched()
    }

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, index: usize, event: ButtonEvent) {
        self.track.on_button(ctx, row, index, event);
    }

    fn select_item(&mut self, ctx: &mut Ctx, index: usize) {
        self.track.base.select_item(ctx, index);
    }

    fn select_previous_item(&mut self, ctx: &mut Ctx) {
        self.track.base.select_previous_item(ctx);
    }

    fn select_next_item(&mut self, ctx: &mut Ctx) {
        self.track.base.select_next_item(ctx);
    }

    fn select_previous_item_page(&mut self, ctx: &mut Ctx) {
        self.track.select_previous_item_page(ctx);
    }

    fn select_next_item_page(&mut self, ctx: &mut Ctx) {
        self.track.select_next_item_page(ctx);
    }

    fn select_item_page(&mut self, ctx: &mut Ctx, page: usize) {
        self.track.base.select_item_page(ctx, page);
    }

    fn has_previous_item(&self, ctx: &Ctx) -> bool {
        self.track.has_previous_item(ctx)
    }

    fn has_next_item(&self, ctx: &Ctx) -> bool {
        self.track.has_next_item(ctx)
    }

    fn has_previous_item_page(&self, ctx: &Ctx) -> bool {
        self.track.has_previous_item_page(ctx)
    }

    fn has_next_item_page(&self, ctx: &Ctx) -> bool {
        self.track.has_next_item_page(ctx)
    }

    impl_as_any!();
}
