// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The MPC/Force specific parts of the control surface (former `ACVSControlSurface`) and the
//! controller state, which is stored in `ctx.ext` ([`AcvsState`]).

use std::time::{Duration, Instant};

use super::definition::AcvsDevice;
use super::display::{self, AcvsDisplay, ITEM_ID_TEMPO, MESSAGE_ID_PONG, MESSAGE_ID_TEXT};
use super::screen_item::ScreenItem;
use crate::config::acvs as config;
use crate::daw::color::ColorEx;
use crate::framework::ctx::{Ctx, ModeManagerId};
use crate::midi::from_hex_str;

const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(5000);
const PING_INTERVAL: u64 = 3000;

// Commands sent from the MPC/Force Touch display on channel 0x00
pub const NOTE_TRACK1_SELECT: i32 = 0;
pub const NOTE_TRACK1_STOP: i32 = 16;
pub const NOTE_CLIP1_LAUNCH: i32 = 24;
pub const NOTE_SCENE1: i32 = 88;
pub const NOTE_SOLO: i32 = 0;
pub const NOTE_MUTE: i32 = 1;
pub const NOTE_CUE: i32 = 2;
pub const NOTE_CROSSFADER: i32 = 4;
pub const NOTE_REC_ARM: i32 = 5;
pub const NOTE_TOGGLE_DEVICE: i32 = 0;
pub const NOTE_PREV_DEVICE: i32 = 1;
pub const NOTE_NEXT_DEVICE: i32 = 2;
pub const NOTE_PREV_BANK: i32 = 3;
pub const NOTE_NEXT_BANK: i32 = 4;

// Additional commands sent from Touch display on channel 0x0A
pub const NOTE_METRONOME: i32 = 0;
pub const NOTE_ARRANGE_OVERDUB: i32 = 3;
pub const NOTE_ARRANGER_AUTOMATION_ARM: i32 = 4;
pub const NOTE_LOOP_SWITCH: i32 = 5;
pub const NOTE_LAUNCH_QUANTIZE: i32 = 6;
pub const NOTE_TOGGLE_ARRANGE_SESSION: i32 = 7;
pub const NOTE_FOLLOW: i32 = 8;
pub const NOTE_DEVICE_LOCK: i32 = 10;
pub const NOTE_NUDGE_DOWN: i32 = 12;
pub const NOTE_NUDGE_UP: i32 = 13;
pub const NOTE_DELETE: i32 = 14;
pub const NOTE_QUANTIZE_INTERVAL: i32 = 15;
pub const NOTE_QUANTIZE: i32 = 16;
pub const NOTE_STOP_ALL_CLIPS: i32 = 20;
pub const NOTE_INSERT_SCENE: i32 = 21;
pub const NOTE_ARRANGE_RECORD: i32 = 22;
pub const CC_PLAY_POSITION: i32 = 0;
pub const CC_MOVE_LOOP: i32 = 1;
pub const CC_LOOP_LENGTH: i32 = 2;

// Commands sent from MPC buttons on channel 0x0C
pub const NOTE_MPC_LAUNCH_CLIP_OR_SCENE1: i32 = 0;
pub const NOTE_MPC_NOTE_REPEAT: i32 = 68;
pub const NOTE_MPC_FULL_LEVEL: i32 = 69;
pub const NOTE_MPC_16_LEVEL: i32 = 70;
pub const NOTE_MPC_ERASE: i32 = 71;
pub const NOTE_MPC_SHIFT: i32 = 72;
pub const NOTE_MPC_UNDO: i32 = 74;
pub const NOTE_MPC_COPY: i32 = 75;
pub const NOTE_MPC_TAP: i32 = 76;
pub const NOTE_MPC_REC: i32 = 77;
pub const NOTE_MPC_OVERDUB: i32 = 78;
pub const NOTE_MPC_STOP: i32 = 79;
pub const NOTE_MPC_PLAY: i32 = 80;
pub const NOTE_MPC_PLAY_START: i32 = 81;
pub const NOTE_MPC_CURSOR_UP: i32 = 106;
pub const NOTE_MPC_CURSOR_DOWN: i32 = 107;
pub const NOTE_MPC_CURSOR_LEFT: i32 = 108;
pub const NOTE_MPC_CURSOR_RIGHT: i32 = 109;

// Commands sent from Force buttons on channel 0x0C
pub const NOTE_FORCE_TRACK_SELECT1: i32 = 0;
pub const NOTE_FORCE_TRACK_ASSIGN1: i32 = 8;
pub const NOTE_FORCE_LAUNCH_CLIP_OR_SCENE1: i32 = 16;
pub const NOTE_FORCE_LAUNCH_SCENE1: i32 = 80;
pub const NOTE_FORCE_MASTER: i32 = 88;
pub const NOTE_FORCE_STOP_ALL_CLIPS: i32 = 89;
pub const NOTE_FORCE_LAUNCH: i32 = 91;
pub const NOTE_FORCE_NOTE: i32 = 92;
pub const NOTE_FORCE_STEP_SEQ: i32 = 93;
pub const NOTE_FORCE_CLIP_SELECT: i32 = 94;
pub const NOTE_FORCE_EDIT: i32 = 95;
pub const NOTE_FORCE_COPY: i32 = 96;
pub const NOTE_FORCE_DELETE: i32 = 97;
pub const NOTE_FORCE_ARP: i32 = 98;
pub const NOTE_FORCE_TAP_TEMPO: i32 = 99;
pub const NOTE_FORCE_MUTE: i32 = 100;
pub const NOTE_FORCE_SOLO: i32 = 101;
pub const NOTE_FORCE_REC_ARM: i32 = 102;
pub const NOTE_FORCE_CLIP_STOP: i32 = 103;
pub const NOTE_FORCE_PLAY: i32 = 104;
pub const NOTE_FORCE_STOP: i32 = 105;
pub const NOTE_FORCE_REC: i32 = 106;
pub const NOTE_FORCE_UNDO: i32 = 107;
pub const NOTE_FORCE_LOAD: i32 = 108;
pub const NOTE_FORCE_SAVE: i32 = 109;
pub const NOTE_FORCE_NAVIGATE: i32 = 113;
pub const NOTE_FORCE_SHIFT: i32 = 114;
pub const NOTE_FORCE_CURSOR_UP: i32 = 115;
pub const NOTE_FORCE_CURSOR_DOWN: i32 = 116;
pub const NOTE_FORCE_CURSOR_LEFT: i32 = 117;
pub const NOTE_FORCE_CURSOR_RIGHT: i32 = 118;
pub const NOTE_FORCE_ASSIGN_A: i32 = 119;
pub const NOTE_FORCE_ASSIGN_B: i32 = 120;

pub const CC_VOLUME: i32 = 0;
pub const CC_PAN: i32 = 1;
pub const CC_SEND1_LEVEL: i32 = 3;
pub const CC_PARAM1_VALUE: i32 = 0;

/// The controller specific state.
pub struct AcvsState {
    /// The active device (read from the configuration at the start of the instance).
    pub device: AcvsDevice,
    pub display: AcvsDisplay,
    /// The mode manager for the track buttons (former `getTrackModeManager`).
    pub track_modes: ModeManagerId,
    last_pong: Option<Instant>,
    is_shutdown: bool,
    /// Blocks repeated pad notes of the MPC (former `noteBlocker`).
    pub note_blocker: [bool; 64],
}

impl AcvsState {
    pub fn new(device: AcvsDevice, track_modes: ModeManagerId) -> Self {
        AcvsState { device, display: AcvsDisplay::new(device), track_modes, last_pong: None, is_shutdown: false, note_blocker: [false; 64] }
    }
}

pub fn state(ctx: &Ctx) -> &AcvsState {
    ctx.ext::<AcvsState>()
}

pub fn state_mut(ctx: &mut Ctx) -> &mut AcvsState {
    ctx.ext_mut::<AcvsState>()
}

pub fn is_force(ctx: &Ctx) -> bool {
    state(ctx).device == AcvsDevice::Force
}

pub fn track_modes(ctx: &Ctx) -> ModeManagerId {
    state(ctx).track_modes
}

/// Set a screen item (former `ACVSDisplay.setScreenItem`).
pub fn set_screen_item(ctx: &mut Ctx, item: ScreenItem, value: i32) {
    let Ctx { ext, surface, .. } = ctx;
    if let Some(state) = ext.downcast_mut::<AcvsState>() {
        state.display.set_screen_item(&surface.output, item, value);
    }
}

/// Set the text of a display item.
pub fn set_row(ctx: &mut Ctx, row: usize, text: &str) {
    state_mut(ctx).display.set_row(row, text);
}

/// Set the color of a display item.
pub fn set_color(ctx: &mut Ctx, row: usize, color: ColorEx) {
    state_mut(ctx).display.set_color(row, color);
}

/// Send the changed texts and colors of the display.
pub fn flush_display(ctx: &mut Ctx) {
    let Ctx { ext, surface, .. } = ctx;
    if let Some(state) = ext.downcast_mut::<AcvsState>() {
        state.display.flush(&surface.output);
    }
}

/// Former `forceFlush` of the surface: all lights and display items are sent again.
pub fn force_flush(ctx: &mut Ctx) {
    ctx.surface.clear_cache();
    state_mut(ctx).display.force_flush();
}

/// Send a ping and repeat it every 3 seconds (former `sendPing`).
pub fn send_ping(ctx: &mut Ctx) {
    if state(ctx).is_shutdown {
        return;
    }
    let now = ctx.host.now();
    if let Some(last_pong) = state(ctx).last_pong {
        if now.saturating_duration_since(last_pong) > HEARTBEAT_TIMEOUT {
            state_mut(ctx).last_pong = None;
            ctx.host.println("Disconnected.");
        }
    }
    let device = state(ctx).device;
    ctx.surface.output.send_sysex(&display::ping_message(device));
    ctx.schedule(PING_INTERVAL, send_ping);
}

/// Handle the system exclusive messages of the device (former `handleSysEx`).
pub fn handle_sysex(ctx: &mut Ctx, data: &str) {
    let bytes = from_hex_str(data);
    let Some(message) = display::parse_message(&bytes) else {
        ctx.host.error(&format!("Unknown System Exclusive message: {data}"));
        return;
    };
    match message.message_id {
        MESSAGE_ID_PONG => {
            let device = config::active_device(&ctx.config);
            if device != message.device {
                config::set_active_device(&mut ctx.config, message.device);
                ctx.host.println(&format!("Switching to {}", message.device.name()));
                ctx.restart_requested = true;
                return;
            }
            let now = ctx.host.now();
            let is_reconnect = state(ctx).last_pong.is_none_or(|last_pong| now.saturating_duration_since(last_pong) > HEARTBEAT_TIMEOUT);
            if is_reconnect {
                force_flush(ctx);
                ctx.host.println(&format!("Connected to {}", device.name()));
            }
            state_mut(ctx).last_pong = Some(now);
        }
        MESSAGE_ID_TEXT => {
            let content = &message.content;
            if content.len() < 2 {
                return;
            }
            // Note: as in Java (content[0] << 8 + content[1]) the shift is 8 + content[1]
            let shift = (8 + content[1] as u32).min(31);
            let item_id = (content[0] as usize) << shift;
            let text: String = content.iter().skip(4).map(|&b| b as char).collect();
            handle_text_message(ctx, item_id, &text);
        }
        other => ctx.host.error(&format!("Unknown ACVS System Exclusive message: {other}")),
    }
}

/// Former `ITextMessageHandler` of the setup: the tempo can be edited on the device.
fn handle_text_message(ctx: &mut Ctx, item_id: usize, text: &str) {
    if item_id != ITEM_ID_TEMPO {
        ctx.host.error(&format!("Unknown text message: {item_id}"));
        return;
    }
    match text.trim().parse::<f64>() {
        Ok(tempo) => ctx.model.transport_mut().set_tempo(tempo),
        Err(_) => ctx.host.error(&format!("Illegal tempo format: {text}")),
    }
}

/// Former `internalShutdown`: clear the tracks and pads on the device.
pub fn shutdown(ctx: &mut Ctx) {
    state_mut(ctx).is_shutdown = true;
    ctx.host.println("Stop sending ping.");
    set_screen_item(ctx, ScreenItem::TrackNumberOfScenes, 0);
    for track_index in 0..8 {
        set_screen_item(ctx, ScreenItem::TrackType(track_index), 0);
    }
    let is_mpc = !is_force(ctx);
    for position in 0..64 {
        if is_mpc {
            set_screen_item(ctx, ScreenItem::MpcPadState(position), 0);
            set_screen_item(ctx, ScreenItem::MpcPadColor(position), 0);
        } else {
            set_screen_item(ctx, ScreenItem::ForcePadState(position), 0);
            set_screen_item(ctx, ScreenItem::ForcePadColor(position), 0);
        }
    }
}
