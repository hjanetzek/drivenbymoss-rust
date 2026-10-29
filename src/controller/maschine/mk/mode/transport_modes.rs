// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The transport modes (former `TempoMode`, `PositionMode`, `LoopStartMode` and
//! `LoopLengthMode`).

use super::{with_display, BaseMode};
use crate::framework::ctx::Ctx;
use crate::framework::feature::Mode;
use crate::impl_as_any;

/// Which transport value is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportModeKind {
    Tempo,
    Position,
    LoopStart,
    LoopLength,
}

/// Edit the tempo, the play position, the loop start or the loop length with the knobs.
pub struct TransportMode {
    base: BaseMode,
    kind: TransportModeKind,
}

impl TransportMode {
    pub fn new(_ctx: &Ctx, kind: TransportModeKind) -> Self {
        TransportMode { base: BaseMode::new(0), kind }
    }
}

impl Mode for TransportMode {
    fn name(&self) -> &str {
        match self.kind {
            TransportModeKind::Tempo => "Tempo",
            TransportModeKind::Position => "Position",
            TransportModeKind::LoopStart => "Loop Start",
            TransportModeKind::LoopLength => "Loop Length",
        }
    }

    fn on_knob_value(&mut self, ctx: &mut Ctx, _index: usize, value: i32) {
        let slow = ctx.surface.is_knob_sensitivity_slow();
        let value_changer = ctx.model.value_changer();
        let is_increase = value_changer.is_increase(value);
        let speed = value_changer.calc_knob_change(value);
        let transport = ctx.model.transport_mut();
        match self.kind {
            // Former TempoCommand.execute
            TransportModeKind::Tempo => transport.change_tempo(is_increase, slow),
            TransportModeKind::Position => transport.change_position(speed > 0.0, slow),
            TransportModeKind::LoopStart => transport.change_loop_start(speed > 0.0, slow),
            TransportModeKind::LoopLength => transport.change_loop_length(speed > 0.0, slow),
        }
    }

    fn update_display(&mut self, ctx: &mut Ctx) {
        let transport = ctx.model.transport();
        let tempo = transport.tempo();
        let position = transport.position_text().to_string();
        let beats = transport.beat_text().to_string();
        let loop_start = transport.loop_start_beat_text().to_string();
        let loop_length = transport.loop_length_beat_text().to_string();
        let kind = self.kind;
        with_display(ctx, |d| {
            d.clear();
            match kind {
                TransportModeKind::Tempo | TransportModeKind::Position => {
                    let is_tempo = kind == TransportModeKind::Tempo;
                    let (tempo_mark, position_mark) = if is_tempo { ("> ", "  ") } else { ("  ", "> ") };
                    d.set_cell(0, 0, "Tempo:").set_block(0, 1, &format!("{tempo_mark}{tempo:.2}"));
                    d.set_block(0, 2, "Time:").set_block(0, 3, &format!("{position_mark}{position}"));
                    d.set_block(1, 2, "Position:").set_block(1, 3, &format!("{position_mark}{beats}"));
                }
                TransportModeKind::LoopStart | TransportModeKind::LoopLength => {
                    let is_start = kind == TransportModeKind::LoopStart;
                    let (start_mark, length_mark) = if is_start { ("> ", "  ") } else { ("  ", "> ") };
                    d.set_block(0, 0, "Arranger Loop");
                    d.set_block(0, 2, "Start:").set_block(0, 3, &format!("{start_mark}{loop_start}"));
                    d.set_block(1, 2, "Length:").set_block(1, 3, &format!("{length_mark}{loop_length}"));
                }
            }
            d.all_done();
        });
    }

    fn select_previous_item(&mut self, _ctx: &mut Ctx) {
        self.base.select_previous_item();
    }

    fn select_next_item(&mut self, _ctx: &mut Ctx) {
        self.base.select_next_item();
    }

    fn select_item(&mut self, _ctx: &mut Ctx, index: usize) {
        self.base.select_item(index);
    }

    impl_as_any!();
}
