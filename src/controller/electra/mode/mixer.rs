// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The mixer and the sends page of the normal model (former `MixerMode` and `SendsMode`).

use super::{electra_mode_common, play_position_parameter, track_exists, value_text, NormalModeBase, TrackInfo};
use crate::controller::electra::colors;
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::parameterprovider::{
    provider, CombinedParameterProvider, EmptyParameterProvider, FixedParameterProvider, PanParameterProvider, ParamRef, SendParameterProvider,
    VolumeParameterProvider,
};
use crate::midi::ButtonEvent;

/// The first group of the track labels of the mixer page (former `FIRST_TRACK_GROUP`).
const MIXER_FIRST_TRACK_GROUP: i32 = 501;
/// The first group of the track labels of the sends page.
const SENDS_FIRST_TRACK_GROUP: i32 = 510;

/// The buttons of the master column of the mixer and sends page (column 6): track page, record,
/// play. Returns false for the other rows.
fn on_master_column_button(base: &mut NormalModeBase, ctx: &mut Ctx, row: usize) {
    match row {
        // Next track page
        2 => ctx.model.current_track_bank_mut().select_next_page(),
        // Previous track page
        3 => ctx.model.current_track_bank_mut().select_previous_page(),
        // Record
        4 => ctx.model.transport_mut().start_recording(),
        // Play
        5 => base.toggle_play(ctx),
        _ => {}
    }
}

/// The mixer page (former `MixerMode`): volume and panorama of 5 tracks and the master, rec arm,
/// mute, solo and select pads.
pub struct MixerMode {
    pub base: NormalModeBase,
}

impl MixerMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = NormalModeBase::new(0, "Volume");
        let parameters = CombinedParameterProvider::new(vec![
            // Row 1
            provider(VolumeParameterProvider::new(ctx)),
            provider(FixedParameterProvider::single(ParamRef::MasterVolume)),
            // Row 2
            provider(PanParameterProvider::new(ctx)),
            provider(FixedParameterProvider::single(play_position_parameter())),
            // These 4 rows only contain buttons
            provider(EmptyParameterProvider::new(4 * 6)),
        ]);
        base.electra.base.set_parameter_provider(ctx, provider(parameters));
        MixerMode { base }
    }
}

impl Mode for MixerMode {
    electra_mode_common!(base.electra);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if column == 5 {
            on_master_column_button(&mut self.base, ctx, row);
            return;
        }
        if !track_exists(ctx, column) {
            return;
        }
        let track = ctx.model.current_track_bank_mut().item_mut(column);
        match row {
            // Record Arm
            2 => track.toggle_rec_arm(),
            // Mute
            3 => track.toggle_mute(),
            // Solo
            4 => track.toggle_solo(),
            // Select
            5 => track.select(),
            _ => {}
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        for column in 0..5 {
            let track = TrackInfo::of(ctx, column);
            let exists = Some(track.exists);
            let cache = self.base.cache();
            cache.update_group_label(MIXER_FIRST_TRACK_GROUP + column as i32, &track.group_label());
            let volume_text = if track.exists { value_text(&track.volume_str) } else { " ".to_string() };
            cache.update_value(0, column, track.volume, Some(&volume_text));
            cache.update_value(1, column, track.pan, Some(&value_text(&track.pan_str)));
            let label = |text: &'static str| if track.exists { text } else { " " };
            cache.update_element(0, column, Some(label("VOLUME")), Some(track.color), exists);
            cache.update_element(1, column, Some(label("PAN")), Some(track.color), exists);
            cache.update_element(2, column, Some(label("REC ARM")), Some(if track.is_rec_arm { colors::REC_ARM_ON } else { colors::REC_ARM_OFF }), exists);
            cache.update_element(3, column, Some(label("MUTE")), Some(if track.is_mute { colors::MUTE_ON } else { colors::MUTE_OFF }), exists);
            cache.update_element(4, column, Some(label("SOLO")), Some(if track.is_solo { colors::SOLO_ON } else { colors::SOLO_OFF }), exists);
            cache.update_element(5, column, Some(label("SELECT")), Some(if track.is_selected { colors::SELECT_ON } else { colors::SELECT_OFF }), exists);
        }
        self.base.update_master_column(ctx);
        self.base.flush(ctx);
    }
}

/// The sends page (former `SendsMode`): 6 sends of 5 tracks.
pub struct SendsMode {
    pub base: NormalModeBase,
}

impl SendsMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = NormalModeBase::new(1, "Sends");
        let mut providers = Vec::new();
        for row in 0..6 {
            providers.push(provider(SendParameterProvider::new(ctx, row, 0)));
            providers.push(match row {
                0 => provider(FixedParameterProvider::single(ParamRef::MasterVolume)),
                1 => provider(FixedParameterProvider::single(play_position_parameter())),
                _ => provider(EmptyParameterProvider::new(1)),
            });
        }
        base.electra.base.set_parameter_provider(ctx, provider(CombinedParameterProvider::new(providers)));
        SendsMode { base }
    }
}

impl Mode for SendsMode {
    electra_mode_common!(base.electra);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down || column != 5 {
            return;
        }
        on_master_column_button(&mut self.base, ctx, row);
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        for column in 0..5 {
            let track = TrackInfo::of(ctx, column);
            let group_label = track.group_label();
            let sends: Vec<(bool, i32, String, String)> = (0..6)
                .map(|row| {
                    let item = ctx.model.current_track_bank().item(column);
                    let send_bank = item.send_bank();
                    if !track.exists || row >= send_bank.item_count() {
                        return (false, 0, String::new(), String::new());
                    }
                    let send = send_bank.item(row);
                    (send.does_exist(), send.value(), send.name().to_string(), send.displayed_value())
                })
                .collect();
            let cache = self.base.cache();
            cache.update_group_label(SENDS_FIRST_TRACK_GROUP + column as i32, &group_label);
            for (row, (exists, value, name, displayed)) in sends.into_iter().enumerate() {
                let text = if exists { value_text(&displayed) } else { " ".to_string() };
                cache.update_value(row, column, value, Some(&text));
                cache.update_element(row, column, Some(&name), Some(if exists { track.color } else { ColorEx::BLACK }), Some(exists));
            }
        }
        self.base.update_master_column(ctx);
        self.base.flush(ctx);
    }
}
