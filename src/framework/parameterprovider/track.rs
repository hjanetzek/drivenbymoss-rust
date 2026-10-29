// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Track parameter providers (former `parameterprovider.track`: `VolumeParameterProvider`,
//! `PanParameterProvider`, `SendParameterProvider`, `SelectedTrackParameterProvider`). All
//! providers work on the current track bank of the model.

use super::{ParamRef, ParameterProvider};
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;

fn track_color(ctx: &Ctx, index: usize) -> Option<ColorEx> {
    Some(ctx.model.current_track_bank().item(index).color())
}

/// Provides the volumes of the tracks of the page (former `VolumeParameterProvider`).
pub struct VolumeParameterProvider {
    size: usize,
}

impl VolumeParameterProvider {
    pub fn new(ctx: &Ctx) -> Self {
        VolumeParameterProvider { size: ctx.model.current_track_bank().page_size() }
    }
}

impl ParameterProvider for VolumeParameterProvider {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, _ctx: &Ctx, index: usize) -> ParamRef {
        ParamRef::TrackVolume { index }
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        track_color(ctx, index)
    }
}

/// Provides the panoramas of the tracks of the page (former `PanParameterProvider`).
pub struct PanParameterProvider {
    size: usize,
}

impl PanParameterProvider {
    pub fn new(ctx: &Ctx) -> Self {
        PanParameterProvider { size: ctx.model.current_track_bank().page_size() }
    }
}

impl ParameterProvider for PanParameterProvider {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, _ctx: &Ctx, index: usize) -> ParamRef {
        ParamRef::TrackPan { index }
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        track_color(ctx, index)
    }
}

/// Provides a send of the tracks of the page or all sends of the selected track (former
/// `SendParameterProvider`).
pub struct SendParameterProvider {
    size: usize,
    send_index: i32,
    send_offset: usize,
}

impl SendParameterProvider {
    /// With a send index of -1 the sends of the selected track are provided (starting at the
    /// offset), otherwise the send `send_offset + send_index` of all tracks of the page.
    pub fn new(ctx: &Ctx, send_index: i32, send_offset: usize) -> Self {
        SendParameterProvider { size: ctx.model.current_track_bank().page_size(), send_index, send_offset }
    }
}

impl ParameterProvider for SendParameterProvider {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, _ctx: &Ctx, index: usize) -> ParamRef {
        if self.send_index < 0 {
            ParamRef::SelectedTrackSend { send: self.send_offset + index }
        } else {
            ParamRef::TrackSend { index, send: self.send_offset + self.send_index as usize }
        }
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        let bank = ctx.model.current_track_bank();
        if self.send_index < 0 {
            let send_bank = bank.selected_item()?.send_bank();
            let idx = self.send_offset + index;
            (idx < send_bank.item_count()).then(|| send_bank.item(idx).color())
        } else {
            let send_bank = bank.item(index).send_bank();
            (send_bank.item_count() > 0).then(|| send_bank.item(self.send_offset + self.send_index as usize).color())
        }
    }
}

/// Provides volume, panorama and the sends of the selected track (former
/// `SelectedTrackParameterProvider`).
pub struct SelectedTrackParameterProvider {
    size: usize,
}

impl SelectedTrackParameterProvider {
    pub fn new(ctx: &Ctx) -> Self {
        SelectedTrackParameterProvider { size: ctx.model.current_track_bank().page_size() }
    }
}

impl ParameterProvider for SelectedTrackParameterProvider {
    fn size(&self) -> usize {
        self.size
    }

    fn get(&self, ctx: &Ctx, index: usize) -> ParamRef {
        if ctx.model.current_track_bank().selected_item().is_none() {
            return ParamRef::Empty;
        }
        match index {
            0 => ParamRef::SelectedTrackVolume,
            1 => ParamRef::SelectedTrackPan,
            _ => ParamRef::SelectedTrackSend { send: index - 2 },
        }
    }

    fn color(&self, ctx: &Ctx, index: usize) -> Option<ColorEx> {
        let track = ctx.model.current_track_bank().selected_item()?;
        if index < 2 {
            return Some(track.color());
        }
        let send_bank = track.send_bank();
        (index - 2 < send_bank.item_count()).then(|| send_bank.item(index - 2).color())
    }
}
