// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The virtual keyboard: notes, drum notes, CCs, aftertouch and pitchbend sent to the DAW as if
//! they were played on the MIDI input of the controller, the octaves and the note repeat (former
//! `MidiModule`).

use super::{index_of, is_trigger, parse_int, parse_int_or_throw, sub_command, to_integer, OscError, OscModule, OscResult, OscShared, Path, Value};
use crate::daw::color::ColorEx;
use crate::daw::constants::Resolution;
use crate::framework::ctx::Ctx;
use crate::framework::osc::OscArg;
use crate::framework::view::base::replace_octave_color_with_track_color;

const CMD_NOTE_ON: i32 = 0x90;
const CMD_POLY_AFTERTOUCH: i32 = 0xA0;
const CMD_CC: i32 = 0xB0;
const CMD_CHANNEL_AFTERTOUCH: i32 = 0xD0;
const CMD_PITCHBEND: i32 = 0xE0;

/// The note repeat resolutions in the error message.
const RESOLUTION_ERROR: &str = "Value must be one of {1/4, 1/4t, 1/8, 1/8t, 1/16, 1/16t, 1/32, 1/32t}.";

/// Former `MidiModule`.
#[derive(Default)]
pub struct MidiModule;

impl MidiModule {
    /// Former `updateNoteMatrix`: the key manager and the key translation table of the note input
    /// follow the octave of the scales; the range is shown as a notification.
    pub fn update_note_matrix(ctx: &mut Ctx, shared: &mut OscShared) {
        let matrix = ctx.model.scales().note_matrix();
        shared.key_manager.set_note_matrix(matrix);
        ctx.surface.set_key_translation_table(matrix.to_vec());
        let range = ctx.model.scales().range_text();
        ctx.notify(&range);
    }

    /// Former `adjustDrumDevice`.
    fn adjust_drum_device(ctx: &mut Ctx) {
        let offset = ctx.model.scales().drum_offset();
        if let Some(drum) = ctx.model.drum_device_mut() {
            drum.drum_pad_bank_mut().scroll_to(usize::try_from(offset).unwrap_or(0), false);
        }
        let range = ctx.model.scales().drum_range_text();
        ctx.host.show_notification(&range);
    }

    /// Former `sendNote`.
    fn send_note(ctx: &mut Ctx, shared: &mut OscShared, midi_channel: i32, note: i32, velocity: i32) -> OscResult {
        let velocity = if velocity > 0 && ctx.config.is_accent_active() { ctx.config.fixed_accent_value() } else { velocity };
        let table = ctx.surface.key_translation_table();
        let data0 = usize::try_from(note).ok().and_then(|n| table.get(n).copied()).ok_or_else(|| OscError::Runtime(format!("Index {note} out of bounds for length {}", table.len())))?;
        let mark: Vec<usize> = (0..128.min(table.len())).filter(|&i| table[i] == data0).collect();
        if data0 >= 0 {
            ctx.surface.input.send_raw_midi_event(CMD_NOTE_ON + midi_channel, data0, velocity);
        }
        // Mark selected notes
        for i in mark {
            shared.key_manager.set_key_pressed(i as i32, velocity);
        }
        Ok(())
    }

    /// Former `parseNoteRepeat`.
    fn parse_note_repeat(ctx: &mut Ctx, path: &mut Path, value: Value) -> OscResult {
        let sub = sub_command(path)?;
        let resolution = |value: Value| -> Result<f64, OscError> {
            let text = value.text().ok_or_else(|| OscError::IllegalParameter("Value must not be empty.".into()))?;
            Resolution::by_name(&text).map(Resolution::value).ok_or_else(|| OscError::IllegalParameter(RESOLUTION_ERROR.into()))
        };
        let note_repeat = &mut ctx.surface.input.default_note_input_mut().note_repeat;
        match sub.as_str() {
            "isActive" => note_repeat.set_active(to_integer(value)? > 0),
            "period" => note_repeat.set_period(resolution(value)?),
            "length" => note_repeat.set_note_length(resolution(value)?),
            _ => return Err(OscError::UnknownCommand(sub)),
        }
        Ok(())
    }

    /// Former `getNoteColor`.
    fn note_color(ctx: &Ctx, shared: &OscShared, note: i32) -> ColorEx {
        if !ctx.model.can_selected_track_hold_notes() {
            return ColorEx::BLACK;
        }
        if shared.key_manager.is_key_pressed(note) {
            return if ctx.model.has_recording_state() { ColorEx::RED } else { ColorEx::GREEN };
        }
        let cursor_track = ctx.model.cursor_track();
        let color_id = replace_octave_color_with_track_color(Some(cursor_track), shared.key_manager.color(ctx.model.scales(), note));
        let (red, green, blue) = ctx.colors.color(ctx.colors.color_index(color_id));
        ColorEx::new(red, green, blue)
    }
}

impl OscModule for MidiModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["vkb_midi"]
    }

    fn execute(&mut self, ctx: &mut Ctx, shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        if command != "vkb_midi" {
            return Err(OscError::UnknownCommand(command.to_string()));
        }
        let command = sub_command(path)?;
        match command.as_str() {
            "velocity" => {
                let velocity = to_integer(value)?;
                ctx.config.set_accent_enabled(velocity > 0);
                if velocity > 0 {
                    ctx.config.set_fixed_accent_value(velocity);
                }
                return Ok(());
            }
            "noterepeat" => return Self::parse_note_repeat(ctx, path, value),
            _ => {}
        }

        let Some(channel) = parse_int(&command) else {
            return Err(OscError::UnknownCommand(command));
        };
        let midi_channel = (i64::from(channel) - 1).clamp(0, 15) as i32;
        let sub = sub_command(path)?;
        match sub.as_str() {
            "note" => {
                if path.is_empty() {
                    // Java casts the value to an array of two numbers
                    let Value::Many(values) = value else {
                        return Err(OscError::Runtime("class cast: the value is not an array".into()));
                    };
                    let number = |arg: Option<&OscArg>| arg.and_then(OscArg::as_number).map(|n| n as i32).ok_or_else(|| OscError::Runtime("class cast: not a number".into()));
                    let note = number(values.first())?;
                    let velocity = number(values.get(1))?;
                    return Self::send_note(ctx, shared, midi_channel, note, velocity);
                }
                let n = sub_command(path)?;
                match n.as_str() {
                    "+" => {
                        if is_trigger(value) {
                            ctx.model.scales_mut().inc_octave();
                            Self::update_note_matrix(ctx, shared);
                        }
                    }
                    "-" => {
                        if is_trigger(value) {
                            ctx.model.scales_mut().dec_octave();
                            Self::update_note_matrix(ctx, shared);
                        }
                    }
                    _ => {
                        let note = parse_int_or_throw(&n)?;
                        let velocity = to_integer(value)?;
                        Self::send_note(ctx, shared, midi_channel, note, velocity)?;
                    }
                }
            }
            "drum" => {
                let n = sub_command(path)?;
                match n.as_str() {
                    "+" => {
                        if is_trigger(value) {
                            ctx.model.scales_mut().inc_drum_octave();
                            Self::adjust_drum_device(ctx);
                        }
                    }
                    "-" => {
                        if is_trigger(value) {
                            ctx.model.scales_mut().dec_drum_octave();
                            Self::adjust_drum_device(ctx);
                        }
                    }
                    _ => {
                        let note = parse_int_or_throw(&n)?;
                        let mut velocity = to_integer(value)?;
                        if velocity > 0 && ctx.config.is_accent_active() {
                            velocity = ctx.config.fixed_accent_value();
                        }
                        let matrix = ctx.model.scales().drum_matrix();
                        let data0 = usize::try_from(note).ok().and_then(|n| matrix.get(n).copied()).ok_or_else(|| OscError::Runtime(format!("Index {note} out of bounds for length 128")))?;
                        if data0 >= 0 {
                            ctx.surface.input.send_raw_midi_event(CMD_NOTE_ON + midi_channel, data0, velocity);
                        }
                    }
                }
            }
            "cc" => {
                let Some(cc) = path.pop_front() else {
                    ctx.host.println("Missing MIDI CC value.");
                    return Ok(());
                };
                let cc = parse_int_or_throw(&cc)?;
                let cc_value = to_integer(value)?;
                ctx.surface.input.send_raw_midi_event(CMD_CC + midi_channel, cc, cc_value);
            }
            "aftertouch" => {
                let pressure = to_integer(value)?;
                let Some(note) = path.pop_front() else {
                    // As in Java: the pressure is sent as the second data byte
                    ctx.surface.input.send_raw_midi_event(CMD_CHANNEL_AFTERTOUCH + midi_channel, 0, pressure);
                    return Ok(());
                };
                let note = index_of(parse_int_or_throw(&note)? + 1)?;
                let table = ctx.surface.key_translation_table();
                let data1 = table.get(note).copied().ok_or_else(|| OscError::Runtime(format!("Index {note} out of bounds for length {}", table.len())))?;
                ctx.surface.input.send_raw_midi_event(CMD_POLY_AFTERTOUCH + midi_channel, data1, pressure);
            }
            "pitchbend" => {
                let bend = to_integer(value)?;
                ctx.surface.input.send_raw_midi_event(CMD_PITCHBEND + midi_channel, 0, bend);
            }
            _ => return Err(OscError::UnknownCommand(command)),
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        for i in 0..127 {
            let color = Self::note_color(ctx, shared, i);
            let [red, green, blue] = color.to_double_rgb();
            shared.writer.send_color(&format!("/vkb_midi/note/{i}/color"), red, green, blue, dump);
        }

        // Flush note repeat
        let note_repeat = &ctx.surface.input.default_note_input().note_repeat;
        shared.writer.send_bool("/vkb_midi/noterepeat/isActive", note_repeat.is_active, dump);
        shared.writer.send_str("/vkb_midi/noterepeat/period", Resolution::name_at(Resolution::get_match(note_repeat.period)), dump);
        shared.writer.send_str("/vkb_midi/noterepeat/length", Resolution::name_at(Resolution::get_match(note_repeat.note_length)), dump);
    }
}
