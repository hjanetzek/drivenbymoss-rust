// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The mode which sends all data to the MPC/Force screen and pads (former `ControlMode`), the
//! crossfade track modes (former `TrackCrossfadeAMode` and `TrackCrossfadeBMode`) and the only
//! view (former `ControlView`).

use super::colors::*;
use super::definition::AcvsDevice;
use super::display::*;
use super::screen_item::{mpc, ScreenItem};
use super::surface::{self as acvs};
use crate::config::acvs as config;
use crate::daw::bank::BankItem;
use crate::daw::color::{ColorEx, DawColor};
use crate::daw::constants::{ChannelType, LaunchQuantization};
use crate::daw::events::ParameterBankId;
use crate::daw::slot::Slot;
use crate::daw::track::Track;
use crate::framework::ctx::Ctx;
use crate::framework::feature::{Mode, View};
use crate::framework::ids::{ContinuousId, Modes};
use crate::framework::mode::parameter_mode::{sequential_controls, ModeBank, ParameterModeBase};
use crate::framework::mode::track::{DefaultTrackMode, TrackModeBehavior};
use crate::framework::parameterprovider::device::BankParameterProvider;
use crate::framework::parameterprovider::provider;
use crate::framework::parameterprovider::special::CombinedParameterProvider;
use crate::framework::parameterprovider::track::VolumeParameterProvider;
use crate::framework::view::base::ViewBase;
use crate::impl_as_any;

/// The colors of the track assign buttons of the Force per track mode (former `MODE_COLORS`).
fn mode_color(mode: Option<Modes>) -> i32 {
    match mode {
        Some(Modes::Mute) => COLOR_ORANGE,
        Some(Modes::Solo) => COLOR_BLUE,
        Some(Modes::RecArm) => COLOR_RED,
        Some(Modes::StopClip) => COLOR_GREEN,
        Some(Modes::CrossfadeModeA) => COLOR_LIGHT_ORANGE,
        Some(Modes::CrossfadeModeB) => COLOR_RED,
        _ => COLOR_BLACK,
    }
}

/// The value of the crossfade parameter of a track; the default (AB) if the track has no
/// crossfade parameter (Reaper: the value is half of the range).
fn crossfade_value(ctx: &Ctx, track: &Track) -> i32 {
    track.crossfade_parameter().map_or(ctx.model.value_changer().upper_bound() / 2, |p| p.value())
}

/// The data which is sent to the screen in one update.
#[derive(Default)]
struct Update {
    items: Vec<(ScreenItem, i32)>,
    rows: Vec<(usize, String)>,
    colors: Vec<(usize, ColorEx)>,
}

impl Update {
    fn item(&mut self, item: ScreenItem, value: i32) {
        self.items.push((item, value));
    }

    fn row(&mut self, row: usize, text: impl Into<String>) {
        self.rows.push((row, text.into()));
    }

    fn color(&mut self, row: usize, color: ColorEx) {
        self.colors.push((row, color));
    }
}

fn on(value: bool) -> i32 {
    if value {
        127
    } else {
        0
    }
}

/// The mode which sends all data to the screen of the MPC/Force (former `ControlMode`).
pub struct ControlMode {
    base: ParameterModeBase,
    current_max_scene: i32,
}

impl ControlMode {
    pub fn new(ctx: &Ctx) -> Self {
        let device = acvs::state(ctx).device;
        let mut base = ParameterModeBase::without_controls("Control", true, ModeBank::TrackBank);
        if device == AcvsDevice::Force || device == AcvsDevice::MpcX {
            let mut knobs = sequential_controls(ContinuousId::VolumeKnob1, 8);
            knobs.extend(sequential_controls(ContinuousId::ParamKnob1, 8));
            base.set_controls(knobs);
            let provider1 = provider(VolumeParameterProvider::new(ctx));
            let provider2 = provider(BankParameterProvider::new(ctx, ParameterBankId::CursorDevice));
            base.set_parameter_provider(ctx, provider(CombinedParameterProvider::new(vec![provider1, provider2])));
        }
        ControlMode { base, current_max_scene: 0 }
    }

    /// Collect all data of the screen (former `updateDisplay`).
    fn collect(&self, ctx: &Ctx, update: &mut Update) -> i32 {
        let is_mpc = !acvs::is_force(ctx);
        let tb = ctx.model.current_track_bank();
        let device = ctx.model.cursor_device();

        let max_scene = calc_max_scene(ctx);
        update.item(ScreenItem::TrackNumberOfScenes, max_scene);
        // Maximum number of sends
        update.item(ScreenItem::TrackSendsNo, calc_number_of_sends(ctx));

        for i in 0..8 {
            // Send track data
            let track = tb.item(i);
            send_track_data(ctx, update, i, track);
            self.send_clip_data(ctx, update, is_mpc, i, track);
        }
        self.send_scene_data(ctx, update, is_mpc);

        // Set device data
        let exists = device.does_exist();
        let parameter_bank = device.parameter_bank();
        let selected_page = parameter_bank.map(|bank| bank.page_bank().selected_item()).unwrap_or_default();
        update.row(ITEM_ID_DEVICE_BANK_NAME, if exists { selected_page } else { String::new() });
        update.row(ITEM_ID_DEVICE_NAME, if exists { device.name().to_string() } else { String::new() });
        update.item(ScreenItem::DeviceCount, if exists { device.device_bank().item_count() as i32 } else { 0 });
        let device_position = device.position();
        update.item(ScreenItem::CurrentDeviceIndex, device_position.max(0));
        update.item(ScreenItem::CurrentDeviceActive, on(device.is_enabled()));
        for i in 0..8 {
            let (name, displayed, does_exist, value) = match parameter_bank {
                Some(bank) => {
                    let param = bank.item(i);
                    (param.name().to_string(), param.displayed_value(), param.does_exist(), param.value())
                }
                None => (String::new(), String::new(), false, 0),
            };
            update.row(ITEM_ID_DEVICE_PARAMETER_NAME_FIRST + i, name.clone());
            update.row(ITEM_ID_DEVICE_PARAMETER_VALUE_FIRST + i, displayed.clone());
            update.item(ScreenItem::DeviceParamEnabled(i), on(does_exist));
            update.item(ScreenItem::DeviceParamValue(i), value);

            // Labels for Q-Link knobs: Q-Link rows 3 + 4 are device parameters
            update.item(ScreenItem::KnobStyleColor(8 + i), i32::from(does_exist));
            update.item(ScreenItem::KnobValue(8 + i), value);
            update.row(ITEM_ID_DEVICE_PARAM_NAME1 + 8 + i, name);
            update.row(ITEM_ID_DEVICE_PARAM_VALUE1 + 8 + i, displayed);

            // Q-Link rows 1 + 2 are track levels
            let track = tb.item(i);
            update.item(ScreenItem::KnobStyleColor(i), i32::from(track.does_exist()));
            update.item(ScreenItem::KnobValue(i), track.volume());
            update.row(ITEM_ID_DEVICE_PARAM_NAME1 + i, track.name().to_string());
            update.row(ITEM_ID_DEVICE_PARAM_VALUE1 + i, track.volume_str());
        }

        // Set transport data
        let transport = ctx.model.transport();
        let beats = |text: &str| text.split(':').next().unwrap_or("").replace('.', ":");
        update.row(ITEM_ID_TEMPO, transport.format_tempo(transport.tempo()));
        update.row(ITEM_ID_ARRANGEMENT_POSITION, beats(transport.beat_text()));
        update.row(ITEM_ID_LOOP_START, beats(transport.loop_start_beat_text()));
        update.row(ITEM_ID_LOOP_LENGTH, beats(transport.loop_length_beat_text()));

        send_additional_mpc_parameters(ctx, update);
        if !is_mpc {
            send_additional_force_parameters(ctx, update);
        }
        max_scene
    }

    fn send_clip_data(&self, ctx: &Ctx, update: &mut Update, is_mpc: bool, track_index: usize, track: &Track) {
        let slot_bank = track.slot_bank();
        // Clips on the screen
        for clip_index in 0..8 {
            let slot = slot_bank.item(clip_index);
            let position = clip_index * 8 + track_index;
            update.row(ITEM_ID_CLIPS_FIRST + position, slot.name().to_string());
            update.item(ScreenItem::ClipState(position), convert_slot_state(slot));
            // Clip colors can alternatively set with CC from the color palette
            update.color(ITEM_ID_CLIPS_FIRST + position, slot.color());
        }
        // Progress currently not supported in the API
        update.item(ScreenItem::TrackClipProgress(track_index), 0);

        // Set pad colors and state representing clips
        if !config::is_launch_clips(&ctx.config) {
            return;
        }
        for clip_index in 0..8 {
            let slot = slot_bank.item(clip_index);
            let position = clip_index * 8 + track_index;
            let slot_state = convert_slot_state(slot);
            let slot_color = convert_slot_state_to_color(ctx, slot);
            if is_mpc {
                update.item(ScreenItem::MpcPadState(position), slot_state);
                update.item(ScreenItem::MpcPadColor(position), slot_color);
            } else {
                update.item(ScreenItem::ForcePadState(position), slot_state);
                update.item(ScreenItem::ForcePadColor(position), slot_color);
            }
        }
    }

    fn send_scene_data(&self, ctx: &Ctx, update: &mut Update, is_mpc: bool) {
        let scene_bank = ctx.model.scene_bank();
        for scene_index in 0..8 {
            // Scenes on the screen
            let scene = scene_bank.item(scene_index);
            update.row(ITEM_ID_SCENES_FIRST + scene_index, scene.name());
            update.color(ITEM_ID_SCENES_FIRST + scene_index, scene.color());
            update.item(ScreenItem::SceneSelect(scene_index), on(scene.is_selected()));
        }

        if is_mpc {
            // Scene on pads
            if config::is_launch_clips(&ctx.config) {
                return;
            }
            for scene_index in 0..8 {
                let scene = scene_bank.item(scene_index);
                let scene_color = if scene.does_exist() { ctx.colors.color_index(DawColor::color_id(scene.color())) } else { COLOR_BLACK };
                let offset = if scene_index < 4 { 0 } else { 4 };
                update.item(ScreenItem::MpcPadState(offset + scene_index), 2);
                update.item(ScreenItem::MpcPadColor(offset + scene_index), scene_color);
            }
            for pad_index in 0..4 {
                update.item(ScreenItem::MpcPadState(16 + pad_index), 0);
                update.item(ScreenItem::MpcPadColor(16 + pad_index), COLOR_BLACK);
                update.item(ScreenItem::MpcPadState(24 + pad_index), 0);
                update.item(ScreenItem::MpcPadColor(24 + pad_index), COLOR_BLACK);
            }
            return;
        }

        for scene_index in 0..8 {
            let scene = scene_bank.item(scene_index);
            let color = if !scene.does_exist() {
                0
            } else if scene.is_selected() {
                1
            } else {
                2
            };
            update.item(ScreenItem::ForceScene(scene_index), color);
        }
    }
}

fn send_track_data(ctx: &Ctx, update: &mut Update, track_index: usize, track: &Track) {
    update.row(ITEM_ID_TRACK_HEADER_FIRST + track_index, track.name().to_string());
    update.color(ITEM_ID_TRACK_HEADER_FIRST + track_index, track.color());
    update.row(ITEM_ID_TRACK_FADER_LEVEL_FIRST + track_index, track.volume_str());
    // ITEM_ID_TRACK_PEAK_LEVEL_FIRST - not showing up
    update.row(ITEM_ID_TRACK_PAN_FIRST + track_index, track.pan_str());

    let is_muted_by_solo = track.does_exist() && ctx.model.current_track_bank().is_muted_by_solo(track.position().max(0) as usize);
    update.item(ScreenItem::TrackSelect(track_index), on(track.is_selected()));
    update.item(ScreenItem::TrackType(track_index), track_type(track));
    update.item(ScreenItem::TrackSolo(track_index), on(track.is_solo()));
    update.item(ScreenItem::TrackMute(track_index), on(track.is_mute()));
    update.item(ScreenItem::TrackSoloMute(track_index), on(is_muted_by_solo));
    // TRACK1_CUE - not supported
    update.item(ScreenItem::TrackCrossfader(track_index), crossfade(crossfade_value(ctx, track)));
    update.item(ScreenItem::TrackRecArm(track_index), on(track.is_rec_arm()));
    update.item(ScreenItem::TrackLevel(track_index), track.volume());
    update.item(ScreenItem::TrackPan(track_index), track.pan());
    update.item(ScreenItem::TrackVuLeft(track_index), track.vu_left());
    update.item(ScreenItem::TrackVuRight(track_index), track.vu_right());
    update.item(ScreenItem::TrackVuPeakLeft(track_index), track.vu_peak_left());
    update.item(ScreenItem::TrackVuPeakRight(track_index), track.vu_peak_right());

    let send_bank = track.send_bank();
    let firsts = [ITEM_ID_TRACK_SEND1_FIRST, ITEM_ID_TRACK_SEND2_FIRST, ITEM_ID_TRACK_SEND3_FIRST, ITEM_ID_TRACK_SEND4_FIRST];
    for (send_index, first) in firsts.iter().enumerate() {
        let send = send_bank.item(send_index);
        update.row(first + track_index, send.name().to_string());
        update.item(ScreenItem::TrackSendLevel(send_index, track_index), send.value());
    }
}

fn send_additional_mpc_parameters(ctx: &Ctx, update: &mut Update) {
    let transport = ctx.model.transport();
    let application = ctx.model.application();
    let device = ctx.model.cursor_device();
    let is_shift = ctx.surface.is_shift_pressed();
    update.item(ScreenItem::Mpc(mpc::METRONOME), on(transport.is_metronome_on()));
    // MPC_CAPTURE_MIDI - not supported
    // MPC_ABLETON_LINK - not supported
    let is_overdub = if is_shift { ctx.model.is_launcher_overdub() } else { ctx.model.is_arranger_overdub() };
    update.item(ScreenItem::Mpc(mpc::ARRANGE_OVERDUB), on(is_overdub));
    let is_automation = if is_shift { transport.is_writing_clip_launcher_automation() } else { transport.is_writing_arranger_automation() };
    update.item(ScreenItem::Mpc(mpc::ARRANGER_AUTOMATION_ARM), on(is_automation));
    update.item(ScreenItem::Mpc(mpc::LOOP_SWITCH), on(transport.is_loop()));
    update.item(ScreenItem::Mpc(mpc::LAUNCH_QUANTIZE), convert_launch_quantization(transport.default_launch_quantization()));
    update.item(ScreenItem::Mpc(mpc::ARRANGEMENT_SESSION), i32::from(application.is_arrange_layout()));
    update.item(ScreenItem::Mpc(mpc::FOLLOW), on(ctx.model.arranger().is_playback_follow_enabled()));
    // MPC_CLIP_DEVICE_VIEW - not supported
    update.item(ScreenItem::Mpc(mpc::DEVICE_LOCK), on(device.is_pinned()));
    update.item(ScreenItem::Mpc(mpc::DETAIL_VIEW), on(device.is_expanded()));
    update.item(ScreenItem::Mpc(mpc::QUANTIZE_INTERVAL), (10 - ctx.config.quantize_amount() / 10).clamp(0, 7));
    update.item(ScreenItem::Mpc(mpc::CLIP_SCENE_LAUNCH), i32::from(!config::is_launch_clips(&ctx.config)));
}

fn send_additional_force_parameters(ctx: &Ctx, update: &mut Update) {
    let active_mode = ctx.mode_manager(acvs::track_modes(ctx)).active_id();
    let mode_color = mode_color(active_mode);
    let launch_quantization = ctx.model.transport().default_launch_quantization();
    let value_changer = ctx.model.value_changer();
    for track_index in 0..8 {
        let track = ctx.model.track_bank().item(track_index);
        let color = if ctx.surface.is_shift_pressed() {
            if LaunchQuantization::ALL.get(track_index) == Some(&launch_quantization) {
                COLOR_SILVER
            } else {
                COLOR_BLACK
            }
        } else if !track.does_exist() {
            COLOR_BLACK
        } else if track.is_selected() {
            COLOR_SILVER
        } else {
            ctx.colors.color_index(DawColor::color_id(track.color()))
        };
        update.item(ScreenItem::ForceTrackColor(track_index), color);

        let crossfade = value_changer.to_normalized_value(crossfade_value(ctx, track) as f64);
        let color = match active_mode {
            Some(Modes::CrossfadeModeA) => {
                if crossfade < 0.1 {
                    mode_color
                } else {
                    COLOR_BLACK
                }
            }
            Some(Modes::CrossfadeModeB) => {
                if crossfade > 0.9 {
                    mode_color
                } else {
                    COLOR_BLACK
                }
            }
            Some(Modes::Mute) => {
                if track.is_mute() {
                    mode_color
                } else {
                    COLOR_BLACK
                }
            }
            Some(Modes::Solo) => {
                if track.is_solo() {
                    mode_color
                } else {
                    COLOR_BLACK
                }
            }
            Some(Modes::RecArm) => {
                if track.is_rec_arm() {
                    mode_color
                } else {
                    COLOR_BLACK
                }
            }
            _ => mode_color,
        };
        update.item(ScreenItem::ForceTrackAssign(track_index), color);
    }
}

/// Former `getCrossfade`.
fn crossfade(value: i32) -> i32 {
    match value {
        // A
        0 => 1,
        // B
        127 => 2,
        _ => 0,
    }
}

/// Former `convertLaunchQuantization (LaunchQuantization)`.
pub fn convert_launch_quantization(launch_quantization: LaunchQuantization) -> i32 {
    match launch_quantization {
        LaunchQuantization::ResNone => 0,
        LaunchQuantization::Res1_16 => 11,
        LaunchQuantization::Res1_8 => 9,
        LaunchQuantization::Res1_4 => 7,
        LaunchQuantization::Res1_2 => 5,
        LaunchQuantization::Res1 => 4,
        LaunchQuantization::Res2 => 3,
        LaunchQuantization::Res4 => 2,
        LaunchQuantization::Res8 => 1,
    }
}

/// Former `getTrackType`.
fn track_type(track: &Track) -> i32 {
    if !track.does_exist() {
        return 0;
    }
    match track.channel_type() {
        ChannelType::Audio => 6,
        ChannelType::Instrument => {
            if track.has_drum_device() {
                2
            } else {
                4
            }
        }
        ChannelType::Hybrid => 4,
        ChannelType::Group | ChannelType::GroupOpen => 7,
        ChannelType::Effect => 8,
        ChannelType::Master => 9,
        _ => 0,
    }
}

/// Former `convertSlotState`.
fn convert_slot_state(slot: &Slot) -> i32 {
    if !slot.does_exist() {
        return 0;
    }
    if !slot.has_content() {
        return 1;
    }
    if slot.is_stop_queued() {
        return if slot.is_recording() { 8 } else { 5 };
    }
    if slot.is_recording() {
        return 7;
    }
    if slot.is_recording_queued() {
        return 6;
    }
    if slot.is_playing() {
        return 4;
    }
    if slot.is_playing_queued() {
        return 3;
    }
    2
}

/// Former `convertSlotStateToColor`.
fn convert_slot_state_to_color(ctx: &Ctx, slot: &Slot) -> i32 {
    if !slot.does_exist() || !slot.has_content() {
        return COLOR_BLACK;
    }
    ctx.colors.color_index(DawColor::color_id(slot.color()))
}

/// Former `calcNumberOfSends`.
fn calc_number_of_sends(ctx: &Ctx) -> i32 {
    let tb = ctx.model.current_track_bank();
    let mut num_sends = 0;
    for i in 0..4 {
        if !tb.can_edit_send(i) {
            break;
        }
        num_sends = i as i32 + 1;
    }
    num_sends
}

/// Former `calcMaxScene`.
fn calc_max_scene(ctx: &Ctx) -> i32 {
    let scene_bank = ctx.model.scene_bank();
    let mut max_scene = 0;
    for i in 0..8 {
        if scene_bank.item(i).does_exist() {
            max_scene = i as i32 + 1;
        }
    }
    max_scene
}

impl Mode for ControlMode {
    fn name(&self) -> &str {
        self.base.name()
    }

    // AbstractParameterMode
    crate::forward_mode!(base => on_activate, on_deactivate, knob_value);

    fn update_display(&mut self, ctx: &mut Ctx) {
        let mut update = Update::default();
        let max_scene = self.collect(ctx, &mut update);
        for (item, value) in update.items {
            acvs::set_screen_item(ctx, item, value);
        }
        let display = &mut acvs::state_mut(ctx).display;
        for (row, text) in update.rows {
            display.set_row(row, &text);
        }
        for (row, color) in update.colors {
            display.set_color(row, color);
        }
        display.all_done();
        if self.current_max_scene != max_scene {
            self.current_max_scene = max_scene;
            acvs::force_flush(ctx);
        }
    }

    impl_as_any!();
}

/// Former `TrackCrossfadeAMode`: toggles the crossfade mode A of a track. Reaper has no
/// crossfade modes for tracks (the parameter does not exist), the function does nothing there.
pub struct CrossfadeABehavior;
impl TrackModeBehavior for CrossfadeABehavior {}

/// Former `TrackCrossfadeBMode`, see [`CrossfadeABehavior`].
pub struct CrossfadeBBehavior;
impl TrackModeBehavior for CrossfadeBBehavior {}

/// Create the crossfade A mode.
pub fn crossfade_a_mode() -> DefaultTrackMode<CrossfadeABehavior> {
    DefaultTrackMode::with_name("Track Crossfade A", true, -1)
}

/// Create the crossfade B mode.
pub fn crossfade_b_mode() -> DefaultTrackMode<CrossfadeBBehavior> {
    DefaultTrackMode::with_name("Track Crossfade B", true, -1)
}

/// The view of the MPC/Force: it has no grid (former `ControlView`).
pub struct ControlView {
    base: ViewBase,
}

impl ControlView {
    pub fn new() -> Self {
        ControlView { base: ViewBase::new("Control") }
    }
}

impl Default for ControlView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ControlView {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn draw_grid(&mut self, _ctx: &mut Ctx) {
        // Not used
    }

    fn on_grid_note(&mut self, _ctx: &mut Ctx, _note: i32, _velocity: i32) {
        // Not used
    }

    impl_as_any!();
}
