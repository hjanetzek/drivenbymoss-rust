// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Parser for the messages sent from the host (former `MessageParser`).
//!
//! The host sends text messages of the form `<address> <value>`, e.g. `/track/3/volume 0.7`.
//! The address table of the typed protocol turns a line into a [`Msg`] ([`Msg::from_text`],
//! `src/protocol/msg.rs`), which [`Parser::apply`] applies to the [`Model`]. Everything which the
//! Java parser did with the controller setup (re-binding knobs, updating the note mapping,
//! configuration values, note repeat settings of the surface) is returned as [`ParserEvent`]s
//! which the app layer handles.

use crate::daw::channel::Channel;
use crate::daw::color::ColorEx;
use crate::daw::constants::{AutomationMode, ChannelType, DeviceId, GrooveParameterId};
use crate::daw::device::SpecificDevice;
use crate::daw::model::Model;
use crate::daw::parameter::{Parameter, ParameterKind};
use crate::daw::parameter_bank::ParameterBank;
use crate::daw::track_bank::{TrackBank, TrackRef};
use crate::protocol::msg::{BrowserItem, Ch, Dev, HostParam, Msg, PadRef, PageOwner, ParamOwner};
use crate::protocol::{CueParam, Miss};

/// Actions which the app layer has to execute after parsing a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ParserEvent {
    /// The size of a bank (tracks, sends, parameters) changed: the active mode of all surfaces
    /// should be re-activated (`mode.on_activate()`) to rebind the knobs.
    RebindKnobs,
    /// The note mapping of the active view should be updated. Java executed this with a delay
    /// of [`UPDATE_NOTE_MAPPING_DELAY_MS`] milliseconds.
    UpdateNoteMapping,
    /// Set the quantize amount (`configuration.setQuantizeAmount`).
    QuantizeAmount(i32),
    /// Set the note repeat active state (`configuration.setNoteRepeatActive`).
    NoteRepeatActive(bool),
    /// Set the note repeat period (already converted: 1 / received value).
    NoteRepeatPeriod(f64),
    /// Set the note repeat note length.
    NoteRepeatNoteLength(f64),
    /// Set the note repeat mode (index of the arpeggiator mode).
    NoteRepeatMode(i32),
    /// Set if the note repeat uses the pressure for velocity.
    NoteRepeatUsePressure(bool),
    /// An action was selected in Reaper (`/action/select`, for the configuration dialog).
    ActionSelected(String),
}

/// The delay of the note mapping update in Java.
pub const UPDATE_NOTE_MAPPING_DELAY_MS: u64 = 1000;

/// Parses the messages from the host and updates the model.
pub struct Parser {
    events: Vec<ParserEvent>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// A gray color for the reports without a (valid) color.
fn gray() -> [f64; 3] {
    ColorEx::GRAY.to_double_rgb()
}

/// The device of the common device reports: the cursor device, the first instrument or the EQ
/// (if the model setup created them).
fn device_mut(model: &mut Model, dev: Dev) -> Option<&mut SpecificDevice> {
    match dev {
        Dev::Device => Some(model.cursor_device_mut()),
        Dev::Primary => model.specific_device_mut(DeviceId::FirstInstrument),
        Dev::Eq => model.specific_device_mut(DeviceId::Eq),
    }
}

/// The track bank and the track of a channel report.
fn channel_mut(model: &mut Model, ch: Ch) -> (&mut TrackBank, TrackRef) {
    match ch {
        Ch::Track(p) => (model.track_bank_mut(), TrackRef::Position(p)),
        Ch::Effect(p) => (model.reported_effect_track_bank_mut(), TrackRef::Position(p)),
        Ch::Master => (model.track_bank_mut(), TrackRef::Master),
    }
}

/// Apply `f` to the channel of a drum pad (of every drum machine for `drum/pad`) or a layer.
fn for_pad(model: &mut Model, pad: PadRef, mut f: impl FnMut(&mut Channel)) {
    match pad {
        PadRef::DrumPad(i) => model.for_each_drum_device(|device| {
            let bank = device.drum_pad_bank_mut();
            bank.set_host_reported();
            f(bank.unpaged_item_mut(i));
        }),
        PadRef::Pad(dev, i) => {
            if let Some(device) = device_mut(model, dev) {
                let bank = device.drum_pad_bank_mut();
                bank.set_host_reported();
                f(bank.unpaged_item_mut(i));
            }
        }
        PadRef::Layer(dev, i) => {
            if let Some(device) = device_mut(model, dev) {
                f(device.layer_bank_mut().unpaged_item_mut(i));
            }
        }
    }
}

/// The parameter bank of a parameter report, if it exists.
fn param_bank_mut(model: &mut Model, owner: ParamOwner) -> Option<&mut ParameterBank> {
    match owner {
        ParamOwner::Device(dev) => device_mut(model, dev)?.parameter_bank_mut(),
        ParamOwner::Project => model.project_mut().host_parameter_bank_mut(),
        // A host which pins its cursor track (Bitwig) reports the remote controls of that track
        // (CursorTrackImpl.getParameterBank of the Java version), Reaper those of the selected
        // track
        ParamOwner::TrackFx => {
            if model.host_pins_track() {
                model.cursor_track_parameter_bank_mut()
            } else {
                model.track_bank_mut().selected_item_mut()?.parameter_bank_mut()
            }
        }
        ParamOwner::MasterFx => model.project_mut().parameter_bank_mut(),
    }
}

/// The parameter bank with the named pages of a page report, if it exists.
fn page_bank_mut(model: &mut Model, owner: PageOwner) -> Option<&mut ParameterBank> {
    match owner {
        PageOwner::Device(dev) => device_mut(model, dev)?.parameter_bank_mut(),
        PageOwner::Project => model.project_mut().host_parameter_bank_mut(),
        // Bitwig: the named pages of the cursor track's remote controls and the page selected
        // in Bitwig (CursorTrackImpl.getParameterBank of the Java version)
        PageOwner::TrackFx => model.cursor_track_parameter_bank_mut(),
    }
}

/// A parameter of the host which is reported as `<prefix> <value>`, `<prefix>/str <text>`,
/// `<prefix>/str/<limit> <text>` and `<prefix>/name <name>` (protocol v2).
fn host_param_mut(model: &mut Model, param: HostParam) -> Option<&mut Parameter> {
    match param {
        // Bitwig (TransportImpl.getCrossfadeParameter): the crossfader of the transport; the
        // master crossfader then sends `transport crossfade`
        HostParam::Crossfade => {
            let param = model.crossfade_parameter_mut()?;
            if param.kind() != ParameterKind::HostCrossfade {
                param.set_kind(ParameterKind::HostCrossfade);
            }
            Some(param)
        }
        // Bitwig (ProjectImpl): cue volume and mix
        HostParam::Cue(CueParam::Volume) => Some(model.project_mut().cue_volume_parameter_mut()),
        HostParam::Cue(CueParam::Mix) => Some(model.project_mut().cue_mix_parameter_mut()),
        // Bitwig: the six parameters of GrooveImpl; from now on the host's parameters are used
        HostParam::Groove(groove) => model.groove_mut().host_parameter_mut(groove.name()),
    }
}

impl Parser {
    pub fn new() -> Self {
        Parser { events: Vec::new() }
    }

    /// Parse a message line `"<address> <value>"` (the value may contain spaces).
    pub fn parse_line(&mut self, model: &mut Model, line: &str) -> Vec<ParserEvent> {
        let (address, value) = line.split_once(' ').unwrap_or((line, ""));
        self.parse(model, address, value)
    }

    /// Parse a message and update the model. Returns the events for the app layer.
    pub fn parse(&mut self, model: &mut Model, address: &str, value: &str) -> Vec<ParserEvent> {
        let path = address.strip_prefix('/').unwrap_or(address);
        if !path.is_empty() {
            let host = model.host().clone();
            match Msg::from_text(address, value, &mut |error| host.error(error)) {
                Ok(msg) => return self.apply_msg(model, msg),
                // An invalid value: ignored
                Err(Miss::Value) => {}
                Err(Miss::Path) => host.error(&format!("Unhandled OSC address: {address} {value}")),
            }
        }
        model.after_parse();
        std::mem::take(&mut self.events)
    }

    /// Apply a message (of the text or the binary form) and update the model. Returns the events
    /// for the app layer.
    pub fn apply_msg(&mut self, model: &mut Model, msg: Msg<'_>) -> Vec<ParserEvent> {
        if let Msg::ActionSelect { value } = msg {
            // Not a model value
            return vec![ParserEvent::ActionSelected(value.to_string())];
        }
        self.apply(model, msg);
        model.after_parse();
        std::mem::take(&mut self.events)
    }

    fn rebind_knobs(&mut self) {
        self.events.push(ParserEvent::RebindKnobs);
    }

    fn update_note_mapping(&mut self) {
        self.events.push(ParserEvent::UpdateNoteMapping);
    }

    /// Apply a message to the model. Every message of the table has its arm (no wildcard), so
    /// a message without a handler does not compile.
    pub fn apply(&mut self, model: &mut Model, msg: Msg<'_>) {
        match msg {
            // ------------------------------------------------------------------------------
            // Transport, time, beats, click
            Msg::Play { value } => model.transport_mut().set_play_state(value),
            Msg::Stop { value } => model.transport_mut().set_play_state(value == 0.0),
            Msg::Repeat { value } => model.transport_mut().set_looping_state(value),
            Msg::Record { value } => model.transport_mut().set_record_state(value),
            Msg::Tempo { value } => model.transport_mut().tempo_parameter_mut().set_internal_value(value),
            Msg::Time { value } => model.transport_mut().set_position_value(value),
            Msg::TimeStr { value } => model.transport_mut().set_position_text(value),
            Msg::TimeText { value } => model.transport_mut().set_position_text_verbatim(value),
            Msg::TimeHZoom { value } => model.transport_mut().set_h_zoom(value),
            Msg::TimeLoopStart { value } => model.transport_mut().set_loop_start_value(value),
            Msg::TimeLoopStartStr { value } => model.transport_mut().set_loop_start_text(value),
            Msg::TimeLoopStartBeat { value } => model.transport_mut().set_loop_start_beat_text(value),
            Msg::TimeLoopLength { value } => model.transport_mut().set_loop_length_value(value),
            Msg::TimeLoopLengthStr { value } => model.transport_mut().set_loop_length_text(value),
            Msg::TimeLoopLengthBeat { value } => model.transport_mut().set_loop_length_beat_text(value),
            Msg::BeatsPosition { value } => model.transport_mut().set_position_beats_value(value),
            Msg::BeatsText { value } => model.transport_mut().set_beats_verbatim(value),
            Msg::BeatsLoopStart { value } => model.transport_mut().set_loop_start_beats_value(value),
            Msg::BeatsLoopLength { value } => model.transport_mut().set_loop_length_beats_value(value),
            Msg::BeatsLoopStartText { value } => model.transport_mut().set_loop_start_beat_text_verbatim(value),
            Msg::BeatsLoopLengthText { value } => model.transport_mut().set_loop_length_beat_text_verbatim(value),
            Msg::Beat { value } => model.transport_mut().set_beats(value),
            Msg::Numerator { value } => {
                if value > 0 {
                    model.transport_mut().set_numerator(value);
                }
            }
            Msg::Denominator { value } => {
                if value > 0 {
                    model.transport_mut().set_denominator(value);
                }
            }
            // Bitwig: the independent punch switches, the fill mode
            Msg::PunchIn { value } => model.transport_mut().set_host_switch_state("punchIn", value),
            Msg::PunchOut { value } => model.transport_mut().set_host_switch_state("punchOut", value),
            Msg::Fill { value } => model.transport_mut().set_host_switch_state("fill", value),
            Msg::FollowPlayback { value } => model.arranger_mut().set_playback_follow(value),
            // Bitwig: the arranger and clip launcher overdub switches
            Msg::OverdubArranger { value } => model.transport_mut().set_arranger_overdub_state(value),
            Msg::OverdubLauncher { value } => model.transport_mut().set_launcher_overdub_state(value),
            Msg::AutomationMode { value } => {
                let mode = match value {
                    0 => AutomationMode::TrimRead,
                    1 => AutomationMode::Read,
                    2 => AutomationMode::Touch,
                    3 => AutomationMode::Write,
                    4 => AutomationMode::Latch,
                    5 => AutomationMode::LatchPreview,
                    _ => return,
                };
                model.transport_mut().set_automation_write_mode_state(mode);
            }
            // Bitwig: the arranger and clip launcher automation write switches
            Msg::AutomationArranger { value } => model.transport_mut().set_writing_arranger_automation_state(value),
            Msg::AutomationLauncher { value } => model.transport_mut().set_writing_clip_launcher_automation_state(value),
            Msg::Metronome { value } => model.transport_mut().set_metronome_state(value),
            Msg::MetronomePreroll { value } => model.transport_mut().set_preroll_metronome_internal((value & 2) > 0),
            Msg::MetronomePrerollMeasures { value } => model.transport_mut().set_preroll_measures_internal(value),
            Msg::MetronomeVolume { value } => model.transport_mut().set_internal_metronome_volume(value),
            Msg::MetronomeVolumeStr { value } => model.transport_mut().metronome_volume_parameter_mut().set_metronome_volume_str(value),
            // Bitwig: isMetronomeTickPlaybackEnabled
            Msg::MetronomeTicks { value } => model.transport_mut().set_host_switch_state("ticks", value),

            // ------------------------------------------------------------------------------
            // Application, project, host
            Msg::ProjectName { value } => {
                model.project_mut().set_internal_name(value);
                self.update_note_mapping();
            }
            Msg::ProjectEngine { value } => model.application_mut().set_internal_engine_active(value),
            Msg::ProjectCanUndo { value } => model.application_mut().set_can_undo_state(value),
            Msg::ProjectCanRedo { value } => model.application_mut().set_can_redo_state(value),
            Msg::ProjectIsDirty { value } => model.project_mut().set_dirty(value),
            // Bitwig (ProjectImpl): solo and mute of all tracks
            Msg::ProjectHasSolo { value } => model.track_bank_mut().set_host_project_solo(value),
            Msg::ProjectHasMute { value } => model.track_bank_mut().set_host_project_mute(value),
            // Bitwig: application.panelLayout ()
            Msg::Layout { value } => model.application_mut().set_internal_panel_layout(value),
            // Bitwig (ApplicationImpl): the global record quantization grid and note length
            Msg::RecordQuantization { value } => model.ctx().update_host_record_quantization(None, Some(value)),
            Msg::RecordQuantizationNoteLength { value } => model.ctx().update_host_record_quantization(Some(value), None),
            // Bitwig: the visibility flags of the arranger and the mixer (ArrangerImpl, MixerImpl)
            Msg::Arranger { flag, value } => model.arranger_mut().set_flag_state(flag.name(), value),
            Msg::Mixer { flag, value } => model.mixer_mut().set_flag_state(flag.name(), value),
            // The command forms the host understands (HostFeature), e.g. `/host/features
            // valueInc valueReset`
            Msg::HostFeatures { value } => model.ctx().set_host_features(value),
            Msg::ActionSelect { value } => self.events.push(ParserEvent::ActionSelected(value.to_string())),

            // ------------------------------------------------------------------------------
            // Track banks, channels, sends
            Msg::TrackCount { value } => {
                let tb = model.track_bank_mut();
                tb.set_item_count(value.max(0) as usize);
                tb.mark_dirty();
                self.rebind_knobs();
            }
            // The host pages the tracks (protocol v2); the core keeps its own offset
            Msg::TrackOffset {} => model.track_bank_mut().enable_host_paging(),
            // Bitwig (CursorTrackImpl): the position of the pinned cursor track (-1 if not
            // pinned), hasPrevious (), hasNext ()
            Msg::TrackPinned { value } => model.set_host_pinned_track(value),
            Msg::TrackHasPrevious { value } => model.set_host_cursor_track_neighbours(Some(value), None),
            Msg::TrackHasNext { value } => model.set_host_cursor_track_neighbours(None, Some(value)),
            // The effect tracks (Bitwig's effect track bank)
            Msg::EffectTrackCount { value } => {
                let tb = model.reported_effect_track_bank_mut();
                tb.set_item_count(value.max(0) as usize);
                tb.mark_dirty();
                self.rebind_knobs();
            }
            Msg::EffectTrackOffset {} => model.reported_effect_track_bank_mut().enable_host_paging(),

            Msg::ChannelExists { ch, value } => channel(model, ch, |track| track.set_exists(value)),
            Msg::ChannelDepth { ch, value } => {
                let (tb, r) = channel_mut(model, ch);
                tb.track_mut(r).set_depth(value);
                tb.mark_dirty();
            }
            Msg::ChannelActive { ch, value } => {
                let (tb, r) = channel_mut(model, ch);
                tb.track_mut(r).set_internal_is_activated(value);
                tb.mark_dirty();
            }
            // Bitwig (TrackImpl): canHoldNoteData (), canHoldAudioData ()
            Msg::ChannelCanHoldNotes { ch, value } => channel(model, ch, |track| track.set_can_hold_notes_state(value)),
            Msg::ChannelCanHoldAudio { ch, value } => channel(model, ch, |track| track.set_can_hold_audio_state(value)),
            Msg::ChannelType { ch, value } => {
                let (tb, r) = channel_mut(model, ch);
                tb.track_mut(r).set_type(value);
                // A master in the track list of the host replaces the one of the core
                if value == ChannelType::Master {
                    tb.mark_dirty();
                }
            }
            Msg::ChannelIsGroupExpanded { ch, value } => channel(model, ch, |track| track.set_is_group_expanded(value)),
            Msg::ChannelSelected { ch, value: is_selected } => {
                let (tb, r) = channel_mut(model, ch);
                let track = tb.track_mut(r);
                // Only a change: Bitwig reports the values of a track again after a command to
                // it, a repeated selection must not e.g. recall the preferred view. The
                // selection itself, not the pinned track (Track::is_selected)
                if track.channel().is_selected() == is_selected {
                    return;
                }
                track.set_selected(is_selected);
                tb.handle_bank_track_selection(r, is_selected);
                if is_selected {
                    self.update_note_mapping();
                }
            }
            Msg::ChannelNumber { ch, value } => {
                // The position of the master track is calculated by the track bank
                if ch != Ch::Master {
                    channel(model, ch, |track| track.set_position(value));
                } else {
                    channel(model, ch, |_| {});
                }
            }
            Msg::ChannelName { ch, value } => channel(model, ch, |track| track.set_internal_name(value)),
            Msg::ChannelVolume { ch, value } => channel(model, ch, |track| track.set_internal_volume(value)),
            Msg::ChannelVolumeStr { ch, limit, value } => channel(model, ch, |track| match limit {
                Some(limit) => track.volume_parameter_mut().set_limited_value_str(limit, value),
                None => track.set_volume_str(value),
            }),
            Msg::ChannelPan { ch, value } => channel(model, ch, |track| track.set_internal_pan(value)),
            Msg::ChannelPanStr { ch, limit, value } => channel(model, ch, |track| match limit {
                Some(limit) => track.pan_parameter_mut().set_limited_value_str(limit, value),
                None => track.set_pan_str(value),
            }),
            Msg::ChannelVu { ch, value } => channel(model, ch, |track| track.set_vu(value)),
            Msg::ChannelVuLeft { ch, value } => channel(model, ch, |track| track.set_vu_left(value)),
            Msg::ChannelVuRight { ch, value } => channel(model, ch, |track| track.set_vu_right(value)),
            Msg::ChannelVuHoldLeft { ch, value } => channel(model, ch, |track| track.set_vu_hold_left(value)),
            Msg::ChannelVuHoldRight { ch, value } => channel(model, ch, |track| track.set_vu_hold_right(value)),
            Msg::ChannelMute { ch, value } => channel(model, ch, |track| track.set_mute_state(value)),
            Msg::ChannelSolo { ch, value } => channel(model, ch, |track| track.set_solo_state(value)),
            Msg::ChannelRecArm { ch, value } => channel(model, ch, |track| track.set_rec_arm_state(value)),
            Msg::ChannelMonitor { ch, value } => channel(model, ch, |track| track.set_monitor_state(value)),
            Msg::ChannelAutoMonitor { ch, value } => channel(model, ch, |track| track.set_auto_monitor_state(value)),
            Msg::ChannelOverdub { ch, value } => channel(model, ch, |track| track.set_overdub(value)),
            Msg::ChannelInQuantLengthEnabled { ch, value } => channel(model, ch, |track| track.set_record_quantization_note_length_state(value)),
            Msg::ChannelInQuantResolution { ch, value } => channel(model, ch, |track| track.set_record_quantization_grid_value(value)),
            Msg::ChannelColor { ch, value } => channel(model, ch, |track| track.set_color_state(value.unwrap_or(gray()))),
            Msg::ChannelIsStopped { ch, value } => channel(model, ch, |track| track.set_stopped_state(value)),
            Msg::ChannelPlayingNotes { ch, value } => {
                let (tb, r) = channel_mut(model, ch);
                tb.track_mut(r);
                if let TrackRef::Position(p) = r {
                    tb.handle_notes(p, &value);
                }
            }

            Msg::SendCount { ch, value } => {
                channel(model, ch, |track| track.send_bank_mut().set_item_count(value.max(0) as usize));
                self.rebind_knobs();
            }
            Msg::SendActive { ch, send, value } => channel(model, ch, |track| track.send_bank_mut().unpaged_item_mut(send).set_internal_enabled(value)),
            Msg::SendName { ch, send, value } => channel(model, ch, |track| {
                let send = track.send_bank_mut().unpaged_item_mut(send);
                send.set_internal_name(value);
                send.set_exists(!value.is_empty());
            }),
            Msg::SendVolume { ch, send, value } => channel(model, ch, |track| track.send_bank_mut().unpaged_item_mut(send).set_internal_value(value)),
            Msg::SendVolumeStr { ch, send, limit, value } => channel(model, ch, |track| {
                let send = track.send_bank_mut().unpaged_item_mut(send);
                match limit {
                    Some(limit) => send.set_limited_value_str(limit, value),
                    None => send.set_value_str(value),
                }
            }),
            Msg::SendColor { ch, send, value } => channel(model, ch, |track| track.send_bank_mut().unpaged_item_mut(send).set_color_state(value.unwrap_or(gray()))),

            // ------------------------------------------------------------------------------
            // The clip launcher slots of a track (protocol v2) with the absolute slot (scene)
            // position. Reported states replace the derived ones.
            Msg::SlotCount { ch, value } => channel(model, ch, |track| track.slot_bank_mut().set_item_count(value.max(0) as usize)),
            Msg::SlotExists { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_exists(value)),
            Msg::SlotHasContent { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_has_content(value)),
            Msg::SlotName { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_internal_name(value)),
            Msg::SlotColor { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_color_state(value.map(ColorEx::from_array).unwrap_or(ColorEx::GRAY))),
            Msg::SlotSelected { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_selected(value)),
            Msg::SlotPlaying { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_playing_state(value)),
            Msg::SlotRecording { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_recording_state(value)),
            Msg::SlotPlayQueued { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_play_queued_state(value)),
            Msg::SlotRecQueued { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_rec_queued_state(value)),
            Msg::SlotStopQueued { ch, slot, value } => self.slot(model, ch, slot, |slot| slot.set_stop_queued_state(value)),

            // Scenes (of every scene bank)
            // The host pages the scenes (protocol v2); the core keeps its own offset
            Msg::SceneOffset {} => model.track_bank_mut().enable_host_scene_paging(),
            Msg::SceneCount { value } => model.for_each_scene_bank(|bank| bank.set_item_count(value.max(0) as usize)),
            Msg::SceneExists { scene, value } => model.for_each_scene_bank(|bank| bank.unpaged_item_mut(scene).set_exists(value)),
            Msg::SceneNumber { scene, value } => model.for_each_scene_bank(|bank| bank.unpaged_item_mut(scene).set_position(value)),
            Msg::SceneName { scene, value } => model.for_each_scene_bank(|bank| bank.unpaged_item_mut(scene).set_internal_name(value)),
            Msg::SceneSelected { scene, value } => model.for_each_scene_bank(|bank| bank.unpaged_item_mut(scene).set_selected(value)),
            Msg::SceneColor { scene, value } => {
                model.for_each_scene_bank(|bank| bank.unpaged_item_mut(scene).set_color_state(value.map(ColorEx::from_array).unwrap_or(ColorEx::GRAY)))
            }
            Msg::ScenePosition { scene, value } => model.for_each_scene_bank(|bank| bank.unpaged_item_mut(scene).set_begin_position(value)),
            Msg::SceneEndPosition { scene, value } => model.for_each_scene_bank(|bank| bank.unpaged_item_mut(scene).set_end_position(value)),

            // ------------------------------------------------------------------------------
            // Devices
            Msg::DeviceCount { dev, value } => {
                if dev == Dev::Device {
                    model.cursor_device_mut().set_device_count(value.max(0) as usize);
                }
            }
            Msg::DeviceExists { dev, value } => device(model, dev, |device| device.set_exists(value)),
            Msg::DevicePosition { dev, value } => device(model, dev, |device| device.set_position(value)),
            Msg::DeviceBypass { dev, value } => device(model, dev, |device| device.set_enabled(!value)),
            Msg::DeviceName { dev, value } => device(model, dev, |device| device.set_name(value)),
            Msg::DeviceWindow { dev, value } => device(model, dev, |device| device.set_window_open(value)),
            Msg::DevicePinned { dev, value } => device(model, dev, |device| device.set_pinned_state(value)),
            Msg::DeviceExpand { dev, value } => device(model, dev, |device| device.set_expanded(value)),
            Msg::DeviceParameters { dev, value } => device(model, dev, |device| device.set_parameter_page_section_visible(value)),
            Msg::DeviceTouchedParam { dev, value } => device(model, dev, |device| device.set_last_touched_parameter_index(value)),
            // Drum pads and layers of a device (protocol v2)
            Msg::DeviceHasDrumPads { dev, value } => device(model, dev, |device| device.set_has_drum_pads(value)),
            Msg::DeviceHasLayers { dev, value } => device(model, dev, |device| device.set_has_layers(value)),
            Msg::DeviceHasMutedPads { dev, value } => device(model, dev, |device| device.drum_pad_bank_mut().set_has_muted_pads(value)),
            Msg::DeviceHasSoloedPads { dev, value } => device(model, dev, |device| device.drum_pad_bank_mut().set_has_soloed_pads(value)),
            Msg::DevicePadCount { .. } => {}
            Msg::DeviceLayerCount { dev, value } => device(model, dev, |device| device.layer_bank_mut().set_item_count(value.max(0) as usize)),
            Msg::DeviceSiblingName { dev, sibling, value } => sibling_of(model, dev, sibling.0, |sibling| {
                sibling.set_name(value);
                sibling.set_exists(!value.is_empty());
            }),
            Msg::DeviceSiblingBypass { dev, sibling, value } => sibling_of(model, dev, sibling.0, |sibling| sibling.set_enabled(!value)),
            Msg::DeviceSiblingPosition { dev, sibling, value } => sibling_of(model, dev, sibling.0, |sibling| sibling.set_position(value)),
            Msg::DeviceSiblingSelected { dev, sibling, value } => sibling_of(model, dev, sibling.0, |sibling| sibling.set_selected(value)),
            Msg::EqBandType { band, value } => device(model, Dev::Eq, |device| device.set_type_internal(band, value)),

            // The drum machine of the cursor track with its 128 pads (protocol v2), applied to
            // the main and the additional drum devices
            Msg::DrumExists { value } => model.for_each_drum_device(|device| device.set_exists(value)),
            Msg::DrumName { value } => model.for_each_drum_device(|device| device.set_name(value)),
            Msg::DrumBypass { value } => model.for_each_drum_device(|device| device.set_enabled(!value)),
            Msg::DrumWindow { value } => model.for_each_drum_device(|device| device.set_window_open(value)),
            Msg::DrumExpand { value } => model.for_each_drum_device(|device| device.set_expanded(value)),
            Msg::DrumParameters { value } => model.for_each_drum_device(|device| device.set_parameter_page_section_visible(value)),
            Msg::DrumHasDrumPads { value } => model.for_each_drum_device(|device| device.set_has_drum_pads(value)),
            Msg::DrumHasLayers { value } => model.for_each_drum_device(|device| device.set_has_layers(value)),
            Msg::DrumHasMutedPads { value } => model.for_each_drum_device(|device| device.drum_pad_bank_mut().set_has_muted_pads(value)),
            Msg::DrumHasSoloedPads { value } => model.for_each_drum_device(|device| device.drum_pad_bank_mut().set_has_soloed_pads(value)),
            Msg::DrumPadCount {} => {}

            // A field of a drum pad or layer of a device (protocol v2)
            Msg::PadExists { pad, value } => for_pad(model, pad, |c| c.set_exists(value)),
            Msg::PadName { pad, value } => for_pad(model, pad, |c| c.set_internal_name(value)),
            Msg::PadColor { pad, value } => for_pad(model, pad, |c| c.set_color_state(value.unwrap_or(gray()))),
            Msg::PadSelected { pad, value } => for_pad(model, pad, |c| c.set_selected(value)),
            Msg::PadActive { pad, value } => for_pad(model, pad, |c| c.set_internal_is_activated(value)),
            Msg::PadMute { pad, value } => for_pad(model, pad, |c| c.set_mute_state(value)),
            Msg::PadSolo { pad, value } => for_pad(model, pad, |c| c.set_solo_state(value)),
            Msg::PadVolume { pad, value } => for_pad(model, pad, |c| c.set_internal_volume(value)),
            Msg::PadVolumeStr { pad, value } => for_pad(model, pad, |c| c.set_volume_str(value)),
            Msg::PadPan { pad, value } => for_pad(model, pad, |c| c.set_internal_pan(value)),
            Msg::PadPanStr { pad, value } => for_pad(model, pad, |c| c.set_pan_str(value)),

            // ------------------------------------------------------------------------------
            // Parameters, pages, host parameters
            Msg::ParamCount { owner, value } => {
                let count = value.max(0) as usize;
                match owner {
                    ParamOwner::Device(dev) => {
                        let Some(device) = device_mut(model, dev) else {
                            return;
                        };
                        device.set_parameter_count(count);
                    }
                    _ => {
                        let Some(bank) = param_bank_mut(model, owner) else {
                            return;
                        };
                        bank.set_item_count(count);
                    }
                }
                self.rebind_knobs();
            }
            Msg::ParamName { owner, param, value } => param_of(model, owner, param, |bank| {
                let p = bank.unpaged_item_mut(param);
                p.set_internal_name(value);
                p.set_position(param as i32);
                p.set_exists(!value.is_empty());
            }),
            Msg::ParamValue { owner, param, value } => param_of(model, owner, param, |bank| {
                bank.unpaged_item_mut(param).set_internal_value(value);
                bank.notify_value_observers(param);
            }),
            Msg::ParamValueStr { owner, param, limit, value } => param_of(model, owner, param, |bank| match limit {
                Some(limit) => bank.unpaged_item_mut(param).set_limited_value_str(limit, value),
                None => bank.unpaged_item_mut(param).set_value_str(value),
            }),
            Msg::ParamSteps { owner, param, value } => param_of(model, owner, param, |bank| bank.unpaged_item_mut(param).set_internal_number_of_steps(value)),
            Msg::ParamModulated { owner, param, value } => param_of(model, owner, param, |bank| bank.unpaged_item_mut(param).set_modulated_value(value)),
            // The named parameter pages (protocol v2): count, selected, <index>/name
            Msg::PageCount { owner, value } => {
                if let Some(bank) = page_bank_mut(model, owner) {
                    bank.set_page_count(value.max(0) as usize);
                }
            }
            Msg::PageSelected { owner, value } => {
                if let Some(bank) = page_bank_mut(model, owner) {
                    if value >= 0 {
                        bank.set_selected_page_from_host(value as usize);
                    }
                }
            }
            Msg::PageName { owner, page, value } => {
                if let Some(bank) = page_bank_mut(model, owner) {
                    bank.set_page_name(page, value);
                }
            }
            // The parameter exists once its value is reported
            Msg::HostParamValue { param, value } => {
                if let Some(param) = host_param_mut(model, param) {
                    param.set_exists(true);
                    param.set_internal_value(value);
                }
            }
            Msg::HostParamStr { param, limit, value } => {
                if let Some(param) = host_param_mut(model, param) {
                    match limit {
                        Some(limit) => param.set_limited_value_str(limit, value),
                        None => param.set_value_str(value),
                    }
                }
            }
            Msg::HostParamName { param, value } => {
                if let Some(param) = host_param_mut(model, param) {
                    param.set_internal_name(value);
                }
            }

            // ------------------------------------------------------------------------------
            // Clip, step grid
            Msg::ClipExists { value } => model.set_cursor_clip_exists(value),
            Msg::ClipStart { value } => model.set_cursor_clip_play_start(value),
            Msg::ClipEnd { value } => model.set_cursor_clip_play_end(value),
            Msg::ClipPlayPosition { value } => model.set_cursor_clip_play_position(value),
            Msg::ClipColor { value } => model.set_cursor_clip_color_value(value.unwrap_or(gray())),
            Msg::ClipLoop { value } => model.set_cursor_clip_loop_is_enabled(value),
            Msg::ClipNotes { value } => model.set_cursor_clip_notes(value),
            // The step grids of the host (protocol v2)
            Msg::ClipLoopStart { value } => grids(model, |clip| clip.set_loop_start_value(value)),
            Msg::ClipLoopLength { value } => grids(model, |clip| clip.set_loop_length_value(value)),
            Msg::ClipShuffle { value } => grids(model, |clip| clip.set_shuffle_value(value)),
            Msg::ClipAccent { value } => grids(model, |clip| clip.set_accent_value(value)),
            Msg::ClipPinned { value } => grids(model, |clip| clip.set_pinned_value(value)),
            Msg::GridPlayingStep { grid, value } => {
                if let Some(clip) = grid_clip(model, grid) {
                    clip.set_host_playing_step(value);
                }
            }
            Msg::GridStep { grid, channel, step, row, value } => {
                if let Some(clip) = grid_clip(model, grid) {
                    clip.set_host_step(channel as i32, step as i32, row as i32, value);
                }
            }
            Msg::ClipAll { value } => model.set_clips(value),

            // ------------------------------------------------------------------------------
            // Browser
            Msg::BrowserPresetsFile { value } => browser(model, |b| b.set_presets_file(value)),
            Msg::BrowserSelectedIndex { value } => browser(model, |b| b.set_preset_selected(value)),
            Msg::BrowserSelectedName {} => {}
            // The browser of the host (protocol v2)
            Msg::BrowserExists { value } => browser(model, |b| b.set_host_active(value)),
            Msg::BrowserPreview { value } => browser(model, |b| b.set_preview(value)),
            Msg::ContentTypeCount { value } => browser(model, |b| b.set_content_type_count(value.max(0) as usize)),
            Msg::ContentTypeSelected { value } => browser(model, |b| b.set_selected_content_type(value.max(0) as usize)),
            Msg::ContentTypeSelectedName { value } => browser(model, |b| b.set_selected_content_type_name(value)),
            Msg::ContentTypeName { index, value } => browser(model, |b| b.set_content_type_name(index, value)),
            Msg::ColumnCount { value } => browser(model, |b| b.set_filter_column_count(value.max(0) as usize)),
            Msg::ColumnExists { column, value } => browser(model, |b| b.filter_column_mut(column).exists = value),
            Msg::ColumnName { column, value } => browser(model, |b| b.filter_column_mut(column).name = value.to_string()),
            Msg::ColumnWildcard { column, value } => browser(model, |b| b.filter_column_mut(column).wildcard = value.to_string()),
            Msg::ColumnCursorIndex { column, value } => browser(model, |b| b.filter_column_mut(column).cursor_index = value),
            Msg::ColumnCursorExists { column, value } => browser(model, |b| b.filter_column_mut(column).cursor_exists = value),
            Msg::ColumnCursorName { column, value } => browser(model, |b| b.filter_column_mut(column).cursor_name = value.to_string()),
            Msg::ResultCursorName { value } => browser(model, |b| b.set_selected_result_name(value)),
            Msg::ItemExists { item, value } => browser_item(model, item, |i| i.exists = value),
            Msg::ItemName { item, value } => browser_item(model, item, |i| i.name = value.to_string()),
            Msg::ItemSelected { item, value } => browser_item(model, item, |i| i.is_selected = value),
            Msg::ItemHits { item, value } => browser_item(model, item, |i| i.hit_count = value),

            // ------------------------------------------------------------------------------
            // Markers
            Msg::MarkerCount { value } => model.marker_bank_mut().set_item_count(value.max(0) as usize),
            Msg::MarkerExists { marker, value } => model.marker_bank_mut().unpaged_item_mut(marker).set_exists(value),
            Msg::MarkerNumber { marker, value } => model.marker_bank_mut().unpaged_item_mut(marker).set_position(value),
            Msg::MarkerName { marker, value } => model.marker_bank_mut().unpaged_item_mut(marker).set_internal_name(value),
            Msg::MarkerColor { marker, value } => model.marker_bank_mut().unpaged_item_mut(marker).set_color_state(value.unwrap_or(gray())),
            Msg::MarkerPosition { marker } | Msg::MarkerEndPosition { marker } => {
                model.marker_bank_mut().unpaged_item_mut(marker);
            }

            // ------------------------------------------------------------------------------
            // Note repeat, quantize, groove
            Msg::NoteRepeatActive { value } => self.events.push(ParserEvent::NoteRepeatActive(value)),
            Msg::NoteRepeatPeriod { value } => self.events.push(ParserEvent::NoteRepeatPeriod(1.0 / value)),
            Msg::NoteRepeatNoteLength { value } => self.events.push(ParserEvent::NoteRepeatNoteLength(value)),
            Msg::NoteRepeatMode { value } => self.events.push(ParserEvent::NoteRepeatMode(value)),
            Msg::NoteRepeatVelocity { value } => self.events.push(ParserEvent::NoteRepeatUsePressure(value)),
            Msg::QuantizeStrength { value } => self.events.push(ParserEvent::QuantizeAmount(value)),
            Msg::GrooveActive { value } => model.groove_mut().parameter_mut(GrooveParameterId::Enabled).set_internal_value(value),
            Msg::GrooveAmount { value } => model.groove_mut().parameter_mut(GrooveParameterId::ShuffleAmount).set_internal_value(value),
        }
    }

    /// Apply `f` to a slot of a track (which sets its position first).
    fn slot(&mut self, model: &mut Model, ch: Ch, slot: usize, f: impl FnOnce(&mut crate::daw::slot::Slot)) {
        channel(model, ch, |track| {
            let slot_item = track.slot_bank_mut().unpaged_item_mut(slot);
            slot_item.set_position(slot as i32);
            f(slot_item);
        });
    }
}

/// Apply `f` to the track of a channel report (it is created if necessary).
fn channel(model: &mut Model, ch: Ch, f: impl FnOnce(&mut crate::daw::track::Track)) {
    let (tb, r) = channel_mut(model, ch);
    f(tb.track_mut(r));
}

/// Apply `f` to a device of the common device reports if it exists.
fn device(model: &mut Model, dev: Dev, f: impl FnOnce(&mut SpecificDevice)) {
    if let Some(device) = device_mut(model, dev) {
        f(device);
    }
}

/// Apply `f` to a sibling of the cursor device (0-based) if it is in the page.
fn sibling_of(model: &mut Model, dev: Dev, sibling: i32, f: impl FnOnce(&mut crate::daw::device::Device)) {
    if dev != Dev::Device {
        return;
    }
    let bank = model.cursor_device_mut().device_bank_mut();
    if sibling < 0 || sibling as usize >= bank.page_size() {
        return;
    }
    f(bank.item_mut(sibling as usize));
}

/// Apply `f` to the parameter bank of a parameter report; the first master FX parameter is also
/// copied to the crossfader parameter.
fn param_of(model: &mut Model, owner: ParamOwner, param: usize, f: impl FnOnce(&mut ParameterBank)) {
    let Some(bank) = param_bank_mut(model, owner) else {
        return;
    };
    f(bank);
    if owner == ParamOwner::MasterFx && param == 0 {
        // Copy the values to the crossfader parameter
        let mut copy = None;
        if let Some(bank) = model.project().parameter_bank() {
            let source = bank.unpaged_item(0);
            copy = Some((source.name().to_string(), source.does_exist(), source.internal_value(), source.displayed_value()));
        }
        if let (Some((name, exists, v, s)), Some(dest)) = (copy, model.crossfade_parameter_mut()) {
            dest.set_internal_name(&name);
            dest.set_exists(exists);
            dest.set_internal_value(v);
            dest.set_value_str(&s);
        }
    }
}

/// Apply `f` to all cursor clips, which are the step grids of the host from now on.
fn grids(model: &mut Model, mut f: impl FnMut(&mut crate::daw::clip::CursorClip)) {
    model.for_each_cursor_clip(|clip| {
        clip.set_host_grid();
        f(clip);
    });
}

/// The cursor clip of a grid of the host; logs an unknown grid.
fn grid_clip(model: &mut Model, grid: crate::protocol::GridId) -> Option<&mut crate::daw::clip::CursorClip> {
    if model.note_clip_ref(grid.steps as usize, grid.rows as usize).is_none() {
        model.host().error(&format!("Unknown Clip Grid: {}x{}", grid.steps, grid.rows));
        return None;
    }
    model.note_clip_mut(grid.steps as usize, grid.rows as usize)
}

fn browser(model: &mut Model, f: impl FnOnce(&mut crate::daw::browser::Browser)) {
    if let Some(browser) = model.browser_mut() {
        f(browser);
    }
}

/// Apply `f` to an item of a filter column or a result of the browser if it exists.
fn browser_item(model: &mut Model, item: BrowserItem, f: impl FnOnce(&mut crate::daw::browser::BrowserColumnItem)) {
    let Some(browser) = model.browser_mut() else {
        return;
    };
    let item = match item {
        BrowserItem::Column(column, i) => browser.filter_column_mut(column).items.get_mut(i),
        BrowserItem::Result(i) => browser.result_item_mut(i),
    };
    if let Some(item) = item {
        f(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daw::clip::Note;
    use crate::daw::constants::RecordQuantization;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use crate::daw::bank::BankItem;
    use crate::daw::clip::{NotePosition, StepState};
    use crate::daw::constants::transport_constants::{INC_FRACTION_TIME, INC_FRACTION_TIME_SLOW};
    use crate::daw::constants::GrooveParameterId;
    use crate::daw::context::HostFeature;
    use crate::daw::events::ModelEvent;
    use crate::daw::model_setup::ModelSetup;
    use crate::framework::value_changer::ValueChanger;
    use crate::host::{Host, MemoryLog};
    use crate::scale::Scales;
    use std::rc::Rc;

    fn create(setup: ModelSetup) -> (Model, Parser, Rc<RecordingBackend>, Rc<MemoryLog>) {
        let backend = Rc::new(RecordingBackend::default());
        let log = Rc::new(MemoryLog::default());
        let vc = ValueChanger::twos_complement(128, 1);
        let scales = Scales::new(&vc, 36, 100, 8, 8);
        let model = Model::new(Host::new(log.clone()), Sender::new(backend.clone()), setup, vc, scales);
        (model, Parser::new(), backend, log)
    }

    fn feed(model: &mut Model, parser: &mut Parser, lines: &[&str]) -> Vec<ParserEvent> {
        lines.iter().flat_map(|l| parser.parse_line(model, l)).collect()
    }

    #[test]
    fn tracks() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        let events = feed(
            &mut model,
            &mut parser,
            &[
                "/track/count 10",
                "/track/0/exists 1",
                "/track/0/name Drums",
                "/track/0/number 0",
                "track/0/volume 0.5",
                "/track/0/volume/str -6.0 dB",
                "/track/0/mute 1",
                "/track/0/color 255 0 0",
                "/track/0/type INSTRUMENT",
                "/track/1/exists 1",
                "/track/1/select 1",
                "/play 1",
                "/tempo 133.5",
            ],
        );
        assert!(events.contains(&ParserEvent::RebindKnobs));
        assert!(events.contains(&ParserEvent::UpdateNoteMapping));
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let tb = model.track_bank();
        assert_eq!(tb.item_count(), 10);
        assert_eq!(tb.item(0).name(), "Drums");
        assert_eq!(tb.item(0).volume(), 64);
        assert_eq!(tb.item(0).volume_str(), "-6.0 dB");
        assert!(tb.item(0).is_mute());
        assert_eq!(tb.item(0).color(), ColorEx::RED);
        assert_eq!(tb.item(0).channel_type(), ChannelType::Instrument);
        assert!(!tb.item(2).does_exist());
        assert!(!tb.item(8).does_exist(), "Out of page");
        assert_eq!(tb.selected_index(), Some(1));
        assert_eq!(model.cursor_track().position(), 1);
        assert!(model.transport().is_playing());
        assert_eq!(model.transport().tempo(), 133.5);
        let events = model.take_events();
        assert!(events.contains(&ModelEvent::TrackSelectionChanged { index: 1, is_selected: true }));

        // Selecting a track on the second page scrolls the bank
        feed(&mut model, &mut parser, &["/track/1/select 0", "/track/9/exists 1", "/track/9/select 1"]);
        assert_eq!(model.track_bank().scroll_position(), 8);
        assert_eq!(model.track_bank().item(1).position(), 9);
        assert_eq!(model.cursor_track().position(), 9);

        // Commands
        backend.take_calls();
        model.cursor_track_mut().set_volume(127);
        model.track_bank_mut().item_mut(1).toggle_solo();
        model.track_bank_mut().item_mut(5).toggle_solo();
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::DoubleArg("track".into(), "9/volume".into(), 1.0), BackendCall::IntArg("track".into(), "9/solo".into(), 1)]
        );
    }

    #[test]
    fn track_paging() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        let mut lines = vec!["/track/count 20".to_string()];
        for i in 0..20 {
            lines.push(format!("/track/{i}/exists 1"));
        }
        lines.push("/track/0/select 1".into());
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        feed(&mut model, &mut parser, &refs);
        backend.take_calls();
        let tb = model.track_bank_mut();
        assert!(tb.can_scroll_page_forwards());
        assert!(!tb.can_scroll_page_backwards());
        tb.select_next_page();
        assert_eq!(tb.scroll_position(), 8);
        // The previously selected track is no longer on the new page, therefore no deselect
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "8/select".into(), 1)]);
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "8/select".into(), 1)]);
        model.track_bank_mut().scroll_to(17, true);
        assert_eq!(backend.take_calls(), vec![BackendCall::NoArg("track".into(), "16/scrollto".into())]);
        model.track_bank_mut().select_next_page();
        model.track_bank_mut().select_next_page();
        assert_eq!(model.track_bank().scroll_position(), 16);
        assert!(!model.track_bank().can_scroll_page_forwards());
        assert!(model.track_bank().item(3).does_exist());
        assert!(!model.track_bank().item(4).does_exist());
        // Shrink
        feed(&mut model, &mut parser, &["/track/count 5"]);
        assert_eq!(model.track_bank().scroll_position(), 0);
    }

    /// SlotImpl: the slot commands address the track by its position, also when the track bank
    /// is scrolled (the index in the page launched the clip of another track).
    #[test]
    fn slot_commands_use_the_track_position() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        let mut lines = vec!["/track/count 20".to_string(), "/scene/count 8".to_string()];
        for i in 0..20 {
            lines.push(format!("/track/{i}/exists 1"));
        }
        lines.push("/track/10/slot/count 8".into());
        lines.push("/track/10/slot/2/exists 1".into());
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        feed(&mut model, &mut parser, &refs);
        model.track_bank_mut().set_bank_offset(9);
        backend.take_calls();
        assert_eq!(model.track_bank().item(1).position(), 10);
        let slot = model.track_bank_mut().item_mut(1).slot_bank_mut().item_mut(2);
        slot.launch(true, false);
        slot.set_color(ColorEx::RED);
        slot.paste(9, 3);
        slot.insert_file("a.wav");
        assert_eq!(model.track_bank().item(1).slot_bank().item(2).track_position(), 10);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("track".into(), "10/clip/2/launch".into()),
                BackendCall::StringArg("track".into(), "10/clip/2/color".into(), "RGB(255,0,0)".into()),
                BackendCall::StringArg("track".into(), "10/clip/2/paste".into(), "9/3".into()),
                BackendCall::StringArg("track".into(), "10/clip/2/insertFile".into(), "a.wav".into()),
            ]
        );
    }

    #[test]
    fn full_flat_track_list_with_master() {
        let mut setup = ModelSetup::default();
        setup.set_has_full_flat_track_list(true);
        let (mut model, mut parser, _, _) = create(setup);
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1", "/master/select 1"]);
        let tb = model.track_bank();
        assert_eq!(tb.item_count(), 3);
        assert!(tb.item(2).is_master());
        assert_eq!(tb.item(2).name(), "Master");
        assert_eq!(model.cursor_track_ref(), Some(TrackRef::Master));
    }

    #[test]
    fn folders() {
        let mut setup = ModelSetup::default();
        setup.set_has_flat_track_list(false);
        let (mut model, mut parser, _, _) = create(setup);
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/count 4",
                "/track/0/exists 1",
                "/track/0/type GROUP",
                "/track/1/exists 1",
                "/track/1/depth 1",
                "/track/2/exists 1",
                "/track/2/depth 1",
                "/track/3/exists 1",
            ],
        );
        assert_eq!(model.track_bank().item_count(), 2);
        assert_eq!(model.track_bank().item(1).position(), 3);
        feed(&mut model, &mut parser, &["/track/2/select 1"]);
        assert!(model.track_bank().has_parent());
        assert_eq!(model.track_bank().item_count(), 2);
        assert_eq!(model.track_bank().item(1).position(), 2);
        assert_eq!(model.track_bank().item(1).index(), 1);
    }

    #[test]
    fn sends_scenes_slots() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/count 2",
                "/track/0/exists 1",
                "/track/0/send/count 2",
                "/track/0/send/1/name FX",
                "/track/0/send/1/volume 0.25",
                "/track/0/send/1/volume/str -12 dB",
                "/scene/count 10",
                "/scene/0/exists 1",
                "/scene/0/name ",
                "/scene/0/position 0",
                "/scene/0/endPosition 10",
                "/scene/9/exists 1",
                "/scene/9/name Outro",
                "/clip/all 0;2;Clip A;1;255 0 0;0;Clip B;0;-1 0 0;1;",
                "/time 5",
                "/play 1",
            ],
        );
        let track = model.track_bank().item(0);
        let send = track.send_bank().item(1);
        assert!(send.does_exist());
        assert_eq!(send.name(), "FX");
        assert_eq!(send.value(), 32);
        assert_eq!(send.displayed_value(), "-12 dB");
        assert!(!track.send_bank().item(0).does_exist());

        assert_eq!(model.scene_bank().item(0).name(), "Scene 1");
        let slot = track.slot_bank().item(0);
        assert!(slot.does_exist() && slot.has_content());
        assert_eq!(slot.name(), "Clip A");
        assert_eq!(slot.color(), ColorEx::RED);
        assert!(slot.is_selected());
        assert!(slot.is_playing());
        assert!(!track.slot_bank().item(1).is_playing());
        assert!(track.slot_bank().item(1).is_muted());
        let track1 = model.track_bank().item(1);
        assert!(track1.slot_bank().item(1).does_exist());
        assert!(!track1.slot_bank().item(1).has_content());

        backend.take_calls();
        model.track_bank_mut().item_mut(0).slot_bank_mut().item_mut(1).launch(true, false);
        model.track_bank_mut().item_mut(0).send_bank_mut().item_mut(1).set_value(127);
        model.scene_bank_mut().item_mut(0).launch(true, false);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("track".into(), "0/clip/1/launch".into()),
                BackendCall::DoubleArg("track".into(), "0/send/1/volume".into(), 1.0),
                BackendCall::NoArg("scene".into(), "0/launch".into()),
            ]
        );

        // Scrolling the scene bank moves the slot banks
        model.scene_bank_mut().scroll_page_forwards();
        assert_eq!(model.scene_bank().item(1).name(), "Outro");
        // As in Java the offset of the slot bank is clamped to its item count (2 clips)
        assert_eq!(model.track_bank().item(0).slot_bank().scroll_position(), 1);
    }

    #[test]
    fn devices_and_params() {
        let mut setup = ModelSetup::default();
        setup.enable_device(DeviceId::Eq);
        let (mut model, mut parser, backend, log) = create(setup);
        let events = feed(
            &mut model,
            &mut parser,
            &[
                "/device/exists 1",
                "/device/name VST: ReaEQ (Cockos)",
                "/device/count 3",
                "/device/position 0",
                "/device/bypass 0",
                "/device/param/count 12",
                "/device/param/9/name Gain",
                "/device/param/9/value 0.5",
                "/device/param/9/value/str 0 dB",
                "/device/sibling/2/name VST: ReaComp (Cockos)",
                "/eq/exists 1",
                "/eq/band/1/value 8",
                "/primary/exists 1",
            ],
        );
        assert!(events.contains(&ParserEvent::RebindKnobs));
        assert!(log.lines.borrow().iter().all(|l| !l.contains("Unhandled") || l.contains("band")), "{:?}", log.lines.borrow());
        let device = model.cursor_device();
        assert_eq!(device.name(), "ReaEQ");
        assert!(device.is_enabled());
        assert!(device.can_select_next());
        assert_eq!(device.device_bank().item(1).name(), "ReaComp");
        let bank = device.parameter_bank().unwrap();
        assert_eq!(bank.page_bank().item_count(), 2);
        assert!(!bank.item(1).does_exist());
        model.take_events();
        let bank = model.cursor_device_mut().parameter_bank_mut().unwrap();
        bank.select_next_item();
        assert_eq!(bank.item(1).name(), "Gain");
        assert_eq!(bank.item(1).value(), 64);
        assert_eq!(bank.item(1).displayed_value(), "0 dB");
        backend.take_calls();
        bank.item_mut(1).set_value(127);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("device".into(), "param/9/value".into(), 1.0)]);
        assert!(model.take_events().contains(&ModelEvent::ParameterPageChanged { bank: crate::daw::events::ParameterBankId::CursorDevice }));
    }

    #[test]
    fn clip_and_transport() {
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        model.note_clip(8, 128);
        feed(
            &mut model,
            &mut parser,
            &[
                "/clip/exists 1",
                "/clip/start 0",
                "/clip/end 4",
                "/clip/loop 1",
                "/clip/color 0 255 0",
                "/time/str 1:23.456",
                "/time/loop/start 2",
                "/beats/position 7.5",
                "/beats/loop/length 16",
                "/beats/text 2.04.03:00",
                "/time/loop/length/str 0:04.000",
                "/time/loop/length/beat 2.1.00",
                "/numerator 3",
                "/denominator 4",
                "/automode 3",
                "/click 1",
                "/click/volumeStr -3 dB",
                "/project/name Test",
                "/groove/active 1",
                "/groove/amount 0",
                "/marker/count 1",
                "/marker/0/exists 1",
            ],
        );
        let clip = model.cursor_clip().unwrap();
        assert!(clip.does_exist());
        assert!(clip.is_loop_enabled());
        assert_eq!(clip.color(), ColorEx::GREEN);
        let t = model.transport();
        assert_eq!(t.position_text(), "1.23:456");
        assert_eq!(t.loop_start(), 2.0);
        assert_eq!(t.position_beats(), 7.5);
        assert_eq!(t.loop_length_beats(), 16.0);
        assert_eq!(t.beat_text(), "2.04.03:00");
        // The time text must not overwrite the beat text and vice versa
        assert_ne!(t.loop_length_str(), "");
        assert_ne!(t.loop_length_beat_text(), "");
        assert_ne!(t.loop_length_str(), t.loop_length_beat_text());
        assert_eq!(t.quarters_per_measure(), 3);
        assert!(t.is_writing_arranger_automation());
        assert!(t.is_metronome_on());
        assert_eq!(t.metronome_volume_str(), "-3 dB");
        assert_eq!(model.project().name(), "Test");
        assert_eq!(model.groove().parameter(GrooveParameterId::Enabled).value(), 127);
        assert_eq!(model.groove().parameter(GrooveParameterId::ShuffleAmount).displayed_value(), "0.0 %");
        assert_eq!(model.marker_bank().item(0).name(), "Marker 1");
    }

    /// Bitwig reports its shortened texts (`getLimited`) for some lengths; other lengths are cut.
    #[test]
    fn host_limited_texts() {
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &["/track/count 1", "/track/0/exists 1", "/track/0/pan/str 0.00 %", "/track/0/pan/str/6 0.00", "/track/0/volume/str -10.0 dB", "/track/0/volume/str/6 -10.0"],
        );
        let track = model.track_bank().item(0);
        assert_eq!(track.pan_str(), "0.00 %");
        assert_eq!(track.pan_str_limited(6), "0.00");
        assert_eq!(track.volume_str_limited(6), "-10.0");
        // Not reported for this length: cut as for Reaper
        assert_eq!(track.pan_str_limited(3), "0.0");
        feed(&mut model, &mut parser, &["/device/exists 1", "/device/param/count 8", "/device/param/0/name Cutoff", "/device/param/0/value/str 1.50 kHz", "/device/param/0/value/str/6 1.5k"]);
        let param = model.cursor_device().parameter_bank().unwrap().item(0);
        assert_eq!(param.displayed_value(), "1.50 kHz");
        assert_eq!(param.displayed_value_limited(6), "1.5k");
        assert_eq!(param.displayed_value_limited(8), "1.50 kHz");
    }

    /// A repeated selection (Bitwig reports the values of a track again after a command to it)
    /// is no selection change.
    #[test]
    fn repeated_track_selection_is_no_change() {
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1"]);
        model.take_events();
        let selections = |model: &mut Model| model.take_events().into_iter().filter(|e| matches!(e, ModelEvent::TrackSelectionChanged { .. })).count();
        feed(&mut model, &mut parser, &["/track/0/select 1"]);
        assert_eq!(selections(&mut model), 1);
        feed(&mut model, &mut parser, &["/track/0/select 1"]);
        assert_eq!(selections(&mut model), 0);
    }

    /// Bitwig: the automation write switches replace the Reaper automation mode.
    #[test]
    fn host_automation_switches() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/automode/arranger 0", "/automode/launcher 1", "/automode 4", "/overdub/arranger 1", "/overdub/launcher 0"]);
        assert!(model.is_arranger_overdub());
        assert!(!model.is_launcher_overdub());
        backend.take_calls();
        model.toggle_overdub();
        model.toggle_launcher_overdub();
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::IntArg("transport".into(), "overdub/arranger".into(), 0), BackendCall::IntArg("transport".into(), "overdub/launcher".into(), 1)]
        );
        let t = model.transport();
        assert!(!t.is_writing_arranger_automation());
        assert!(t.is_writing_clip_launcher_automation());
        assert_eq!(t.automation_write_mode(), AutomationMode::Latch);
        assert_eq!(t.automation_write_modes()[0], AutomationMode::Read);
        backend.take_calls();
        let t = model.transport_mut();
        t.toggle_write_arranger_automation();
        t.toggle_write_clip_launcher_automation();
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::NoArg("automation".into(), "toggleArranger".into()), BackendCall::NoArg("automation".into(), "toggleLauncher".into())]
        );
    }

    /// TransportImpl of the Java version for Bitwig: independent punch switches, fill mode,
    /// metronome ticks and volume, play / stop / restart / stop and rewind of the host.
    #[test]
    fn host_transport() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        let t = model.transport();
        assert!(!t.is_punch_in_enabled() && !t.is_fill_mode_active() && !t.is_metronome_ticks_on());
        feed(&mut model, &mut parser, &["/punchIn 1", "/punchOut 1", "/fill 1", "/click/ticks 1", "/click/volumeStr -6.0 dB", "/track/offset 0"]);
        let t = model.transport();
        assert!(t.is_punch_in_enabled() && t.is_punch_out_enabled() && t.is_fill_mode_active() && t.is_metronome_ticks_on());
        assert_eq!(t.metronome_volume_str(), "-6.0 dB");
        backend.take_calls();
        let t = model.transport_mut();
        t.play();
        t.stop();
        t.restart();
        t.stop_and_rewind();
        t.toggle_punch_in();
        t.set_punch_out(false);
        t.toggle_fill_mode_active();
        t.set_fill_mode_active(true);
        t.toggle_metronome_ticks();
        t.set_metronome_ticks(false);
        t.change_metronome_volume(1);
        t.metronome_volume_parameter_mut().reset_value();
        let change = ValueChanger::twos_complement(128, 1).calc_knob_change(1) / 128.0;
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("transport".into(), "play".into()),
                BackendCall::NoArg("transport".into(), "stop".into()),
                BackendCall::NoArg("transport".into(), "restart".into()),
                BackendCall::NoArg("transport".into(), "stopAndRewind".into()),
                BackendCall::NoArg("transport".into(), "togglePunchIn".into()),
                BackendCall::IntArg("transport".into(), "punchOut".into(), 0),
                BackendCall::NoArg("transport".into(), "toggleFill".into()),
                BackendCall::IntArg("transport".into(), "fill".into(), 1),
                BackendCall::NoArg("transport".into(), "toggleMetronomeTicks".into()),
                BackendCall::IntArg("transport".into(), "metronomeTicks".into(), 0),
                BackendCall::DoubleArg("transport".into(), "metronomeVolume/inc".into(), change),
                BackendCall::DoubleArg("transport".into(), "metronomeVolume".into(), 0.8),
            ]
        );
        // The host's switches are not changed locally
        assert!(model.transport().is_punch_out_enabled());
    }

    /// ApplicationImpl, ProjectImpl, ArrangerImpl, MixerImpl of the Java version for Bitwig.
    #[test]
    fn host_application_and_project() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        assert!(!model.project().cue_volume_parameter().does_exist());
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/offset 0",
                "/project/engine 0",
                "/layout MIX",
                "/project/hasSolo 1",
                "/project/hasMute 0",
                "/project/cueVolume 0.5",
                "/project/cueVolume/str -6.0 dB",
                "/project/cueVolume/str/6 -6.0",
                "/project/cueMix 0.25",
                "/project/cueMix/str 25 %",
                "/arranger/cueMarkers 1",
                "/arranger/ioSection 1",
                "/mixer/meterSection 1",
                "/mixer/ioSection 0",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        assert!(!model.application().is_engine_active());
        assert!(model.application().is_mixer_layout());
        assert!(model.track_bank().has_solo() && !model.track_bank().has_mute());
        assert_eq!(model.project().cue_volume(), 64);
        assert_eq!(model.project().cue_volume_str(), "-6.0 dB");
        assert_eq!(model.project().cue_volume_str_limited(6), "-6.0");
        assert_eq!(model.project().cue_mix_str(), "25 %");
        assert!(model.arranger().are_cue_markers_visible() && model.arranger().is_io_section_visible() && !model.arranger().is_timeline_visible());
        assert!(model.mixer().is_meter_section_visible() && !model.mixer().is_io_section_visible());
        backend.take_calls();
        model.application_mut().toggle_engine_active();
        model.application_mut().next_panel_layout();
        model.application_mut().previous_panel_layout();
        model.track_bank_mut().clear_solo();
        model.track_bank_mut().clear_mute();
        model.project_mut().change_cue_volume(1);
        model.project_mut().reset_cue_mix();
        model.project_mut().touch_cue_mix(true);
        let change = ValueChanger::twos_complement(128, 1).calc_knob_change(1) / 128.0;
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::IntArg("project".into(), "engine".into(), 1),
                BackendCall::NoArg("application".into(), "layout/+".into()),
                BackendCall::NoArg("application".into(), "layout/-".into()),
                BackendCall::NoArg("project".into(), "unsoloAll".into()),
                BackendCall::NoArg("project".into(), "unmuteAll".into()),
                BackendCall::DoubleArg("project".into(), "cueVolume/inc".into(), change),
                BackendCall::NoArg("project".into(), "cueMix/reset".into()),
                BackendCall::IntArg("project".into(), "cueMix/touch".into(), 1),
            ]
        );
    }

    /// ChannelImpl / TrackImpl / SendImpl of the Java version for Bitwig: activation of tracks
    /// and the master, monitoring, can hold notes/audio, send colors, VU clip state, solo toggle,
    /// indications.
    #[test]
    fn host_tracks_and_channels() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/offset 0",
                "/track/count 2",
                "/track/0/exists 1",
                "/track/1/exists 1",
                "/track/0/active 0",
                "/master/active 0",
                "/track/0/monitor 1",
                "/track/0/canHoldNotes 0",
                "/track/0/canHoldAudio 1",
                "/track/0/send/count 1",
                "/track/0/send/0/name FX",
                "/track/0/send/0/color 255 0 0",
                "/track/1/vuleft 0.995",
                "/track/1/vuright 0.5",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let tb = model.track_bank();
        let track = tb.item(0);
        assert!(!track.is_activated() && track.is_monitor() && !track.can_hold_notes() && track.can_hold_audio_data());
        assert!(tb.item(1).can_hold_notes());
        assert!(!tb.master().is_activated());
        assert_eq!(track.send_bank().item(0).color(), ColorEx::RED);
        assert!(tb.item(1).vu_left_clip_state() && !tb.item(1).vu_right_clip_state() && tb.item(1).vu_clip_state());
        backend.take_calls();
        let tb = model.track_bank_mut();
        tb.item_mut(1).toggle_solo();
        tb.item_mut(1).set_volume_indication(true);
        tb.item_mut(1).set_pan_indication(false);
        tb.item_mut(0).send_bank_mut().item_mut(0).set_indication(true);
        tb.master_mut().set_is_activated(true);
        tb.set_indication(true);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("track".into(), "1/solo/toggle".into()),
                BackendCall::IntArg("track".into(), "1/volume/indicate".into(), 1),
                BackendCall::IntArg("track".into(), "1/pan/indicate".into(), 0),
                BackendCall::IntArg("track".into(), "0/send/0/volume/indicate".into(), 1),
                BackendCall::IntArg("master".into(), "active".into(), 1),
                BackendCall::IntArg("track".into(), "indicate".into(), 1),
            ]
        );
    }

    /// GrooveImpl of the Java version for Bitwig: six parameters with the host's values and
    /// texts, relative changes, reset, touch, indication.
    #[test]
    fn host_groove() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        assert!(!model.groove().parameter(GrooveParameterId::AccentRate).does_exist());
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/offset 0",
                "/groove/enabled/name Groove",
                "/groove/enabled 1",
                "/groove/enabled/str On",
                "/groove/shuffleAmount 0.25",
                "/groove/shuffleAmount/str 25.0 %",
                "/groove/shuffleRate 0.5",
                "/groove/accentAmount 0",
                "/groove/accentRate 0.5",
                "/groove/accentPhase 0.75",
                "/groove/accentPhase/str/6 75%",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let groove = model.groove();
        assert_eq!(groove.parameter(GrooveParameterId::Enabled).name(), "Groove");
        assert_eq!(groove.parameter(GrooveParameterId::Enabled).displayed_value(), "On");
        assert_eq!(groove.parameter(GrooveParameterId::ShuffleAmount).value(), 32);
        assert_eq!(groove.parameter(GrooveParameterId::ShuffleAmount).displayed_value(), "25.0 %");
        assert!(groove.parameter(GrooveParameterId::AccentRate).does_exist());
        assert_eq!(groove.parameter(GrooveParameterId::AccentPhase).displayed_value_limited(6), "75%");
        backend.take_calls();
        let groove = model.groove_mut();
        groove.parameter_mut(GrooveParameterId::ShuffleAmount).inc(2.0);
        groove.parameter_mut(GrooveParameterId::AccentRate).set_value(127);
        groove.parameter_mut(GrooveParameterId::ShuffleRate).reset_value();
        groove.parameter_mut(GrooveParameterId::Enabled).touch_value(true);
        groove.set_indication(true);
        let calls = backend.take_calls();
        assert_eq!(
            calls[..5],
            [
                BackendCall::DoubleArg("groove".into(), "shuffleAmount/inc".into(), 2.0 / 128.0),
                BackendCall::DoubleArg("groove".into(), "accentRate".into(), 1.0),
                BackendCall::NoArg("groove".into(), "shuffleRate/reset".into()),
                BackendCall::IntArg("groove".into(), "enabled/touch".into(), 1),
                BackendCall::IntArg("groove".into(), "enabled/indicate".into(), 1),
            ]
        );
        assert_eq!(calls.len(), 10);
    }

    /// AbstractTrackBankImpl.getEditSendName (the send of the first track),
    /// CursorClipImpl.resetAccent (setImmediately).
    #[test]
    fn send_names_and_accent_reset() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        model.note_clip(8, 128);
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1", "/track/0/send/count 2", "/track/0/send/1/name Reverb", "/clip/exists 1", "/clip/grid/8x128/playingStep 0"]);
        assert!(model.track_bank().can_edit_send(1) && !model.track_bank().can_edit_send(0));
        assert_eq!(model.track_bank().edit_send_name(1), "Reverb");
        backend.take_calls();
        model.note_clip(8, 128).reset_accent();
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("clip".into(), "accent/immediately".into(), 0.5)]);
    }

    /// Bitwig: scene scrolling as AbstractItemBank (selected scene and pages), the generic
    /// selections on drum pads (DrumPadImpl.select), layers scroll exactly.
    #[test]
    fn host_scene_scrolling_pads_and_layers() {
        let mut setup = ModelSetup::default();
        setup.device_ids.push(DeviceId::FirstInstrument);
        let (mut model, mut parser, backend, _) = create(setup);
        feed(&mut model, &mut parser, &["/scene/count 4", "/scene/0/exists 1", "/scene/1/exists 1", "/scene/offset 0", "/track/offset 0"]);
        // Reaper would allow scrolling forwards by one (offset 0 of 4); Bitwig: no selected scene
        // and no next page
        assert!(!model.scene_bank().can_scroll_forwards() && !model.scene_bank().can_scroll_backwards());
        feed(&mut model, &mut parser, &["/scene/0/select 1"]);
        assert!(model.scene_bank().can_scroll_forwards());
        feed(&mut model, &mut parser, &["/primary/exists 1", "/primary/hasDrumPads 1", "/primary/pad/36/exists 1", "/primary/pad/37/exists 1", "/primary/layer/count 20"]);
        backend.take_calls();
        let device = model.specific_device_mut(DeviceId::FirstInstrument).unwrap();
        let pads = device.drum_pad_bank_mut();
        let index = (37 - pads.scroll_position()) as usize;
        pads.item_mut(index).select();
        device.layer_bank_mut().scroll_to(5, true);
        assert_eq!(device.layer_bank().scroll_position(), 5);
        assert_eq!(backend.take_calls(), vec![BackendCall::NoArg("primary".into(), "pad/37/select".into())]);
    }

    /// CursorDeviceImpl.selectPrevious/Next: the sibling page moves at its edges and its first
    /// device is selected 75 ms later (AbstractItemBankImpl.selectPreviousPage/NextPage).
    #[test]
    fn host_cursor_device_siblings() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/offset 0", "/device/exists 1", "/device/count 12", "/device/position 7", "/device/sibling/1/name A"]);
        backend.take_calls();
        model.cursor_device_mut().select_next();
        model.cursor_device_mut().select_previous();
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::NoArg("device".into(), "+".into()), BackendCall::NoArg("device".into(), "page/+".into()), BackendCall::NoArg("device".into(), "-".into())]
        );
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::NoArg("device".into(), "sibling/1/select".into())]);
    }

    /// ApplicationImpl: the record quantization is global on Bitwig (TrackImpl delegates).
    #[test]
    fn host_record_quantization() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1", "/track/1/select 1", "/recordQuantization 1/16", "/recordQuantization/noteLength 1"]);
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let track = model.track_bank().item(0);
        assert_eq!(track.record_quantization_grid(), RecordQuantization::Res1_16);
        assert!(track.is_record_quantization_note_length());
        backend.take_calls();
        let track = model.cursor_track_mut();
        track.next_record_quantization();
        track.toggle_record_quantization_note_length();
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::StringArg("application".into(), "recordQuantization".into(), "1/8".into()),
                BackendCall::NoArg("application".into(), "recordQuantization/noteLength/toggle".into()),
            ]
        );
    }

    /// AbstractTrackBankImpl.addChannel: the extension creates the track from the string
    /// arguments (type, name, color, devices) and sets name and color 300 ms later.
    #[test]
    fn add_track_command() {
        let (mut model, _, backend, _) = create(ModelSetup::default());
        model.application_mut().add_instrument_track();
        model.application_mut().add_channel(ChannelType::Effect, Some("FX"), Some(ColorEx::RED), &["ReaEQ"]);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::StringArgs("track".into(), "addTrack".into(), vec!["INSTRUMENT".into(), "".into(), "".into()]),
                BackendCall::StringArgs("track".into(), "addTrack".into(), vec!["EFFECT".into(), "FX".into(), "RGB(255,0,0)".into(), "ReaEQ".into()]),
            ]
        );
    }

    /// ProjectImpl: the project's remote controls (root track group) with their own commands.
    #[test]
    fn host_project_remote_controls() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &["/track/offset 0", "/project/param/count 16", "/project/param/9/name Macro", "/project/param/9/value 0.5", "/project/param/9/value/str 50 %", "/project/page/count 2", "/project/page/1/name Mix", "/project/page/selected 1"],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let bank = model.project().parameter_bank().unwrap();
        assert_eq!(bank.scroll_position(), 8);
        assert_eq!(bank.item(1).name(), "Macro");
        assert_eq!(bank.item(1).displayed_value(), "50 %");
        assert_eq!(bank.page_bank().selected_item(), "Mix");
        backend.take_calls();
        let bank = model.project_mut().parameter_bank_mut().unwrap();
        bank.item_mut(1).change_value(1);
        bank.select_previous_item();
        let change = ValueChanger::twos_complement(128, 1).calc_knob_change(1) / 128.0;
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::DoubleArg("project".into(), "param/9/value/inc".into(), change), BackendCall::IntArg("project".into(), "parameterPage".into(), 0)]
        );
    }

    #[test]
    fn note_repeat_and_notes() {
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        let events = feed(&mut model, &mut parser, &["/noterepeat/active 1", "/noterepeat/period 4", "/quantize/strength 50"]);
        assert_eq!(events, vec![ParserEvent::NoteRepeatActive(true), ParserEvent::NoteRepeatPeriod(0.25), ParserEvent::QuantizeAmount(50)]);
        assert_eq!(parser.parse(&mut model, "/action/select", "123"), vec![ParserEvent::ActionSelected("123".into())]);
        feed(&mut model, &mut parser, &["/track/count 1", "/track/0/exists 1"]);
        model.take_events();
        let notes = "0:0:0.0:1.0:0:60:100";
        let parsed = Note::parse_notes(notes);
        if !parsed.is_empty() {
            feed(&mut model, &mut parser, &[&format!("/track/0/playingnotes {notes}")]);
            let events = model.take_events();
            assert!(events.iter().any(|e| matches!(e, ModelEvent::TrackNote { track_index: 0, .. })), "{events:?}");
        }
    }

    #[test]
    fn reported_slot_states() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/count 1",
                "/track/0/exists 1",
                "/track/0/isStopped 0",
                "/track/0/slot/count 2",
                "/track/0/slot/1/exists 1",
                "/track/0/slot/1/hasContent 1",
                "/track/0/slot/1/playQueued 1",
                "/track/0/slot/1/color 255 0 0",
                "/scene/count 2",
                "/scene/1/exists 1",
                "/scene/1/select 1",
            ],
        );
        let track = model.track_bank().item(0);
        assert!(track.is_playing());
        let slot = track.slot_bank().item(1);
        assert!(slot.has_content() && slot.is_playing_queued() && !slot.is_playing());
        assert_eq!(slot.color(), ColorEx::RED);
        assert!(model.scene_bank().item(1).is_selected());
        // Scene values which are no colors are not parsed as colors
        assert!(!log.lines.borrow().iter().any(|l| l.contains("Color")), "{:?}", log.lines.borrow());

        backend.take_calls();
        let slot = model.track_bank_mut().item_mut(0).slot_bank_mut().item_mut(1);
        slot.launch(true, true);
        slot.launch(false, true);
        model.track_bank_mut().item_mut(0).stop(false);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("track".into(), "0/clip/1/launchAlt".into()),
                BackendCall::NoArg("track".into(), "0/clip/1/launchReleaseAlt".into()),
                BackendCall::NoArg("track".into(), "0/stop".into()),
            ]
        );
    }

    #[test]
    fn host_paging() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/count 20", "/scene/count 20"]);
        backend.take_calls();
        let bank_calls = |calls: Vec<BackendCall>| calls.into_iter().filter(|c| matches!(c, BackendCall::IntArg(p, _, _) if p == "bank")).collect::<Vec<_>>();
        // Without a reported offset (Reaper) the core pages alone
        model.track_bank_mut().select_next_page();
        assert!(bank_calls(backend.take_calls()).is_empty());
        // The host reports its window offsets: it gets the current offsets and each change
        feed(&mut model, &mut parser, &["/track/offset 0", "/scene/offset 0"]);
        assert_eq!(
            bank_calls(backend.take_calls()),
            vec![BackendCall::IntArg("bank".into(), "track".into(), 8), BackendCall::IntArg("bank".into(), "scene".into(), 0)]
        );
        model.track_bank_mut().select_previous_page();
        model.track_bank_mut().scene_bank_mut().scroll_page_forwards();
        assert_eq!(
            bank_calls(backend.take_calls()),
            vec![BackendCall::IntArg("bank".into(), "track".into(), 0), BackendCall::IntArg("bank".into(), "scene".into(), 8)]
        );
    }

    /// ModelImpl.createNoteClip / recordNoteClip of the Java version for Bitwig.
    #[test]
    fn create_and_record_note_clip() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &["/track/count 1", "/track/0/exists 1", "/track/0/slot/count 8", "/track/0/slot/2/exists 1", "/track/0/slot/3/exists 1", "/track/0/slot/3/recording 1", "/overdub/arranger 0", "/overdub/launcher 0"],
        );
        backend.take_calls();
        let track = model.track_bank().ref_at(0).unwrap();
        model.create_note_clip(track, 2, 4, true);
        model.record_note_clip(track, 2);
        model.record_note_clip(track, 3);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::IntArg("track".into(), "0/createClip/2".into(), 4),
                BackendCall::NoArg("track".into(), "0/clip/2/select".into()),
                BackendCall::NoArg("track".into(), "0/clip/2/launch".into()),
                BackendCall::IntArg("transport".into(), "overdub/launcher".into(), 1),
                BackendCall::NoArg("track".into(), "0/clip/2/record".into()),
                BackendCall::NoArg("track".into(), "0/clip/2/launch".into()),
                BackendCall::NoArg("track".into(), "0/clip/3/launch".into()),
            ]
        );
    }

    /// AbstractItemBankImpl: the selections after a page change are scheduled (75 ms), the
    /// previous item at the first item of a page is the last item of the previous page, the
    /// previous page selects its first item (Bitwig).
    #[test]
    fn page_change_selections() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        let mut lines = vec!["/track/count 20".to_string(), "/scene/count 20".to_string()];
        for i in 0..20 {
            lines.push(format!("/track/{i}/exists 1"));
            lines.push(format!("/scene/{i}/exists 1"));
        }
        lines.push("/track/offset 0".into());
        lines.push("/scene/offset 0".into());
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        feed(&mut model, &mut parser, &refs);
        model.track_bank_mut().set_bank_offset(8);
        model.scene_bank_mut().set_bank_offset(8);
        feed(&mut model, &mut parser, &["/track/8/select 1", "/scene/8/select 1"]);
        backend.take_calls();

        model.scene_bank_mut().select_previous_item();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("bank".into(), "scene".into(), 0)]);
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::NoArg("scene".into(), "7/select".into())]);

        model.scene_bank_mut().select_next_page();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("bank".into(), "scene".into(), 8)]);
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::NoArg("scene".into(), "8/select".into())]);

        model.scene_bank_mut().select_previous_page();
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("bank".into(), "scene".into(), 0), BackendCall::NoArg("scene".into(), "0/select".into())]);

        model.scene_bank_mut().select_item_at_position(13);
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("bank".into(), "scene".into(), 8)]);
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::NoArg("scene".into(), "13/select".into())]);

        model.track_bank_mut().select_previous_item();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("bank".into(), "track".into(), 0)]);
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "7/select".into(), 1)]);

        // AbstractItemBankImpl.scrollTo: exactly to the position, without a selection
        model.track_bank_mut().scroll_to(13, true);
        model.run_all_tasks();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("bank".into(), "track".into(), 13)]);
        assert_eq!(model.track_bank().scroll_position(), 13);
    }

    /// With host paging the banks can scroll back from any offset > 0, as Bitwig's banks.
    #[test]
    fn host_paging_can_scroll_back_from_an_unaligned_offset() {
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/count 20", "/scene/count 20"]);
        model.track_bank_mut().set_bank_offset(1);
        // Without host paging (Reaper) a full page is required
        assert!(!model.track_bank().can_scroll_page_backwards());
        feed(&mut model, &mut parser, &["/track/offset 0", "/scene/offset 0"]);
        assert!(model.track_bank().can_scroll_page_backwards());
        model.track_bank_mut().scene_bank_mut().set_bank_offset(1);
        assert!(model.track_bank().scene_bank().can_scroll_page_backwards());
        model.track_bank_mut().set_bank_offset(0);
        assert!(!model.track_bank().can_scroll_page_backwards());
    }

    #[test]
    fn drum_pads() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &[
                "/drum/exists 1",
                "/drum/name Drum Machine",
                "/drum/bypass 0",
                "/drum/window 0",
                "/drum/expand 1",
                "/drum/hasDrumPads 1",
                "/drum/hasLayers 0",
                "/drum/hasSoloedPads 1",
                "/drum/pad/36/exists 1",
                "/drum/pad/36/name Kick",
                "/drum/pad/36/color 255 0 0",
                "/drum/pad/36/mute 1",
                "/drum/pad/37/exists 0",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        // With reported pads the bank shows exactly the requested drum offset (36), as Bitwig
        let bank = model.drum_device().unwrap().drum_pad_bank();
        let kick = bank.item(0);
        assert!(kick.does_exist() && kick.is_mute());
        assert_eq!(kick.name(), "Kick");
        assert_eq!(kick.color(), ColorEx::RED);
        assert!(!bank.item(1).does_exist());
        assert!(bank.has_soloed_pads() && !bank.has_muted_pads());
        // With reported pads the bank scrolls exactly to the drum offset, as in Bitwig
        model.drum_device_mut().unwrap().drum_pad_bank_mut().scroll_to(36, false);
        assert_eq!(model.drum_device().unwrap().drum_pad_bank().item(0).name(), "Kick");
        model.drum_device_mut().unwrap().drum_pad_bank_mut().scroll_to(32, false);
        // The pad color is not replaced with the track color
        model.track_bank_mut().item_mut(0).set_color_state([0.0, 0.0, 1.0]);
        assert_eq!(model.drum_device().unwrap().drum_pad_bank().item(4).color(), ColorEx::RED);

        backend.take_calls();
        let bank = model.drum_device_mut().unwrap().drum_pad_bank_mut();
        bank.item_mut(4).set_mute(false);
        bank.clear_solo();
        assert_eq!(
            backend.take_calls(),
            vec![BackendCall::IntArg("drum".into(), "pad/36/mute".into(), 0), BackendCall::NoArg("drum".into(), "clearSolo".into())]
        );
    }

    #[test]
    fn full_flat_track_list_with_the_master_of_the_host() {
        let setup = ModelSetup { has_full_flat_track_list: true, ..ModelSetup::default() };
        // Reaper: the core adds the master to the end of the list
        let (mut model, mut parser, _backend, _log) = create(setup.clone());
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1"]);
        model.track_bank_mut().recalc_if_dirty();
        assert_eq!(model.track_bank().item_count(), 3);
        // Bitwig: the master is part of the track list of the host and not added again
        let (mut model, mut parser, _backend, _log) = create(setup);
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1", "/track/1/type MASTER"]);
        model.track_bank_mut().recalc_if_dirty();
        assert_eq!(model.track_bank().item_count(), 2);
    }

    #[test]
    fn host_step_grid() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        model.note_clip(8, 128);
        assert!(model.setup_json().contains("\"noteClips\":[[8,128]]"), "{}", model.setup_json());
        feed(
            &mut model,
            &mut parser,
            &[
                "/clip/exists 1",
                "/clip/loopLength 4",
                "/clip/accent 0.75",
                "/clip/grid/8x128/playingStep 3",
                "/clip/grid/8x128/step/0/2/36 1 0 0 0.25 0.8 0 0 0 0 0 0 0.5 1 0.5 0 0 0 1 0 0 0 0 0 0",
                "/clip/grid/8x128/step/0/3/36 1 0 0 0.25 0.8 0 0 0 0 0 0 0.5 0 0 0 0 0 1 0 0 0 0 0 0",
                "/clip/grid/8x128/step/0/3/36 0",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let clip = model.note_clip(8, 128);
        assert!(clip.is_host_grid());
        assert_eq!(clip.current_step(), 3);
        assert_eq!(clip.loop_length(), 4.0);
        assert_eq!(clip.formatted_accent(), "50%");
        let position = NotePosition { channel: 0, step: 2, note: 36 };
        let step = clip.step(&position);
        assert_eq!((step.state(), step.velocity(), step.chance()), (StepState::Start, 0.8, 0.5));
        assert!(step.is_chance_enabled());
        assert_eq!(clip.step(&NotePosition { channel: 0, step: 3, note: 36 }).state(), StepState::Off);

        backend.take_calls();
        clip.toggle_step(&NotePosition { channel: 0, step: 4, note: 38 }, 100);
        clip.update_step_chance(&position, 0.25);
        clip.set_step_length(0.5);
        clip.scroll_steps_page_forward();
        clip.clear_row(0, 36);
        let calls = backend.take_calls();
        assert_eq!(calls[0], BackendCall::IntArg("clip".into(), "grid/8x128/step/0/4/38/toggle".into(), 100));
        // CursorClipImpl.updateStepChance: only the changed attribute outside of an edit
        assert_eq!(calls[1], BackendCall::DoubleArg("clip".into(), "grid/8x128/step/0/2/36/chance".into(), 0.25));
        assert_eq!(calls[2], BackendCall::DoubleArg("clip".into(), "grid/8x128/stepSize".into(), 0.5));
        assert_eq!(calls[3], BackendCall::NoArg("clip".into(), "grid/8x128/page/+".into()));
        assert_eq!(calls[4], BackendCall::NoArg("clip".into(), "grid/8x128/clearRow/0/36".into()));

        let clip = model.note_clip(8, 128);
        clip.update_step_velocity(&position, 0.5);
        clip.update_step_mute_state(&position, true);
        clip.update_step_duration(&position, 0.75);
        clip.update_step_is_repeat_enabled(&position, true);
        clip.update_step_recurrence_length(&position, 3);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::DoubleArg("clip".into(), "grid/8x128/step/0/2/36/velocity".into(), 0.5),
                BackendCall::DoubleArg("clip".into(), "grid/8x128/step/0/2/36/muted".into(), 1.0),
                BackendCall::DoubleArg("clip".into(), "grid/8x128/step/0/2/36/duration".into(), 0.75),
                BackendCall::DoubleArg("clip".into(), "grid/8x128/step/0/2/36/repeatEnabled".into(), 1.0),
                BackendCall::DoubleArg("clip".into(), "grid/8x128/step/0/2/36/recurrenceLength".into(), 3.0),
            ]
        );
        // In an edit (startEdit/stopEdit) all attributes are written
        clip.start_edit(&[position]);
        backend.take_calls();
        clip.update_step_gain(&position, 0.25);
        assert!(backend.take_calls().is_empty());
        clip.stop_edit();
        let calls = backend.take_calls();
        assert!(matches!(&calls[..], [BackendCall::StringArg(p, c, v)] if p == "clip" && c == "grid/8x128/step/0/2/36/update" && v.split(' ').nth(11) == Some("0.25")), "{calls:?}");

        // CursorClipImpl.changeStepVelocity etc.: the knob change keeps its fraction (1.09 for
        // one step at the default sensitivity, the cast cut it to 1)
        let clip = model.note_clip(8, 128);
        clip.change_step_velocity(&position, 1);
        clip.change_step_chance(&position, 1);
        let change = ValueChanger::twos_complement(128, 1).calc_knob_change(1) / 127.0;
        assert!(change > 1.0 / 127.0);
        let calls = backend.take_calls();
        let value = |i: usize| match &calls[i] {
            BackendCall::DoubleArg(_, _, v) => *v,
            other => panic!("{other:?}"),
        };
        assert!((value(0) - (0.5 + change)).abs() < 1e-9, "{calls:?}");
        assert!((value(1) - (0.25 + change)).abs() < 1e-9, "{calls:?}");
    }

    /// CursorClipImpl.changePlayStart/End, changeLoopStart/Length: relative on Bitwig.
    #[test]
    fn host_clip_time_changes() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        model.note_clip(8, 128);
        feed(&mut model, &mut parser, &["/clip/exists 1", "/clip/loopLength 4"]);
        backend.take_calls();
        let clip = model.note_clip(8, 128);
        clip.change_play_start(1, false);
        clip.change_play_end(127, true);
        clip.change_loop_start(1, true);
        clip.change_loop_length(127, false);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::DoubleArg("clip".into(), "start/inc".into(), INC_FRACTION_TIME),
                BackendCall::DoubleArg("clip".into(), "end/inc".into(), -INC_FRACTION_TIME_SLOW),
                BackendCall::DoubleArg("clip".into(), "loopStart/inc".into(), INC_FRACTION_TIME_SLOW),
                BackendCall::DoubleArg("clip".into(), "loopLength/inc".into(), -INC_FRACTION_TIME),
            ]
        );
        assert_eq!(clip.loop_length(), 4.0);
    }

    #[test]
    fn host_browser() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &[
                "/browser/exists 1",
                "/browser/contentType/count 2",
                "/browser/contentType/0/name Devices",
                "/browser/contentType/1/name Presets",
                "/browser/contentType/selected 1",
                "/browser/contentType/selected/name Presets",
                "/browser/column/count 7",
                "/browser/column/2/exists 1",
                "/browser/column/2/name Category",
                "/browser/column/2/wildcard Any Category",
                "/browser/column/2/cursor/exists 1",
                "/browser/column/2/cursor/name Bass",
                "/browser/column/2/cursorIndex 3",
                "/browser/column/2/item/3/name Bass",
                "/browser/column/2/item/3/hits 12",
                "/browser/result/1/name Fat Bass",
                "/browser/result/1/select 1",
                "/browser/result/cursor/name Fat Bass",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let browser = model.browser().unwrap();
        assert!(browser.is_active() && browser.is_preset_content_type());
        assert!(browser.has_previous_content_type() && !browser.has_next_content_type());
        assert_eq!(browser.filter_column_count(), 7);
        let column = browser.filter_column(2).unwrap();
        assert_eq!((column.name.as_str(), column.cursor_name.as_str(), column.cursor_index), ("Category", "Bass", 3));
        assert_eq!(column.items[3].hit_count, 12);
        assert_eq!(browser.selected_result_index(), 1);
        assert_eq!(browser.selected_result(), Some("Fat Bass"));

        backend.take_calls();
        let browser = model.browser_mut().unwrap();
        browser.select_next_filter_item(2);
        browser.select_previous_result();
        browser.next_content_type();
        browser.replace_slot("", 3, 5);
        browser.replace_drum_pad(crate::backend::Processor::Drum, "Kick", 36);
        browser.add_device("Bass", crate::daw::browser::DeviceChainTarget::Track(2));
        browser.stop_browsing(true);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("browser".into(), "column/2/+".into()),
                BackendCall::NoArg("browser".into(), "result/-".into()),
                BackendCall::NoArg("browser".into(), "contentType/+".into()),
                BackendCall::NoArg("browser".into(), "replace/slot/3/5".into()),
                BackendCall::NoArg("browser".into(), "replace/drum/pad/36".into()),
                BackendCall::NoArg("browser".into(), "add/track/2".into()),
                BackendCall::IntArg("browser".into(), "stop".into(), 1),
            ]
        );
    }

    #[test]
    fn first_instrument_layers() {
        let mut setup = ModelSetup::default();
        setup.device_ids.push(DeviceId::FirstInstrument);
        let (mut model, mut parser, backend, log) = create(setup);
        feed(
            &mut model,
            &mut parser,
            &[
                "/primary/exists 1",
                "/primary/hasDrumPads 0",
                "/primary/hasLayers 1",
                "/primary/layer/count 2",
                "/primary/layer/1/exists 1",
                "/primary/layer/1/name Strings",
                "/primary/layer/1/solo 1",
                "/primary/param/count 8",
                "/primary/param/0/name Cutoff",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let device = model.specific_device(DeviceId::FirstInstrument).unwrap();
        assert!(crate::framework::mode::parameter_mode::uses_layers(device));
        let layer = device.layer_bank().item(1);
        assert!(layer.does_exist() && layer.is_solo());
        assert_eq!(layer.name(), "Strings");

        backend.take_calls();
        let device = model.specific_device_mut(DeviceId::FirstInstrument).unwrap();
        device.layer_bank_mut().item_mut(1).set_mute(true);
        device.drum_pad_bank_mut().clear_mute();
        device.parameter_bank_mut().unwrap().item_mut(0).set_normalized_value(0.5);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::IntArg("primary".into(), "layer/1/mute".into(), 1),
                BackendCall::NoArg("primary".into(), "clearMute".into()),
                BackendCall::DoubleArg("primary".into(), "param/0/value".into(), 0.5),
            ]
        );

        // The selected page is shown in Bitwig as well
        feed(&mut model, &mut parser, &["/primary/param/count 16", "/primary/page/count 2"]);
        backend.take_calls();
        model.specific_device_mut(DeviceId::FirstInstrument).unwrap().parameter_bank_mut().unwrap().select_page(1);
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("primary".into(), "parameterPage".into(), 1)]);
    }

    #[test]
    fn pinning() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/count 3", "/track/0/exists 1", "/track/1/exists 1", "/track/1/select 1"]);
        backend.take_calls();
        model.set_cursor_track_pinned(true);
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "1/pin".into(), 1)]);
        feed(&mut model, &mut parser, &["/track/1/select 0", "/track/0/select 1"]);
        assert_eq!(model.cursor_track().position(), 1);
        assert!(model.track_bank().item(1).is_selected());
        assert!(!model.track_bank().item(0).is_selected());
        model.set_cursor_track_pinned(false);
        assert_eq!(model.cursor_track().position(), 0);
    }

    /// CursorTrackImpl of the Java version for Bitwig: the pin is Bitwig's, previous and next
    /// track move the cursor track.
    #[test]
    fn host_pinning_and_cursor_track_navigation() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &["/track/offset 0", "/track/count 3", "/track/0/exists 1", "/track/1/exists 1", "/track/2/exists 1", "/track/0/select 1", "/track/pinned -1", "/track/hasPrevious 0", "/track/hasNext 1"],
        );
        assert!(!model.cursor_track_can_select_previous() && model.cursor_track_can_select_next());
        backend.take_calls();
        model.set_cursor_track_pinned(true);
        model.toggle_cursor_track_pinned();
        model.cursor_track_select_next();
        model.cursor_track_select_previous();
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::IntArg("track".into(), "pinned".into(), 1),
                BackendCall::NoArg("track".into(), "pinned/toggle".into()),
                BackendCall::NoArg("track".into(), "selectNext".into()),
                BackendCall::NoArg("track".into(), "selectPrevious".into()),
            ]
        );
        // Not pinned until the host reports it
        assert!(!model.is_cursor_track_pinned());
        feed(&mut model, &mut parser, &["/track/pinned 2", "/track/0/select 0", "/track/1/select 1"]);
        assert!(model.is_cursor_track_pinned());
        assert_eq!(model.cursor_track().position(), 2);
        feed(&mut model, &mut parser, &["/track/pinned -1"]);
        assert!(!model.is_cursor_track_pinned());
        assert_eq!(model.cursor_track().position(), 1);
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
    }

    /// `/host/features`: a host which pages but did not report its features supports all of
    /// them (compatibility), a host which reported them exactly the listed ones (unknown names
    /// are ignored), a host which reports neither (Reaper) none.
    #[test]
    fn host_features() {
        let (model, _, _, _) = create(ModelSetup::default());
        assert!(HostFeature::ALL.iter().all(|f| !model.ctx().host_supports(*f)));

        let (mut model, mut parser, _, log) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/offset 0"]);
        assert!(HostFeature::ALL.iter().all(|f| model.ctx().host_supports(*f)));
        feed(&mut model, &mut parser, &["/host/features valueInc bogus soloToggle"]);
        let supported: Vec<HostFeature> = HostFeature::ALL.into_iter().filter(|f| model.ctx().host_supports(*f)).collect();
        assert_eq!(supported, vec![HostFeature::ValueInc, HostFeature::SoloToggle]);
        assert!(model.ctx().is_host_paging());
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());

        // Features without paging
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/host/features transportCommands"]);
        assert!(!model.ctx().is_host_paging());
        assert!(model.ctx().host_supports(HostFeature::TransportCommands) && !model.ctx().host_supports(HostFeature::ValueInc));
    }

    /// A paging host without the feature `valueInc` gets absolute values (and without
    /// `valueReset` and `soloToggle` the core's reset and solo values).
    #[test]
    fn host_paging_without_value_inc_sends_absolute_values() {
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/offset 0", "/host/features paging", "/track/count 1", "/track/0/exists 1", "/track/0/volume 0.5"]);
        backend.take_calls();
        let track = model.track_bank_mut().item_mut(0);
        track.change_volume(1);
        track.reset_volume();
        track.toggle_solo();
        let calls = backend.take_calls();
        assert_eq!(calls.len(), 3, "{calls:?}");
        assert!(matches!(&calls[0], BackendCall::DoubleArg(p, c, v) if p == "track" && c == "0/volume" && *v > 0.5), "{calls:?}");
        assert!(matches!(&calls[1], BackendCall::DoubleArg(p, c, v) if p == "track" && c == "0/volume" && (*v - 0.716).abs() < 0.01), "{calls:?}");
        assert_eq!(calls[2], BackendCall::IntArg("track".into(), "0/solo".into(), 1));

        // The same host with valueInc
        feed(&mut model, &mut parser, &["/host/features valueInc"]);
        backend.take_calls();
        model.track_bank_mut().item_mut(0).change_volume(1);
        let calls = backend.take_calls();
        assert!(matches!(&calls[..], [BackendCall::DoubleArg(p, c, _)] if p == "track" && c == "0/volume/inc"), "{calls:?}");
    }

    #[test]
    fn unhandled_is_logged() {
        let (mut model, mut parser, _, log) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/foo/bar 1", "/track/abc/name x", "/track/0/volume notanumber"]);
        assert_eq!(log.lines.borrow().len(), 2);
    }

    /// TransportImpl.getCrossfadeParameter of the Java version for Bitwig: the host reports the
    /// crossfader of its transport (`/crossfade`), the master crossfader then sends
    /// `transport crossfade` with the suffixes of the host features. Without the report
    /// (Reaper) it stays the master FX parameter.
    #[test]
    fn host_crossfade() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &["/track/offset 0", "/host/features valueInc valueReset indication", "/crossfade 0.25", "/crossfade/str A 50 %", "/crossfade/str/6 A 50%", "/crossfade/name Crossfade"],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        backend.take_calls();
        let param = model.crossfade_parameter_mut().unwrap();
        assert!(param.does_exist());
        assert_eq!(param.name(), "Crossfade");
        assert_eq!(param.value(), 32);
        assert_eq!(param.displayed_value(), "A 50 %");
        assert_eq!(param.displayed_value_limited(6), "A 50%");
        param.set_value(127);
        param.inc(1.0);
        param.reset_value();
        param.touch_value(true);
        param.set_indication(true);
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::DoubleArg("transport".into(), "crossfade".into(), 1.0),
                BackendCall::DoubleArg("transport".into(), "crossfade/inc".into(), 1.0 / 128.0),
                BackendCall::NoArg("transport".into(), "crossfade/reset".into()),
                BackendCall::IntArg("transport".into(), "crossfade/touch".into(), 1),
                BackendCall::IntArg("transport".into(), "crossfade/indicate".into(), 1),
            ]
        );

        // Reaper: the master FX parameter 0
        let (mut model, mut parser, backend, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/master/fx/param/count 1", "/master/fx/param/0/name Crossfade", "/master/fx/param/0/value 0.5"]);
        backend.take_calls();
        model.crossfade_parameter_mut().unwrap().set_value(127);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("master".into(), "user/param/0/value".into(), 1.0)]);
    }

    /// CursorTrackImpl.getParameterBank of the Java version for Bitwig: a host which pins its
    /// cursor track reports the remote controls of that (pinned) track as `/track/fx/param/…`;
    /// Reaper reports the FX parameters of the selected track.
    #[test]
    fn host_track_remote_controls() {
        let (mut model, mut parser, backend, log) = create(ModelSetup::default());
        feed(
            &mut model,
            &mut parser,
            &[
                "/track/offset 0",
                "/track/count 3",
                "/track/0/exists 1",
                "/track/1/exists 1",
                "/track/2/exists 1",
                "/track/0/select 1",
                "/track/pinned 2",
                "/track/fx/param/count 16",
                "/track/fx/param/0/name Gain",
                "/track/fx/param/0/value 0.5",
                "/track/fx/param/0/value/str 0 dB",
                "/track/fx/page/count 2",
                "/track/fx/page/0/name Mix",
                "/track/fx/page/1/name Sends",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        assert_eq!(model.cursor_track().position(), 2);
        let bank = model.cursor_track_parameter_bank().unwrap();
        assert_eq!(bank.item(0).name(), "Gain");
        assert_eq!(bank.item(0).value(), 64);
        assert_eq!(bank.item(0).displayed_value(), "0 dB");
        assert!(!model.track_bank().item(0).parameter_bank().unwrap().item(0).does_exist());
        backend.take_calls();
        model.cursor_track_parameter_bank_mut().unwrap().item_mut(0).set_value(127);
        assert_eq!(backend.take_calls(), vec![BackendCall::DoubleArg("track".into(), "2/param/0/value".into(), 1.0)]);

        // The page names and the page selected in Bitwig, the page is sent back
        assert_eq!(model.cursor_track_parameter_bank().unwrap().page_bank().selected_item(), "Mix");
        feed(&mut model, &mut parser, &["/track/fx/page/selected 1"]);
        let bank = model.cursor_track_parameter_bank().unwrap();
        assert_eq!(bank.scroll_position(), 8);
        assert_eq!(bank.page_bank().selected_item(), "Sends");
        backend.take_calls();
        model.cursor_track_parameter_bank_mut().unwrap().select_previous_item();
        assert_eq!(backend.take_calls(), vec![BackendCall::IntArg("track".into(), "parameterPage".into(), 0)]);

        // Reaper: the selected track
        let (mut model, mut parser, _, _) = create(ModelSetup::default());
        feed(&mut model, &mut parser, &["/track/count 2", "/track/0/exists 1", "/track/1/exists 1", "/track/1/select 1", "/track/fx/param/count 8", "/track/fx/param/0/name Gain"]);
        assert_eq!(model.track_bank().item(1).parameter_bank().unwrap().item(0).name(), "Gain");
    }

    /// The markers of Bitwig's cue marker bank (MarkerImpl of the Java version): exists, number,
    /// name, color and position (ignored); the commands address the reported position.
    #[test]
    fn host_markers() {
        let mut setup = ModelSetup::default();
        setup.set_num_markers(8);
        let (mut model, mut parser, backend, log) = create(setup);
        feed(
            &mut model,
            &mut parser,
            &[
                "/marker/0/exists 1",
                "/marker/0/number 0",
                "/marker/0/name Intro",
                "/marker/0/color 255 0 0",
                "/marker/0/position 4.000000",
                "/marker/1/exists 1",
                "/marker/1/number 1",
                "/marker/1/name ",
                "/marker/2/exists 0",
                "/marker/count 2",
            ],
        );
        assert!(log.lines.borrow().is_empty(), "{:?}", log.lines.borrow());
        let bank = model.marker_bank_mut();
        assert_eq!(bank.item_count(), 2);
        assert_eq!(bank.item(0).name(), "Intro");
        assert_eq!(bank.item(0).color(), ColorEx::RED);
        assert_eq!(bank.item(1).name(), "Marker 2");
        assert!(!bank.item(2).does_exist());
        backend.take_calls();
        bank.item_mut(1).launch(true);
        bank.item_mut(1).select();
        bank.item_mut(0).remove_marker();
        bank.add_marker();
        assert_eq!(
            backend.take_calls(),
            vec![
                BackendCall::NoArg("marker".into(), "1/launch".into()),
                BackendCall::NoArg("marker".into(), "1/select".into()),
                BackendCall::NoArg("marker".into(), "0/remove".into()),
                BackendCall::NoArg("marker".into(), "add".into()),
            ]
        );
    }
}
