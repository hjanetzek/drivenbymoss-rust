// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The shift view of the Launchpad (former `ShiftView`): additional functions which are
//! available while Shift is pressed.

use crate::config::values::NEW_CLIP_LENGTH_VALUES;
use crate::controller::launchpad::colors::*;
use crate::controller::launchpad::surface as lp;
use crate::daw::constants::{ChannelType, Resolution};
use crate::framework::command::TriggerCommand;
use crate::framework::commands::mv_helper;
use crate::framework::commands::transport::ConfiguredRecordCommand;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{ScrollStates, View};
use crate::framework::ids::ButtonId;
use crate::framework::view::shift_view::ShiftViewBase;
use crate::impl_as_any;
use crate::midi::ButtonEvent;

const TAG_ACTIVE: &str = "Active";

/// The control mode buttons which are triggered with the scene buttons.
fn control_mode(button: ButtonId) -> Option<ButtonId> {
    Some(match button {
        ButtonId::Scene1 => ButtonId::Volume,
        ButtonId::Scene2 => ButtonId::PanSend,
        ButtonId::Scene3 => ButtonId::Sends,
        ButtonId::Scene4 => ButtonId::Track,
        ButtonId::Scene5 => ButtonId::StopClip,
        ButtonId::Scene6 => ButtonId::Mute,
        ButtonId::Scene7 => ButtonId::Solo,
        ButtonId::Scene8 => ButtonId::RecArm,
        _ => return None,
    })
}

/// The colors of the scene buttons.
fn mode_color(button: ButtonId) -> i32 {
    match button {
        ButtonId::Scene1 => LAUNCHPAD_COLOR_CYAN,
        ButtonId::Scene2 => LAUNCHPAD_COLOR_SKY,
        ButtonId::Scene3 => LAUNCHPAD_COLOR_ORCHID,
        ButtonId::Scene4 => LAUNCHPAD_COLOR_GREEN,
        ButtonId::Scene5 => LAUNCHPAD_COLOR_ROSE,
        ButtonId::Scene6 => LAUNCHPAD_COLOR_YELLOW,
        ButtonId::Scene7 => LAUNCHPAD_COLOR_BLUE,
        ButtonId::Scene8 => LAUNCHPAD_COLOR_RED,
        // Java throws a NullPointerException for other buttons
        _ => LAUNCHPAD_COLOR_BLACK,
    }
}

/// The shift view.
pub struct ShiftView {
    shift: ShiftViewBase,
    configured_record_command: ConfiguredRecordCommand,
    configured_shifted_record_command: ConfiguredRecordCommand,
}

impl ShiftView {
    pub fn new(_ctx: &Ctx) -> Self {
        ShiftView {
            shift: ShiftViewBase::new("Shift"),
            configured_record_command: ConfiguredRecordCommand::with_shift(false),
            configured_shifted_record_command: ConfiguredRecordCommand::with_shift(true),
        }
    }

    fn handle_transport(&mut self, ctx: &mut Ctx, note: i32) {
        match note {
            92 => {
                ctx.simulate_normal_button_press(ButtonId::Metronome);
                mv_helper::delay_display(ctx, |ctx| format!("Metronome: {}", if ctx.model.transport().is_metronome_on() { "On" } else { "Off" }));
            }
            93 => {
                ctx.simulate_shifted_button_press(ButtonId::Metronome);
                ctx.notify("Tap Tempo");
            }
            84 => {
                ctx.simulate_normal_button_press(ButtonId::Undo);
                ctx.notify("Undo");
            }
            85 => {
                ctx.simulate_shifted_button_press(ButtonId::Undo);
                ctx.notify("Redo");
            }
            76 => {
                ctx.config.toggle_delete_mode_active();
                let message = format!("Delete {}", if ctx.config.is_delete_mode_active() { TAG_ACTIVE } else { "Off" });
                ctx.notify(&message);
            }
            77 => {
                ctx.simulate_shifted_button_press(ButtonId::Delete);
                mv_helper::delay_display(ctx, |ctx| format!("Arrangement Loop: {}", if ctx.model.transport().is_loop() { "On" } else { "Off" }));
            }
            69 => ctx.simulate_shifted_button_press(ButtonId::Quantize),
            68 => {
                ctx.simulate_normal_button_press(ButtonId::Quantize);
                ctx.notify("Quantize");
            }
            60 => {
                ctx.config.toggle_duplicate_mode_active();
                let message = format!("Duplicate {}", if ctx.config.is_duplicate_mode_active() { TAG_ACTIVE } else { "Off" });
                ctx.notify(&message);
            }
            61 => {
                ctx.simulate_shifted_button_press(ButtonId::Duplicate);
                ctx.notify("Double");
            }
            52 => {
                ctx.simulate_normal_button_press(ButtonId::Play);
                ctx.notify("Play");
            }
            53 => {
                ctx.simulate_shifted_button_press(ButtonId::Play);
                ctx.notify("New");
            }
            44 => {
                self.configured_record_command.execute(ctx, ButtonEvent::Down, 127);
                self.configured_record_command.execute(ctx, ButtonEvent::Up, 0);
            }
            45 => {
                self.configured_shifted_record_command.execute(ctx, ButtonEvent::Down, 127);
                self.configured_shifted_record_command.execute(ctx, ButtonEvent::Up, 0);
            }
            // Not used
            _ => {}
        }
    }

    fn set_period(ctx: &mut Ctx, index: usize) {
        ctx.config.set_note_repeat_period(Resolution::ALL[index]);
        ctx.schedule(100, move |ctx| ctx.notify(&format!("Period: {}", Resolution::name_at(index))));
    }

    fn set_note_length(ctx: &mut Ctx, index: usize) {
        ctx.config.set_note_repeat_length(Resolution::ALL[index]);
        ctx.schedule(100, move |ctx| ctx.notify(&format!("Note Length: {}", Resolution::name_at(index))));
    }
}

/// The resolution index of the note repeat period pads.
fn period_index(note: i32) -> Option<usize> {
    [79, 80, 71, 72, 63, 64, 55, 56].iter().position(|&n| n == note)
}

/// The resolution index of the note repeat length pads.
fn length_index(note: i32) -> Option<usize> {
    [81, 82, 73, 74, 65, 66, 57, 58].iter().position(|&n| n == note)
}

impl View for ShiftView {
    fn name(&self) -> &str {
        &self.shift.base.name
    }

    fn on_activate(&mut self, ctx: &mut Ctx) {
        self.shift.base.clear_note_mapping(ctx);
        self.shift.on_activate();
    }

    fn draw_grid(&mut self, ctx: &mut Ctx) {
        let is_pro = lp::is_pro(ctx);
        let accent_active = ctx.config.is_accent_active();
        let clip_length_index = ctx.config.new_clip_length() as i32;
        let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
        let note_repeat_active = note_repeat.is_active;
        let period_index = Resolution::get_match(note_repeat.period);
        let length_index = Resolution::get_match(note_repeat.note_length);
        let is_record_lit = self.configured_record_command.is_lit_now(ctx);
        let is_shifted_record_lit = self.configured_shifted_record_command.is_lit_now(ctx);
        let transport = ctx.model.transport();
        let (is_playing, is_loop, is_metronome_on) = (transport.is_playing(), transport.is_loop(), transport.is_metronome_on());
        let is_duplicate_mode_active = ctx.config.is_duplicate_mode_active();
        let is_delete_mode_active = ctx.config.is_delete_mode_active();

        let grid = ctx.surface.grid();
        let hi_lo = |on: bool, hi: i32, lo: i32| if on { hi } else { lo };

        // Add tracks
        grid.light(97, LAUNCHPAD_COLOR_GREEN);
        grid.light(98, LAUNCHPAD_COLOR_GREEN_SPRING);
        grid.light(99, LAUNCHPAD_COLOR_TURQUOISE_CYAN);

        // Accent on/off
        grid.light(91, hi_lo(accent_active, LAUNCHPAD_COLOR_YELLOW_HI, LAUNCHPAD_COLOR_YELLOW_LO));

        // New clip length
        for i in 0..8 {
            grid.light(36 + i, hi_lo(i == clip_length_index, LAUNCHPAD_COLOR_WHITE, LAUNCHPAD_COLOR_GREY_LO));
        }

        // Note Repeat
        grid.light(87, hi_lo(note_repeat_active, LAUNCHPAD_COLOR_ORCHID_HI, LAUNCHPAD_COLOR_ORCHID_LO));

        // Note Repeat period and length
        for (i, (period_note, length_note)) in [(79, 81), (80, 82), (71, 73), (72, 74), (63, 65), (64, 66), (55, 57), (56, 58)].into_iter().enumerate() {
            let (hi, lo) = if i % 2 == 0 { (LAUNCHPAD_COLOR_SKY_HI, LAUNCHPAD_COLOR_SKY_LO) } else { (LAUNCHPAD_COLOR_PINK_HI, LAUNCHPAD_COLOR_PINK_LO) };
            grid.light(period_note, hi_lo(period_index == i, hi, lo));
            grid.light(length_note, hi_lo(length_index == i, hi, lo));
        }

        // Stop all
        grid.light(50, LAUNCHPAD_COLOR_RED_AMBER);
        grid.light(51, LAUNCHPAD_COLOR_RED);

        // Note Repeat Octave Range up/down
        grid.light(88, LAUNCHPAD_COLOR_RED);
        grid.light(89, LAUNCHPAD_COLOR_RED);
        grid.light(90, LAUNCHPAD_COLOR_BLACK);

        if is_pro {
            for range in [44..50, 52..55, 59..63, 67..71, 75..79, 83..87, 92..97] {
                for i in range {
                    grid.light(i, LAUNCHPAD_COLOR_BLACK);
                }
            }
            return;
        }

        // Record
        grid.light(44, hi_lo(is_record_lit, LAUNCHPAD_COLOR_RED_HI, LAUNCHPAD_COLOR_RED_LO));
        grid.light(45, hi_lo(is_shifted_record_lit, LAUNCHPAD_COLOR_ROSE, LAUNCHPAD_COLOR_WHITE));
        for i in 46..50 {
            grid.light(i, LAUNCHPAD_COLOR_BLACK);
        }

        // Play / New
        grid.light(52, hi_lo(is_playing, LAUNCHPAD_COLOR_GREEN_HI, LAUNCHPAD_COLOR_GREEN_LO));
        grid.light(53, LAUNCHPAD_COLOR_GREEN_SPRING);
        grid.light(54, LAUNCHPAD_COLOR_BLACK);
        grid.light(59, LAUNCHPAD_COLOR_BLACK);

        // Duplicate
        if is_duplicate_mode_active {
            grid.light_blink(60, LAUNCHPAD_COLOR_BLUE, LAUNCHPAD_COLOR_OCEAN_BLUE, true);
        } else {
            grid.light(60, LAUNCHPAD_COLOR_BLUE);
        }

        // Double
        grid.light(61, LAUNCHPAD_COLOR_BLUE_ORCHID);
        grid.light(62, LAUNCHPAD_COLOR_BLACK);
        grid.light(67, LAUNCHPAD_COLOR_BLACK);

        // Quantize
        grid.light(68, LAUNCHPAD_COLOR_GREEN);
        // Record Quantization
        grid.light(69, LAUNCHPAD_COLOR_GREEN_SPRING);
        grid.light(70, LAUNCHPAD_COLOR_BLACK);
        grid.light(75, LAUNCHPAD_COLOR_BLACK);

        // Delete
        if is_delete_mode_active {
            grid.light_blink(76, LAUNCHPAD_COLOR_MAGENTA, LAUNCHPAD_COLOR_MAGENTA_PINK, true);
        } else {
            grid.light(76, LAUNCHPAD_COLOR_MAGENTA);
        }

        grid.light(77, hi_lo(is_loop, LAUNCHPAD_COLOR_BLUE_HI, LAUNCHPAD_COLOR_BLUE_LO));
        grid.light(78, LAUNCHPAD_COLOR_BLACK);
        grid.light(83, LAUNCHPAD_COLOR_BLACK);

        // Undo / Redo
        grid.light(84, LAUNCHPAD_COLOR_AMBER);
        grid.light(85, LAUNCHPAD_COLOR_AMBER_YELLOW);
        grid.light(86, LAUNCHPAD_COLOR_BLACK);

        // Metronome
        grid.light(92, hi_lo(is_metronome_on, LAUNCHPAD_COLOR_GREEN_HI, LAUNCHPAD_COLOR_GREEN_LO));
        // Tap Tempo
        grid.light(93, LAUNCHPAD_COLOR_GREEN_SPRING);
        for i in 94..97 {
            grid.light(i, LAUNCHPAD_COLOR_BLACK);
        }
    }

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        if velocity == 0 {
            return;
        }

        self.shift.set_was_used();

        if let Some(index) = period_index(note) {
            Self::set_period(ctx, index);
            return;
        }
        if let Some(index) = length_index(note) {
            Self::set_note_length(ctx, index);
            return;
        }

        match note {
            36..=43 => {
                let new_clip_length = (note - 36) as usize;
                ctx.config.set_new_clip_length(new_clip_length);
                ctx.notify(&format!("New clip length: {}", NEW_CLIP_LENGTH_VALUES[new_clip_length]));
            }
            50 | 51 => {
                ctx.model.current_track_bank_mut().stop(note == 50);
                ctx.notify(if note == 50 { "Stop All Clips (Alt)" } else { "Stop All Clips" });
            }
            87 => {
                ctx.config.toggle_note_repeat_active();
                mv_helper::delay_display(ctx, |ctx| format!("Note Repeat: {}", if ctx.config.is_note_repeat_active() { TAG_ACTIVE } else { "Off" }));
            }
            88 => {
                let octave = (ctx.config.note_repeat_octave() - 1).max(0);
                ctx.config.set_note_repeat_octave(octave);
                ctx.notify(&format!("Note Repeat Octave Range: {octave}"));
            }
            89 => {
                let octave = (ctx.config.note_repeat_octave() + 1).min(8);
                ctx.config.set_note_repeat_octave(octave);
                ctx.notify(&format!("Note Repeat Octave Range: {octave}"));
            }
            91 => {
                let enabled = !ctx.config.is_accent_active();
                ctx.config.set_accent_enabled(enabled);
                ctx.notify(&format!("Fixed Accent: {}", if enabled { "On" } else { "Off" }));
            }
            97 => ctx.model.track_bank_mut().add_channel(ChannelType::Instrument, None, None, &[]),
            98 => ctx.model.track_bank_mut().add_channel(ChannelType::Audio, None, None, &[]),
            99 => ctx.model.application_mut().add_effect_track(),
            _ => {
                if !lp::is_pro(ctx) {
                    self.handle_transport(ctx, note);
                }
            }
        }
    }

    fn button_color(&self, ctx: &Ctx, button: ButtonId) -> i32 {
        if lp::is_pro(ctx) {
            LAUNCHPAD_COLOR_BLACK
        } else {
            mode_color(button)
        }
    }

    fn on_button(&mut self, ctx: &mut Ctx, button: ButtonId, event: ButtonEvent, _velocity: i32) {
        if lp::is_pro(ctx) || event != ButtonEvent::Down {
            return;
        }
        if let Some(mode_button) = control_mode(button) {
            // Former handleControlModes
            if let Some(command) = ctx.button_command(mode_button) {
                command.borrow_mut().execute(ctx, ButtonEvent::Down, 127);
            }
        }
    }

    fn scroll_states(&self, _ctx: &Ctx) -> Option<ScrollStates> {
        let mut states = ScrollStates::default();
        states.set_all(true);
        Some(states)
    }

    fn was_shift_view_used(&self) -> bool {
        self.shift.was_used()
    }

    impl_as_any!();
}
