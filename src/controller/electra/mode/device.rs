// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device, equalizer and project/track parameter pages of the normal model (former
//! `DeviceMode`, `EqualizerMode` and `ProjectTrackMode`).

use super::{electra_mode_common, play_position_parameter, tempo_parameter, value_text, NormalModeBase};
use crate::config::values::fix_ascii;
use crate::controller::electra::colors;
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::daw::constants::DeviceId;
use crate::daw::device::{EqualizerBandType, SpecificDevice};
use crate::daw::events::ParameterBankId;
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::parameterprovider::{
    parameter_bank, parameter_bank_mut, provider, BankParameterProvider, CombinedParameterProvider, EmptyParameterProvider, FixedParameterProvider, ParamRef,
    RangeFilterParameterProvider, SharedProvider,
};
use crate::midi::ButtonEvent;

fn fixed(parameters: Vec<ParamRef>) -> SharedProvider {
    provider(FixedParameterProvider::new(parameters))
}

fn empty(size: usize) -> SharedProvider {
    provider(EmptyParameterProvider::new(size))
}

/// The name, value and text of the parameter at the index of the page of a parameter bank.
fn bank_parameter(ctx: &Ctx, bank: ParameterBankId, index: usize) -> (bool, String, i32, String) {
    let parameter = ParamRef::BankItem { bank, index };
    let exists = parameter.does_exist(ctx);
    (exists, parameter.name(ctx), parameter.value(ctx), parameter.displayed_value(ctx))
}

/// The names of the parameter pages of a bank (of the current page of pages) and the index of the
/// selected one.
fn page_names(ctx: &Ctx, bank: ParameterBankId, count: usize) -> (Vec<String>, usize) {
    match parameter_bank(&ctx.model, bank) {
        Some(bank) => {
            let pages = bank.page_bank();
            ((0..count).map(|i| pages.item(i)).collect(), pages.selected_item_index())
        }
        None => (vec![String::new(); count], usize::MAX),
    }
}

// ---------------------------------------------------------------------------------------------

/// The device page (former `DeviceMode`): 8 parameters of the cursor device, its parameter pages
/// and the devices of the track.
pub struct DeviceMode {
    pub base: NormalModeBase,
}

impl DeviceMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = NormalModeBase::new(2, "Parameters");
        let bank = provider(BankParameterProvider::new(ctx, ParameterBankId::CursorDevice));
        let empty_one = empty(1);
        let parameters = CombinedParameterProvider::new(vec![
            // Row 1
            empty_one.clone(),
            provider(RangeFilterParameterProvider::new(bank.clone(), 0, 4)),
            fixed(vec![ParamRef::MasterVolume]),
            // Row 2
            empty_one,
            provider(RangeFilterParameterProvider::new(bank, 4, 4)),
            fixed(vec![play_position_parameter()]),
            // These 4 rows only contain buttons
            empty(4 * 6),
        ]);
        base.electra.base.set_parameter_provider(ctx, provider(parameters));
        DeviceMode { base }
    }
}

impl Mode for DeviceMode {
    electra_mode_common!(base.electra);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match column {
            0 => {
                let device = ctx.model.cursor_device_mut();
                match row {
                    0 => device.toggle_enabled_state(),
                    1 => device.toggle_window_open(),
                    2 => {
                        if let Some(bank) = device.parameter_bank_mut() {
                            bank.page_bank_select_next_page();
                        }
                    }
                    3 => {
                        if let Some(bank) = device.parameter_bank_mut() {
                            bank.page_bank_select_previous_page();
                        }
                    }
                    4 => device.device_bank_mut().select_next_page(),
                    5 => device.device_bank_mut().select_previous_page(),
                    _ => {}
                }
            }
            5 => match row {
                2 => {
                    let pinned = ctx.model.cursor_device().is_pinned();
                    ctx.model.cursor_device_mut().toggle_pinned();
                    if pinned == ctx.model.is_cursor_track_pinned() {
                        ctx.model.toggle_cursor_track_pinned();
                    }
                }
                3 => ctx.model.cursor_device_mut().toggle_expanded(),
                4 => ctx.model.transport_mut().start_recording(),
                5 => self.base.toggle_play(ctx),
                _ => {}
            },
            _ => {
                if row < 2 {
                    return;
                }
                let index = (row % 2) * 4 + column - 1;
                if row == 2 || row == 3 {
                    if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                        bank.select_page(index);
                    }
                } else {
                    ctx.model.cursor_device_mut().device_bank_mut().item_mut(index).select();
                }
            }
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let (pages, selected_page) = page_names(ctx, ParameterBankId::CursorDevice, 8);
        let device = ctx.model.cursor_device();
        for (i, page) in pages.iter().enumerate() {
            let (exists, name, value, displayed) = bank_parameter(ctx, ParameterBankId::CursorDevice, i);
            let sibling = device.device_bank().item(i);
            let (device_exists, device_name) = (sibling.does_exist(), sibling.name().to_string());
            let cache = self.base.cache();
            let mut row = i / 4;
            let column = 1 + i % 4;
            cache.update_element(row, column, Some(&if exists { fix_ascii(&name) } else { String::new() }), Some(if exists { ColorEx::ORANGE } else { ColorEx::BLACK }), Some(exists));
            cache.update_value(row, column, value, Some(&if exists { value_text(&displayed) } else { " ".to_string() }));

            // Set the page names
            row += 2;
            let page_exists = !page.trim().is_empty();
            let color = if selected_page == i { colors::PARAM_PAGE_SELECTED } else { colors::PARAM_PAGE };
            cache.update_element(row, column, Some(&if page_exists { fix_ascii(page) } else { " ".to_string() }), Some(if page_exists { color } else { ColorEx::BLACK }), Some(page_exists));

            // Set the device names
            row += 2;
            let device_color = if device.index() == i as i32 { colors::DEVICE_SELECTED } else { colors::DEVICE };
            cache.update_element(
                row,
                column,
                Some(&if device_exists { fix_ascii(&device_name) } else { " ".to_string() }),
                Some(if device_exists { device_color } else { ColorEx::BLACK }),
                Some(device_exists),
            );
        }
        let cache = self.base.cache();
        cache.update_color(0, 0, if device.is_enabled() { colors::DEVICE_ON } else { colors::DEVICE_OFF });
        cache.update_color(1, 0, if device.is_window_open() { colors::WINDOW_OPEN } else { colors::WINDOW });
        cache.update_color(2, 5, if device.is_pinned() { colors::PINNED_ON } else { colors::PINNED_OFF });
        cache.update_color(3, 5, if device.is_expanded() { colors::EXPANDED_ON } else { colors::EXPANDED_OFF });
        self.base.update_master_column(ctx);
        self.base.flush(ctx);
    }
}

// ---------------------------------------------------------------------------------------------

/// A parameter of a band of the equalizer device (former `getFrequencyParameter` etc.): the
/// parameter at its absolute position in the parameter bank of the equalizer.
fn eq_parameter(position: usize) -> ParamRef {
    ParamRef::BankPosition { bank: ParameterBankId::Eq, position }
}

fn eq_device(ctx: &Ctx) -> Option<&SpecificDevice> {
    ctx.model.specific_device(DeviceId::Eq)
}

/// The equalizer page (former `EqualizerMode`): type, frequency, gain and Q of 6 bands.
pub struct EqualizerMode {
    pub base: NormalModeBase,
}

impl EqualizerMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = NormalModeBase::new(3, "Equalizer");
        let empty_one = empty(1);
        let mut providers = Vec::new();
        for band in 0..6 {
            providers.push(empty_one.clone());
            let mut parameters = vec![
                ParamRef::EqBandType { band },
                eq_parameter(SpecificDevice::frequency_parameter_index(band)),
                eq_parameter(SpecificDevice::gain_parameter_index(band)),
                eq_parameter(SpecificDevice::q_parameter_index(band)),
            ];
            match band {
                0 => parameters.push(ParamRef::MasterVolume),
                1 => parameters.push(play_position_parameter()),
                _ => {}
            }
            providers.push(fixed(parameters));
            if band >= 2 {
                providers.push(empty_one.clone());
            }
        }
        base.electra.base.set_parameter_provider(ctx, provider(CombinedParameterProvider::new(providers)));
        EqualizerMode { base }
    }
}

impl Mode for EqualizerMode {
    electra_mode_common!(base.electra);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        if column == 0 {
            if let Some(device) = ctx.model.specific_device_mut(DeviceId::Eq) {
                let is_off = device.type_id(row) == EqualizerBandType::Off;
                device.set_type(row, if is_off { EqualizerBandType::Bell } else { EqualizerBandType::Off });
            }
            return;
        }
        if column == 5 {
            match row {
                // Add an equalizer
                2 => {
                    if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
                        track.add_equalizer_device();
                    }
                }
                // On/Off
                3 => {
                    if let Some(device) = ctx.model.specific_device_mut(DeviceId::Eq) {
                        device.toggle_enabled_state();
                    }
                }
                // Record
                4 => ctx.model.transport_mut().start_recording(),
                // Play
                5 => self.base.toggle_play(ctx),
                _ => {}
            }
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        for row in 0..6 {
            let type_id = eq_device(ctx).map(|d| d.type_id(row)).unwrap_or_default();
            let values: Vec<(i32, String)> = [
                ParamRef::EqBandType { band: row },
                eq_parameter(SpecificDevice::frequency_parameter_index(row)),
                eq_parameter(SpecificDevice::gain_parameter_index(row)),
                eq_parameter(SpecificDevice::q_parameter_index(row)),
            ]
            .iter()
            .map(|p| (p.value(ctx), value_text(&p.displayed_value(ctx))))
            .collect();
            let cache = self.base.cache();
            cache.update_color(row, 0, if type_id != EqualizerBandType::Off { colors::BAND_ON } else { colors::BAND_OFF });
            for (i, (value, text)) in values.iter().enumerate() {
                cache.update_value(row, 1 + i, *value, Some(text));
            }
            for column in 2..5 {
                cache.update_element(row, column, None, None, Some(true));
            }
        }
        let is_on = eq_device(ctx).is_some_and(|d| d.does_exist() && d.is_enabled());
        self.base.cache().update_color(3, 5, if is_on { colors::BAND_ON } else { colors::BAND_OFF });
        self.base.update_master_column(ctx);
        self.base.flush(ctx);
    }
}

// ---------------------------------------------------------------------------------------------

/// The project and track parameters page (former `ProjectTrackMode`): 8 project parameters, 8
/// parameters of the cursor track, their pages, and the tempo.
pub struct ProjectTrackMode {
    pub base: NormalModeBase,
}

impl ProjectTrackMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = NormalModeBase::new(6, "Project-Track");
        let project = provider(BankParameterProvider::new(ctx, ParameterBankId::Project));
        let track = provider(BankParameterProvider::new(ctx, ParameterBankId::CursorTrack));
        let empty_one = empty(1);
        let parameters = CombinedParameterProvider::new(vec![
            // Row 1
            empty_one.clone(),
            provider(RangeFilterParameterProvider::new(project.clone(), 0, 4)),
            fixed(vec![ParamRef::MasterVolume]),
            // Row 2
            empty_one.clone(),
            provider(RangeFilterParameterProvider::new(project, 4, 4)),
            fixed(vec![play_position_parameter()]),
            // Row 3
            empty(5),
            fixed(vec![tempo_parameter()]),
            // Row 4
            empty(6),
            // Row 5
            empty_one.clone(),
            provider(RangeFilterParameterProvider::new(track.clone(), 0, 4)),
            empty_one.clone(),
            // Row 6
            empty_one.clone(),
            provider(RangeFilterParameterProvider::new(track, 4, 4)),
            empty_one,
        ]);
        base.electra.base.set_parameter_provider(ctx, provider(parameters));
        ProjectTrackMode { base }
    }
}

impl Mode for ProjectTrackMode {
    electra_mode_common!(base.electra);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match row {
            2 => {
                if column < 5 {
                    if let Some(bank) = parameter_bank_mut(&mut ctx.model, ParameterBankId::Project) {
                        bank.select_page(column);
                    }
                }
            }
            3 => {
                if column < 5 {
                    if let Some(bank) = parameter_bank_mut(&mut ctx.model, ParameterBankId::CursorTrack) {
                        bank.select_page(column);
                    }
                } else {
                    ctx.model.transport_mut().tap_tempo();
                }
            }
            4 if column == 5 => ctx.model.transport_mut().start_recording(),
            5 if column == 5 => self.base.toggle_play(ctx),
            // 0 + 1, not used
            _ => {}
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        for i in 0..8 {
            let project = bank_parameter(ctx, ParameterBankId::Project, i);
            let track = bank_parameter(ctx, ParameterBankId::CursorTrack, i);
            let cache = self.base.cache();
            let row = i / 4;
            let column = 1 + i % 4;
            for (row, (exists, name, value, displayed), color) in [(row, project, ColorEx::PINK), (row + 4, track, ColorEx::MINT)] {
                cache.update_element(row, column, Some(&if exists { fix_ascii(&name) } else { String::new() }), Some(if exists { color } else { ColorEx::BLACK }), Some(exists));
                cache.update_value(row, column, value, Some(&if exists { value_text(&displayed) } else { " ".to_string() }));
            }
        }

        let (project_pages, project_selected) = page_names(ctx, ParameterBankId::Project, 5);
        let (track_pages, track_selected) = page_names(ctx, ParameterBankId::CursorTrack, 5);
        let cache = self.base.cache();
        for column in 0..5 {
            // Set the project and track page names
            for (row, pages, selected, on, off) in [
                (2, &project_pages, project_selected, colors::PROJECT_PARAM_PAGE_SELECTED, colors::PROJECT_PARAM_PAGE),
                (3, &track_pages, track_selected, colors::TRACK_PARAM_PAGE_SELECTED, colors::TRACK_PARAM_PAGE),
            ] {
                let page = &pages[column];
                let page_exists = !page.trim().is_empty();
                let color = if selected == column { on } else { off };
                cache.update_element(row, column, Some(&if page_exists { fix_ascii(page) } else { " ".to_string() }), Some(if page_exists { color } else { ColorEx::BLACK }), Some(page_exists));
            }
        }

        // Row 3 / 4
        let transport = ctx.model.transport();
        cache.update_value(2, 5, 0, Some(&transport.format_tempo(transport.tempo())));

        // Maybe support page bank navigation later...
        for row in [0, 1, 4, 5] {
            cache.update_color(row, 0, ColorEx::BLACK);
        }
        for row in [0, 1, 4, 5] {
            cache.update_element(row, 0, Some(" "), Some(ColorEx::BLACK), Some(false));
        }
        self.base.update_master_column(ctx);
        self.base.flush(ctx);
    }
}
