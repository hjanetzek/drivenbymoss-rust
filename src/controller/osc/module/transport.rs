// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The transport (former `TransportModule`).

use super::super::address::{Dir, FaderOp, PositionMove, TransportIn};
use super::{is_trigger, text_or_throw, to_integer, to_number, to_number_or, toggle_or_set, OscError, OscResult, OscShared, Value};
use crate::daw::constants::{AutomationMode, LaunchQuantization, PostRecordingAction};
use crate::framework::command::TriggerCommand;
use crate::framework::commands::transport::PlayCommand;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::{Key, OscWriter};
use crate::midi::ButtonEvent;

/// The keys of the values of the transport.
struct Keys {
    play: Key<bool>,
    record: Key<bool>,
    overdub: Key<bool>,
    overdub_launcher: Key<bool>,
    repeat: Key<bool>,
    punch_in: Key<bool>,
    punch_out: Key<bool>,
    click: Key<bool>,
    click_ticks: Key<bool>,
    click_volume: Key<i32>,
    click_volume_str: Key<str>,
    click_preroll: Key<bool>,
    preroll: Key<i32>,
    tempo: Key<f64>,
    crossfade: Key<i32>,
    autowrite: Key<bool>,
    autowrite_launcher: Key<bool>,
    automation_write_mode: Key<str>,
    time: Key<str>,
    time_signature: Key<str>,
    beat: Key<str>,
    post_recording_action: Key<str>,
    post_recording_time_offset: Key<f64>,
    default_quantization: Key<str>,
}

/// Former `TransportModule`.
pub struct TransportModule {
    play_command: PlayCommand,
    keys: Keys,
}

impl TransportModule {
    pub fn new(w: &mut OscWriter) -> Self {
        let keys = Keys {
            play: w.key("/play"),
            record: w.key("/record"),
            overdub: w.key("/overdub"),
            overdub_launcher: w.key("/overdub/launcher"),
            repeat: w.key("/repeat"),
            punch_in: w.key("/punchIn"),
            punch_out: w.key("/punchOut"),
            click: w.key("/click"),
            click_ticks: w.key("/click/ticks"),
            click_volume: w.key("/click/volume"),
            click_volume_str: w.key("/click/volumeStr"),
            click_preroll: w.key("/click/preroll"),
            preroll: w.key("/preroll"),
            tempo: w.key("/tempo/raw"),
            crossfade: w.key("/crossfade"),
            autowrite: w.key("/autowrite"),
            autowrite_launcher: w.key("/autowrite/launcher"),
            automation_write_mode: w.key("/automationWriteMode"),
            time: w.key("/time/str"),
            time_signature: w.key("/time/signature"),
            beat: w.key("/beat/str"),
            post_recording_action: w.key("/launcher/postRecordingAction"),
            post_recording_time_offset: w.key("/launcher/postRecordingTimeOffset"),
            default_quantization: w.key("/launcher/defaultQuantization"),
        };
        TransportModule { play_command: PlayCommand::default(), keys }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: TransportIn, value: Value) -> OscResult {
        let is_trigger = is_trigger(value);
        match command {
            TransportIn::Play {} => {
                let is_playing = ctx.model.transport().is_playing();
                if is_trigger != is_playing {
                    ctx.model.transport_mut().play();
                }
            }
            TransportIn::PlayButton {} => {
                // Java executes the play command with DOWN, which the double trigger command
                // ignores (it acts on UP): this does nothing, as in Java
                if is_trigger {
                    self.play_command.execute(ctx, ButtonEvent::Down, 127);
                }
            }
            TransportIn::Stop {} => {
                let transport = ctx.model.transport_mut();
                if transport.is_playing() {
                    transport.stop();
                } else {
                    transport.stop_and_rewind();
                }
            }
            TransportIn::Restart {} => {
                if is_trigger {
                    ctx.model.transport_mut().restart();
                }
            }
            TransportIn::Record {} => ctx.model.transport_mut().start_recording(),
            TransportIn::Overdub { launcher } => {
                if launcher {
                    ctx.model.toggle_launcher_overdub();
                } else {
                    ctx.model.toggle_overdub();
                }
            }
            TransportIn::Repeat {} => toggle_or_set(ctx.model.transport_mut(), value, |t| t.toggle_loop(), |t, on| t.set_loop(on)),
            TransportIn::PunchIn {} => toggle_or_set(ctx.model.transport_mut(), value, |t| t.toggle_punch_in(), |t, on| t.set_punch_in(on)),
            TransportIn::PunchOut {} => toggle_or_set(ctx.model.transport_mut(), value, |t| t.toggle_punch_out(), |t, on| t.set_punch_out(on)),
            TransportIn::Click {} => toggle_or_set(ctx.model.transport_mut(), value, |t| t.toggle_metronome(), |t, on| t.set_metronome(on)),
            TransportIn::ClickVolume {} => {
                let volume = to_integer(value)?;
                ctx.model.transport_mut().set_metronome_volume(volume);
            }
            TransportIn::ClickTicks {} => toggle_or_set(ctx.model.transport_mut(), value, |t| t.toggle_metronome_ticks(), |t, on| t.set_metronome_ticks(on)),
            TransportIn::ClickPreroll {} => {
                if is_trigger {
                    ctx.model.transport_mut().toggle_preroll_metronome();
                }
            }
            TransportIn::Quantize {} => {
                let clip = ctx.model.note_clip(8, 128);
                if clip.does_exist() {
                    clip.quantize(1.0);
                }
            }
            TransportIn::TempoRaw {} => {
                let tempo = to_number(value)?;
                ctx.model.transport_mut().set_tempo(tempo);
            }
            TransportIn::TempoTap {} => {
                if is_trigger {
                    ctx.model.transport_mut().tap_tempo();
                }
            }
            TransportIn::TempoChange { dir } => {
                let change = to_number_or(value, 1.0)?;
                let tempo = ctx.model.transport().tempo();
                let tempo = match dir {
                    Dir::Next => tempo + change,
                    Dir::Previous => tempo - change,
                };
                ctx.model.transport_mut().set_tempo(tempo);
            }
            TransportIn::Time {} => {
                // Bitwig: the play start position in beats
                let beats = to_number(value)?;
                ctx.model.transport_mut().set_position_beats(beats, false);
            }
            TransportIn::TimeSignature {} => {
                let text = value.text();
                if let Some(text) = &text {
                    // Java String.split drops the trailing empty parts
                    let mut parts: Vec<&str> = text.split('/').collect();
                    while parts.last().is_some_and(|p| p.is_empty()) {
                        parts.pop();
                    }
                    if let [numerator, denominator] = parts[..] {
                        if let (Some(numerator), Some(denominator)) = (super::super::address::parse_int(numerator), super::super::address::parse_int(denominator)) {
                            ctx.model.transport_mut().set_time_signature(numerator, denominator);
                            return Ok(());
                        }
                    }
                }
                return Err(OscError::IllegalParameter(format!("Illegal parameter: {}", text.unwrap_or_else(|| "null".into()))));
            }
            TransportIn::Position {} => {
                let number = to_number(value)?;
                ctx.model.transport_mut().change_position(number >= 0.0, number.abs() <= 1.0);
            }
            TransportIn::PositionMove { step } => {
                let transport = ctx.model.transport_mut();
                match step {
                    PositionMove::Forward => transport.change_position(true, true),
                    PositionMove::Back => transport.change_position(false, true),
                    PositionMove::FastForward => transport.change_position(true, false),
                    PositionMove::FastBack => transport.change_position(false, false),
                    PositionMove::Start => transport.set_position_beats(0.0, false),
                }
            }
            TransportIn::Crossfade { op } => match op {
                FaderOp::Set => {
                    let crossfade = to_integer(value)?;
                    ctx.model.set_crossfade(crossfade);
                }
                FaderOp::Reset => {
                    if let Some(parameter) = ctx.model.crossfade_parameter_mut() {
                        parameter.reset_value();
                    }
                }
                FaderOp::Indicate | FaderOp::Touched | FaderOp::Ignore => {}
            },
            TransportIn::Autowrite { launcher } => {
                let transport = ctx.model.transport_mut();
                if launcher {
                    transport.toggle_write_clip_launcher_automation();
                } else {
                    transport.toggle_write_arranger_automation();
                }
            }
            TransportIn::AutomationWriteMode {} => {
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
            TransportIn::Preroll {} => {
                let measures = to_integer(value)?;
                ctx.model.transport_mut().set_preroll_measures(measures);
            }
            TransportIn::PostRecordingAction {} => {
                let action = PostRecordingAction::lookup(&text_or_throw(value)?);
                ctx.model.transport_mut().set_clip_launcher_post_recording_action(action);
            }
            TransportIn::PostRecordingTimeOffset {} => {
                let beats = to_number(value)?.clamp(0.0, 4000.0);
                ctx.model.transport_mut().set_clip_launcher_post_recording_time_offset(beats);
            }
            TransportIn::DefaultQuantization {} => {
                let quantization = LaunchQuantization::lookup(&text_or_throw(value)?);
                ctx.model.transport_mut().set_default_launch_quantization(quantization);
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let k = &self.keys;
        let model = &ctx.model;
        let t = model.transport();
        w.put(k.play, t.is_playing(), dump);
        w.put(k.record, t.is_recording(), dump);
        w.put(k.overdub, model.is_arranger_overdub(), dump);
        w.put(k.overdub_launcher, model.is_launcher_overdub(), dump);
        w.put(k.repeat, t.is_loop(), dump);
        w.put(k.punch_in, t.is_punch_in_enabled(), dump);
        w.put(k.punch_out, t.is_punch_out_enabled(), dump);
        w.put(k.click, t.is_metronome_on(), dump);
        w.put(k.click_ticks, t.is_metronome_ticks_on(), dump);
        w.put(k.click_volume, t.metronome_volume(), dump);
        w.put(k.click_volume_str, t.metronome_volume_str(), dump);
        w.put(k.click_preroll, t.is_preroll_metronome_enabled(), dump);
        w.put(k.preroll, t.preroll_measures(), dump);
        w.put(k.tempo, t.tempo(), dump);
        w.put(k.crossfade, model.crossfade(), dump);
        w.put(k.autowrite, t.is_writing_arranger_automation(), dump);
        w.put(k.autowrite_launcher, t.is_writing_clip_launcher_automation(), dump);
        w.put(k.automation_write_mode, &t.automation_write_mode().identifier(), dump);
        w.put(k.time, t.position_text(), dump);
        w.put(k.time_signature, &format!("{} / {}", t.numerator(), t.denominator()), dump);
        w.put(k.beat, t.beat_text(), dump);
        w.put(k.post_recording_action, &t.clip_launcher_post_recording_action().identifier(), dump);
        w.put(k.post_recording_time_offset, t.clip_launcher_post_recording_time_offset(), dump);
        w.put(k.default_quantization, t.default_launch_quantization().value(), dump);
    }
}
