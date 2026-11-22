// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The transport page of the normal model (former `TransportMode`).

use std::rc::Rc;

use super::{custom, electra_mode_common, play_position_parameter, tempo_parameter, value_text, NormalModeBase};
use crate::controller::electra::colors;
use crate::daw::bank::BankItem;
use crate::daw::color::ColorEx;
use crate::daw::constants::AutomationMode;
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::general::{focused_parameter, FocusedParameter};
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::framework::parameterprovider::{provider, CombinedParameterProvider, CueVolumeParameter, EmptyParameterProvider, FixedParameterProvider, ParamRef};
use crate::midi::ButtonEvent;

/// Select the selected slot of the cursor track to record and launch it.
pub fn record_selected_slot(ctx: &mut Ctx) {
    let Some(index) = ctx.model.selected_slot().map(|slot| slot.index()) else {
        return;
    };
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let slot = ctx.model.cursor_track_mut().slot_bank_mut().item_mut(index);
    if !slot.is_recording() {
        slot.start_recording();
    }
    slot.launch(true, false);
}

/// The transport page (former `TransportMode`).
pub struct TransportMode {
    pub base: NormalModeBase,
    new_command: NewCommand,
    launch_markers: bool,
}

impl TransportMode {
    pub fn new(ctx: &Ctx) -> Self {
        let mut base = NormalModeBase::new(4, "Transport");
        let fixed = |parameter: ParamRef| provider(FixedParameterProvider::single(parameter));
        let empty = |size: usize| provider(EmptyParameterProvider::new(size));
        let empty5 = empty(5);
        let parameters = CombinedParameterProvider::new(vec![
            // Row 1
            empty5.clone(),
            fixed(ParamRef::MasterVolume),
            // Row 2
            empty5.clone(),
            fixed(play_position_parameter()),
            // Row 3
            empty5,
            fixed(tempo_parameter()),
            // Row 4
            empty(6),
            // Row 5
            empty(1),
            fixed(ParamRef::MetronomeVolume),
            fixed(custom(CueVolumeParameter)),
            empty(3),
            // Row 6
            empty(2),
            fixed(ParamRef::Custom(Rc::new(FocusedParameter))),
            empty(3),
        ]);
        base.electra.base.set_parameter_provider(ctx, provider(parameters));
        TransportMode { base, new_command: NewCommand::new(), launch_markers: false }
    }

    /// Former `getMarkerColor`.
    fn marker_color(color: ColorEx, does_exist: bool) -> ColorEx {
        if !does_exist {
            return ColorEx::BLACK;
        }
        if color == ColorEx::BLACK {
            ColorEx::GRAY
        } else {
            color
        }
    }

    /// Former `handleMarker`.
    fn handle_marker(&self, ctx: &mut Ctx, marker_index: usize) {
        let marker = ctx.model.marker_bank_mut().item_mut(marker_index);
        if self.launch_markers {
            marker.launch(true);
        } else {
            marker.select();
        }
    }
}

impl Mode for TransportMode {
    electra_mode_common!(base.electra);

    fn on_button(&mut self, ctx: &mut Ctx, row: usize, column: usize, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        match row {
            0 => match column {
                0 => self.new_command.execute_now(ctx),
                1 => {
                    if let Some(clip) = ctx.model.cursor_clip() {
                        clip.quantize(1.0);
                    }
                }
                2 => ctx.model.transport_mut().toggle_write_arranger_automation(),
                3 => ctx.model.transport_mut().set_automation_write_mode(AutomationMode::Read),
                4 => ctx.model.transport_mut().set_automation_write_mode(AutomationMode::Write),
                _ => {}
            },
            1 => match column {
                0 => record_selected_slot(ctx),
                1 => ctx.model.toggle_launcher_overdub(),
                2 => ctx.model.transport_mut().toggle_write_clip_launcher_automation(),
                3 => ctx.model.transport_mut().set_automation_write_mode(AutomationMode::Latch),
                4 => ctx.model.transport_mut().set_automation_write_mode(AutomationMode::Touch),
                _ => {}
            },
            2 => {
                if column == 0 {
                    ctx.model.marker_bank_mut().select_next_page();
                } else if column < 5 {
                    self.handle_marker(ctx, column - 1);
                }
            }
            3 => match column {
                0 => ctx.model.marker_bank_mut().select_previous_page(),
                5 => ctx.model.transport_mut().tap_tempo(),
                _ => self.handle_marker(ctx, 4 + column - 1),
            },
            4 => match column {
                0 => ctx.model.application_mut().redo(),
                4 => ctx.model.marker_bank_mut().add_marker(),
                5 => ctx.model.transport_mut().start_recording(),
                _ => {}
            },
            5 => match column {
                0 => ctx.model.application_mut().undo(),
                1 => ctx.model.transport_mut().toggle_metronome(),
                3 => ctx.model.toggle_overdub(),
                4 => self.launch_markers = !self.launch_markers,
                5 => self.base.toggle_play(ctx),
                _ => {}
            },
            _ => {}
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let transport = ctx.model.transport();
        let automation_write_mode = transport.automation_write_mode();
        let auto_mode = |mode: AutomationMode| if automation_write_mode == mode { colors::AUTO_MODE_ON } else { colors::AUTO_MODE_OFF };
        let markers: Vec<(bool, String, ColorEx)> = (0..8)
            .map(|i| {
                let marker = ctx.model.marker_bank().item(i);
                (marker.does_exist(), marker.name().to_string(), marker.color())
            })
            .collect();
        // The focused parameter is always present on Bitwig (the model setup wants it); an empty
        // one has the value 0 and no text
        let focused = focused_parameter(ctx);
        let (param_value, param_text) = (focused.value(ctx), focused.displayed_value(ctx));
        let cue = CueVolumeParameter;
        let cue_value = crate::framework::parameterprovider::CustomParameter::value(&cue, ctx);
        let cue_text = ctx.model.project().cue_volume_str().to_string();
        let launch_markers = self.launch_markers;

        let cache = self.base.cache();
        // Row 1
        cache.update_color(0, 2, if transport.is_writing_arranger_automation() { colors::AUTO_ON } else { colors::AUTO_OFF });
        cache.update_color(0, 3, auto_mode(AutomationMode::Read));
        cache.update_color(0, 4, auto_mode(AutomationMode::Write));

        // Row 2
        cache.update_color(1, 1, if ctx.model.is_launcher_overdub() { colors::AUTO_MODE_ON } else { colors::AUTO_MODE_OFF });
        cache.update_color(1, 2, if transport.is_writing_clip_launcher_automation() { colors::AUTO_ON } else { colors::AUTO_OFF });
        cache.update_color(1, 3, auto_mode(AutomationMode::Latch));
        cache.update_color(1, 4, auto_mode(AutomationMode::Touch));

        // Row 3 / 4
        cache.update_value(2, 5, 0, Some(&transport.format_tempo(transport.tempo())));
        for (i, (exists, name, color)) in markers.iter().enumerate() {
            let (row, column) = (2 + i / 4, 1 + i % 4);
            cache.update_element(row, column, Some(if *exists { name } else { " " }), Some(Self::marker_color(*color, *exists)), Some(*exists));
        }

        // Row 5
        cache.update_value(4, 1, transport.metronome_volume(), Some(&value_text(transport.metronome_volume_str())));
        cache.update_value(4, 2, cue_value, Some(&value_text(&cue_text)));

        // Row 6
        cache.update_color(5, 1, if transport.is_metronome_on() { colors::METRONOME_ON } else { colors::METRONOME_OFF });
        cache.update_value(5, 2, param_value, Some(&value_text(&param_text)));
        cache.update_color(5, 3, if ctx.model.is_arranger_overdub() { colors::AUTO_MODE_ON } else { colors::AUTO_MODE_OFF });
        cache.update_color(5, 4, if launch_markers { colors::MARKER_LAUNCH_ON } else { colors::MARKER_LAUNCH_OFF });

        self.base.update_master_column(ctx);
        self.base.cache().update_element(4, 3, Some(" "), None, Some(false));
        self.base.flush(ctx);
    }
}
