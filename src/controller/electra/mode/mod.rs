// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The modes of the Electra One: one per page of the DrivenByMoss preset (former
//! `AbstractElectraOneBaseMode`, `AbstractElectraOneMode`, `AbstractElectraOneMiniMode` and their
//! subclasses).
//!
//! Each mode writes its controls into its [`PageCache`] in `update_display` and flushes it. The
//! knobs are bound to the parameters of the mode's provider; a knob touch (a SysEx event of the
//! device) touches the parameter of the control.

pub mod device;
pub mod mini;
pub mod mixer;
pub mod session;
pub mod transport;

use std::rc::Rc;

use super::colors::{self as colors, COLOR_BUTTON_STATE_OFF};
use super::definition::ElectraVariant;
use super::page_cache::PageCache;
use crate::config::values::fix_ascii;
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::daw::util::optimize_name;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::mode::parameter_mode::{ModeBank, ParameterModeBase};
use crate::framework::parameterprovider::{CustomParameter, ParamRef, PlayPositionParameter, TempoParameter};
use crate::midi::ButtonEvent;

/// The first group ID of the normal model's pages (former `AbstractElectraOneMode.GROUP_OFFSET`).
pub const GROUP_OFFSET: i32 = 500;
/// The first group ID of the Mini's pages (former `AbstractElectraOneMiniMode.GROUP_OFFSET`).
pub const MINI_GROUP_OFFSET: i32 = 8;

/// The text of a value as the modes send it: ASCII, at most 15 characters.
pub fn value_text(text: &str) -> String {
    optimize_name(&fix_ascii(text), 15)
}

/// The play position as a parameter (former `new PlayPositionParameter (...)`).
pub fn play_position_parameter() -> ParamRef {
    ParamRef::Custom(Rc::new(PlayPositionParameter))
}

/// The tempo as a parameter (former `new TempoParameter (...)`).
pub fn tempo_parameter() -> ParamRef {
    ParamRef::Custom(Rc::new(TempoParameter))
}

/// A custom parameter (e.g. the cue volume).
pub fn custom(parameter: impl CustomParameter + 'static) -> ParamRef {
    ParamRef::Custom(Rc::new(parameter))
}

/// The base of all modes (former `AbstractElectraOneBaseMode`, a `DefaultTrackMode`).
pub struct ElectraModeBase {
    pub base: ParameterModeBase,
    pub page_cache: PageCache,
}

impl ElectraModeBase {
    pub fn new(variant: ElectraVariant, page_index: usize, name: &str) -> Self {
        let (rows, columns) = variant.grid();
        let group_offset = match variant {
            ElectraVariant::Normal => GROUP_OFFSET,
            ElectraVariant::Mini => MINI_GROUP_OFFSET,
        };
        ElectraModeBase {
            base: ParameterModeBase::new(name, false, ModeBank::TrackBank, variant.knob_ids()),
            page_cache: PageCache::new(page_index, rows, columns, group_offset),
        }
    }

    pub fn name(&self) -> &str {
        self.base.name()
    }

    /// Former `onActivate`: everything is sent again. The app layer also calls it for the active
    /// mode to re-bind the knobs when a bank size changes (`ParserEvent::RebindKnobs`, the Rust
    /// replacement of the parameter observers); the page is then not sent again.
    pub fn on_activate(&mut self, ctx: &mut Ctx) {
        if !self.base.is_active() {
            self.page_cache.reset();
        }
        self.base.on_activate(ctx);
    }

    pub fn on_deactivate(&mut self, ctx: &mut Ctx) {
        self.base.on_deactivate(ctx);
    }

    /// Former `getButtonColor`: the pads have no light.
    pub fn button_color(&self) -> i32 {
        COLOR_BUTTON_STATE_OFF
    }

    /// Former `onKnobTouch`: touch the parameter of the knob.
    pub fn on_knob_touch(&mut self, ctx: &mut Ctx, index: usize, is_touched: bool) {
        self.base.set_touched_knob(index, is_touched);
        self.base.parameter(ctx, index).touch_value(ctx, is_touched);
    }

    pub fn knob_value(&self, ctx: &Ctx, index: usize) -> i32 {
        self.base.knob_value(ctx, index)
    }
}

/// Implements the functions of the `Mode` trait which all Electra modes share; `$base` is the
/// field with the [`ElectraModeBase`].
macro_rules! electra_mode_common {
    ($($base:ident).+) => {
        fn name(&self) -> &str {
            self.$($base).+.name()
        }

        fn on_activate(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($base).+.on_activate(ctx);
        }

        fn on_deactivate(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
            self.$($base).+.on_deactivate(ctx);
        }

        fn button_color(&self, _ctx: &$crate::framework::ctx::Ctx, _button: $crate::framework::ids::ButtonId) -> i32 {
            self.$($base).+.button_color()
        }

        fn on_knob_touch(&mut self, ctx: &mut $crate::framework::ctx::Ctx, index: usize, is_touched: bool) {
            self.$($base).+.on_knob_touch(ctx, index, is_touched);
        }

        fn is_any_knob_touched(&self) -> bool {
            self.$($base).+.base.is_any_knob_touched()
        }

        fn knob_value(&self, ctx: &$crate::framework::ctx::Ctx, index: usize) -> i32 {
            self.$($base).+.knob_value(ctx, index)
        }

        $crate::impl_as_any!();
    };
}
pub(crate) use electra_mode_common;

/// The part of the modes of the normal model (former `AbstractElectraOneMode`): the master
/// column (master volume, play position, record and play).
pub struct NormalModeBase {
    pub electra: ElectraModeBase,
    pub play_command: PlayCommand,
    timer_counter: i32,
}

impl NormalModeBase {
    pub fn new(page_index: usize, name: &str) -> Self {
        NormalModeBase { electra: ElectraModeBase::new(ElectraVariant::Normal, page_index, name), play_command: PlayCommand::new(), timer_counter: 0 }
    }

    pub fn cache(&mut self) -> &mut PageCache {
        &mut self.electra.page_cache
    }

    /// Toggle play (former `playCommand.execute (ButtonEvent.UP, 127)`).
    pub fn toggle_play(&mut self, ctx: &mut Ctx) {
        self.play_command.execute(ctx, ButtonEvent::Up, 127);
    }

    /// Former `updateMasterColumn`.
    pub fn update_master_column(&mut self, ctx: &Ctx) {
        let master = ctx.model.master_track();
        let cache = &mut self.electra.page_cache;
        // Master
        cache.update_color(0, 5, master.color());
        cache.update_value(0, 5, master.volume(), Some(&value_text(&master.volume_str())));
        let transport = ctx.model.transport();
        if self.timer_counter > 10 {
            cache.update_value(1, 5, 0, Some(&value_text(transport.beat_text())));
            cache.update_element(1, 5, Some(&value_text(transport.position_text())), None, None);
            self.timer_counter = 0;
        }
        self.timer_counter += 1;
        // Transport
        cache.update_color(4, 5, if transport.is_recording() { colors::RECORD_ON } else { colors::RECORD_OFF });
        cache.update_color(5, 5, if transport.is_playing() { colors::PLAY_ON } else { colors::PLAY_OFF });
    }

    /// Flush the page cache.
    pub fn flush(&mut self, ctx: &mut Ctx) {
        self.electra.page_cache.flush(ctx);
    }
}

/// The state of a track as the modes show it (former `getTrack (index)`, or `EmptyTrack` if it
/// does not exist).
pub struct TrackInfo {
    pub exists: bool,
    pub position: i32,
    pub name: String,
    pub volume: i32,
    pub volume_str: String,
    pub pan: i32,
    pub pan_str: String,
    pub color: ColorEx,
    pub is_rec_arm: bool,
    pub is_mute: bool,
    pub is_solo: bool,
    pub is_selected: bool,
}

impl TrackInfo {
    /// The track at the index of the page of the current track bank.
    pub fn of(ctx: &Ctx, index: usize) -> TrackInfo {
        let track = ctx.model.current_track_bank().item(index);
        Self::from_track(track)
    }

    pub fn from_track(track: &crate::daw::track::Track) -> TrackInfo {
        if !track.does_exist() {
            return TrackInfo::empty();
        }
        TrackInfo {
            exists: true,
            position: track.position(),
            name: track.name().to_string(),
            volume: track.volume(),
            volume_str: track.volume_str().to_string(),
            pan: track.pan(),
            pan_str: track.pan_str().to_string(),
            color: track.color(),
            is_rec_arm: track.is_rec_arm(),
            is_mute: track.is_mute(),
            is_solo: track.is_solo(),
            is_selected: track.is_selected(),
        }
    }

    /// Former `EmptyTrack`.
    pub fn empty() -> TrackInfo {
        TrackInfo {
            exists: false,
            position: -1,
            name: String::new(),
            volume: 0,
            volume_str: String::new(),
            pan: 0,
            pan_str: String::new(),
            color: ColorEx::BLACK,
            is_rec_arm: false,
            is_mute: false,
            is_solo: false,
            is_selected: false,
        }
    }

    /// The group label of the track: "position: name", empty if it does not exist.
    pub fn group_label(&self) -> String {
        if self.exists {
            format!("{}: {}", self.position + 1, self.name)
        } else {
            String::new()
        }
    }
}

/// Former `DefaultTrackMode.getTrack`: true if the track at the index of the page exists.
pub fn track_exists(ctx: &Ctx, index: usize) -> bool {
    ctx.model.current_track_bank().item(index).does_exist()
}
