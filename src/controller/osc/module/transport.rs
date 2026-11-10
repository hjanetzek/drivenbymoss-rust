// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The transport (former `TransportModule`).

use super::{first, is_trigger, sub_command, to_integer, to_number, to_number_or, OscError, OscModule, OscResult, OscShared, Path, Value, TAG_PREROLL, TAG_RESET};
use crate::daw::constants::{AutomationMode, LaunchQuantization, PostRecordingAction};
use crate::framework::command::TriggerCommand;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

const TAG_LAUNCHER: &str = "launcher";

/// Former `TransportModule`.
#[derive(Default)]
pub struct TransportModule {
    play_command: PlayCommand,
}

impl OscModule for TransportModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &[
            "play",
            "playbutton",
            "stop",
            "restart",
            "record",
            "overdub",
            "repeat",
            "punchIn",
            "punchOut",
            "click",
            "quantize",
            "tempo",
            "time",
            "position",
            "crossfade",
            "autowrite",
            "automationWriteMode",
            TAG_PREROLL,
            TAG_LAUNCHER,
        ]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        let is_trigger = is_trigger(value);
        match command {
            "play" => {
                let is_playing = ctx.model.transport().is_playing();
                if is_trigger != is_playing {
                    ctx.model.transport_mut().play();
                }
            }
            "playbutton" => {
                // Java executes the play command with DOWN, which the double trigger command
                // ignores (it acts on UP): this does nothing, as in Java
                if is_trigger {
                    self.play_command.execute(ctx, ButtonEvent::Down, 127);
                }
            }
            "stop" => {
                let transport = ctx.model.transport_mut();
                if transport.is_playing() {
                    transport.stop();
                } else {
                    transport.stop_and_rewind();
                }
            }
            "restart" => {
                if is_trigger {
                    ctx.model.transport_mut().restart();
                }
            }
            "record" => ctx.model.transport_mut().start_recording(),
            "overdub" => {
                if path.front().is_some_and(|p| p == TAG_LAUNCHER) {
                    ctx.model.toggle_launcher_overdub();
                } else {
                    ctx.model.toggle_overdub();
                }
            }
            "repeat" => {
                let transport = ctx.model.transport_mut();
                if value.is_null() {
                    transport.toggle_loop();
                } else {
                    transport.set_loop(is_trigger);
                }
            }
            "punchIn" => {
                let transport = ctx.model.transport_mut();
                if value.is_null() {
                    transport.toggle_punch_in();
                } else {
                    transport.set_punch_in(is_trigger);
                }
            }
            "punchOut" => {
                let transport = ctx.model.transport_mut();
                if value.is_null() {
                    transport.toggle_punch_out();
                } else {
                    transport.set_punch_out(is_trigger);
                }
            }
            "click" => {
                if path.is_empty() {
                    let transport = ctx.model.transport_mut();
                    if value.is_null() {
                        transport.toggle_metronome();
                    } else {
                        transport.set_metronome(is_trigger);
                    }
                    return Ok(());
                }
                let sub = sub_command(path)?;
                match sub.as_str() {
                    "volume" => {
                        let volume = to_integer(value)?;
                        ctx.model.transport_mut().set_metronome_volume(volume);
                    }
                    "ticks" => {
                        let transport = ctx.model.transport_mut();
                        if value.is_null() {
                            transport.toggle_metronome_ticks();
                        } else {
                            transport.set_metronome_ticks(is_trigger);
                        }
                    }
                    TAG_PREROLL => {
                        if is_trigger {
                            ctx.model.transport_mut().toggle_preroll_metronome();
                        }
                    }
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            "quantize" => {
                let clip = ctx.model.note_clip(8, 128);
                if clip.does_exist() {
                    clip.quantize(1.0);
                }
            }
            "tempo" => {
                let sub = sub_command(path)?;
                match sub.as_str() {
                    "raw" => {
                        let tempo = to_number(value)?;
                        ctx.model.transport_mut().set_tempo(tempo);
                    }
                    "tap" => {
                        if is_trigger {
                            ctx.model.transport_mut().tap_tempo();
                        }
                    }
                    "+" => {
                        let tempo = ctx.model.transport().tempo() + to_number_or(value, 1.0)?;
                        ctx.model.transport_mut().set_tempo(tempo);
                    }
                    "-" => {
                        let tempo = ctx.model.transport().tempo() - to_number_or(value, 1.0)?;
                        ctx.model.transport_mut().set_tempo(tempo);
                    }
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            "time" => {
                if path.is_empty() {
                    // Bitwig: the play start position in beats
                    let beats = to_number(value)?;
                    ctx.model.transport_mut().set_position_beats(beats, false);
                    return Ok(());
                }
                let sub = sub_command(path)?;
                if !sub.eq_ignore_ascii_case("signature") {
                    return Err(OscError::UnknownCommand(sub));
                }
                let text = value.text();
                if let Some(text) = &text {
                    // Java String.split drops the trailing empty parts
                    let mut parts: Vec<&str> = text.split('/').collect();
                    while parts.last().is_some_and(|p| p.is_empty()) {
                        parts.pop();
                    }
                    if let [numerator, denominator] = parts[..] {
                        if let (Some(numerator), Some(denominator)) = (super::parse_int(numerator), super::parse_int(denominator)) {
                            ctx.model.transport_mut().set_time_signature(numerator, denominator);
                            return Ok(());
                        }
                    }
                }
                return Err(OscError::IllegalParameter(format!("Illegal parameter: {}", text.unwrap_or_else(|| "null".into()))));
            }
            "position" => {
                if path.is_empty() {
                    let number = to_number(value)?;
                    ctx.model.transport_mut().change_position(number >= 0.0, number.abs() <= 1.0);
                    return Ok(());
                }
                let sub = first(path)?.to_string();
                let transport = ctx.model.transport_mut();
                match sub.as_str() {
                    "+" => transport.change_position(true, true),
                    "-" => transport.change_position(false, true),
                    "++" => transport.change_position(true, false),
                    "--" => transport.change_position(false, false),
                    "start" => transport.set_position_beats(0.0, false),
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            "crossfade" => {
                if path.is_empty() {
                    let crossfade = to_integer(value)?;
                    ctx.model.set_crossfade(crossfade);
                } else if first(path)? == TAG_RESET {
                    if let Some(parameter) = ctx.model.crossfade_parameter_mut() {
                        parameter.reset_value();
                    }
                }
            }
            "autowrite" => {
                let transport = ctx.model.transport_mut();
                if path.front().is_some_and(|p| p == TAG_LAUNCHER) {
                    transport.toggle_write_clip_launcher_automation();
                } else {
                    transport.toggle_write_arranger_automation();
                }
            }
            "automationWriteMode" => {
                if let Some(text) = value.text() {
                    // Java AutomationMode.valueOf throws for an unknown name
                    let upper = text.to_uppercase();
                    let mode = AutomationMode::ALL
                        .iter()
                        .copied()
                        .find(|m| m.name() == upper)
                        .ok_or_else(|| OscError::Runtime(format!("No enum constant AutomationMode.{upper}")))?;
                    ctx.model.transport_mut().set_automation_write_mode(mode);
                }
            }
            TAG_PREROLL => {
                let measures = to_integer(value)?;
                ctx.model.transport_mut().set_preroll_measures(measures);
            }
            TAG_LAUNCHER => {
                let sub = first(path)?.to_string();
                // Java: value.toString () throws for no value
                let text = || value.text().ok_or_else(|| OscError::Runtime("Cannot invoke \"Object.toString()\" because \"value\" is null".into()));
                match sub.as_str() {
                    "postRecordingAction" => {
                        let action = PostRecordingAction::lookup(&text()?);
                        ctx.model.transport_mut().set_clip_launcher_post_recording_action(action);
                    }
                    "postRecordingTimeOffset" => {
                        let beats = to_number(value)?.clamp(0.0, 4000.0);
                        ctx.model.transport_mut().set_clip_launcher_post_recording_time_offset(beats);
                    }
                    "defaultQuantization" => {
                        let quantization = LaunchQuantization::lookup(&text()?);
                        ctx.model.transport_mut().set_default_launch_quantization(quantization);
                    }
                    _ => return Err(OscError::UnknownCommand(sub)),
                }
            }
            _ => return Err(OscError::UnknownCommand(command.to_string())),
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let model = &ctx.model;
        let t = model.transport();
        w.send_bool("/play", t.is_playing(), dump);
        w.send_bool("/record", t.is_recording(), dump);
        w.send_bool("/overdub", model.is_arranger_overdub(), dump);
        w.send_bool("/overdub/launcher", model.is_launcher_overdub(), dump);
        w.send_bool("/repeat", t.is_loop(), dump);
        w.send_bool("/punchIn", t.is_punch_in_enabled(), dump);
        w.send_bool("/punchOut", t.is_punch_out_enabled(), dump);
        w.send_bool("/click", t.is_metronome_on(), dump);
        w.send_bool("/click/ticks", t.is_metronome_ticks_on(), dump);
        w.send_int("/click/volume", t.metronome_volume(), dump);
        w.send_str("/click/volumeStr", t.metronome_volume_str(), dump);
        w.send_bool("/click/preroll", t.is_preroll_metronome_enabled(), dump);
        w.send_int("/preroll", t.preroll_measures(), dump);
        w.send_double("/tempo/raw", t.tempo(), dump);
        w.send_int("/crossfade", model.crossfade(), dump);
        w.send_bool("/autowrite", t.is_writing_arranger_automation(), dump);
        w.send_bool("/autowrite/launcher", t.is_writing_clip_launcher_automation(), dump);
        w.send_str("/automationWriteMode", &t.automation_write_mode().identifier(), dump);
        w.send_str("/time/str", t.position_text(), dump);
        w.send_str("/time/signature", &format!("{} / {}", t.numerator(), t.denominator()), dump);
        w.send_str("/beat/str", t.beat_text(), dump);
        w.send_str("/launcher/postRecordingAction", &t.clip_launcher_post_recording_action().identifier(), dump);
        w.send_double("/launcher/postRecordingTimeOffset", t.clip_launcher_post_recording_time_offset(), dump);
        w.send_str("/launcher/defaultQuantization", t.default_launch_quantization().value(), dump);
    }
}
