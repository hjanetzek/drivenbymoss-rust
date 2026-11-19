// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The commands of the Push (former `controller.ableton.push.command`) and the registration of
//! the buttons and continuous controls (former `registerTriggerCommands` and
//! `registerContinuousCommands` of `PushControllerSetup`).
//!
//! Differences to Java:
//! - The Push 3 buttons SWING (small knob press: tempo/swing) and TOGGLE_MASTER_CUE are
//!   registered in [`register_continuous_commands`], right after the knob commands they toggle
//!   (Java registers them with the trigger commands and keeps the knob commands in fields).
//! - The touchstrip is not bound as a continuous control since the 7 bit value of a continuous
//!   command would lose the resolution of the pitch bend: the pitch bend of MIDI channel 1 is
//!   taken from the MIDI handler of the surface, which is wrapped for it. The continuous control
//!   `Touchstrip` exists for the touch binding; its `changed_observer` (never called by the
//!   framework since the control has no MIDI binding) is the hook of [`flush`] (former
//!   `touchstripCommand.updateValue`).
//! - The MPE pitch bend of the channels 2-16 (Push 3) is handled by the MIDI handler of the
//!   surface and not bound again.
//! - The aftertouch commands of the PLAY, PIANO, DRUM and DRUM64 views are created by the views.

pub mod continuous;
pub mod modes;
pub mod touchstrip;
pub mod trigger;

use std::cell::RefCell;
use std::rc::Rc;

use self::continuous::{ConfigurePitchbendCommand, MastertrackTouchCommand, PlayPositionKnobCommand, Push3EncoderCommand, PushMasterVolumeCommand, RasteredKnobCommand};
use self::touchstrip::TouchstripCommand;
use self::trigger::*;
use super::colors::*;
use super::surface::{self as push, *};
use super::view::sequencer_clip_key;
use crate::config::push::{self as config, LockState, PushVersion};
use crate::controller::push::setup::is_session_view;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_OFF, BUTTON_STATE_ON};
use crate::framework::command::{shared, ContinuousCommand, FnTrigger, SharedTrigger};
use crate::framework::commands::application::{DuplicateCommand, UndoCommand};
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::cursor::Direction;
use crate::framework::commands::edit::{CreateSceneCommand, DeleteCommand};
use crate::framework::commands::general::NopCommand;
use crate::framework::commands::knob_touch::KnobRowTouchModeCommand;
use crate::framework::commands::misc::{BrowserCommand, ConvertCommand, DoubleCommand, FootswitchCommand};
use crate::framework::commands::mode::{ButtonRowModeCommand, KnobRowModeCommand, ModeSelectCommand};
use crate::framework::commands::transport::PlayCommand;
use crate::framework::commands::transport_ext::TapTempoCommand;
use crate::framework::commands::transport_more::{LoadCommand, RecordCommand, SaveCommand};
use crate::framework::commands::view::ViewButtonCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Modes};
use crate::framework::surface::{ContinuousKind, LightSupplier};
use crate::midi::{BindType, ButtonEvent};

/// The note clips which the views use (`ModelSetup.note_clips`, former `getNoteClip (cols,
/// rows)` calls): the play/chords/drum 64 views and the sequencer (8x128), the poly sequencer,
/// the drum and the XoX drum views (32x128), the drum 4 view (16x128), the raindrops view
/// (1536x128) and the clip length view (0x0). The drum 8 view uses 8x128.
pub const NOTE_CLIPS: [(usize, usize); 5] = [(8, 128), (32, 128), (16, 128), (1536, 128), (0, 0)];

// ----------------------------------------------------------------------------------------------
// Light suppliers

/// The light of a button without a supplier (former `ButtonPressedSupplier` with the color IDs
/// `on` and `hi`).
fn pressed_light(id: ButtonId, on: &'static str, hi: &'static str) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| ctx.colors.color_index(if ctx.surface.is_pressed(id) { hi } else { on }))
}

/// A light with a boolean supplier and the color IDs for false and true.
fn bool_light(supplier: impl Fn(&Ctx) -> bool + 'static, on: &'static str, hi: &'static str) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| ctx.colors.color_index(if supplier(ctx) { hi } else { on }))
}

/// A light with a boolean supplier and the default color IDs (BUTTON_STATE_ON/HI).
fn state_light(supplier: impl Fn(&Ctx) -> bool + 'static) -> LightSupplier {
    bool_light(supplier, BUTTON_STATE_ON, BUTTON_STATE_HI)
}

/// A light with an integer supplier which selects one of the color IDs (BUTTON_STATE_OFF for a
/// negative state).
fn int_light(supplier: impl Fn(&Ctx) -> i32 + 'static, color_ids: &'static [&'static str]) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| {
        let state = supplier(ctx);
        let id = if state < 0 { BUTTON_STATE_OFF } else { color_ids.get(state as usize).copied().unwrap_or(BUTTON_STATE_OFF) };
        ctx.colors.color_index(id)
    })
}

/// Add a button bound to a CC on MIDI channel 1 (former `addButton`).
fn add_button(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, cc: i32, light: Option<LightSupplier>) {
    let light = light.unwrap_or_else(|| pressed_light(id, BUTTON_STATE_ON, BUTTON_STATE_HI));
    ctx.add_button(id, label, Some(command), BindType::Cc, 0, cc, Some(light));
}

/// Add a button with a closure as command.
fn add_fn_button(ctx: &mut Ctx, id: ButtonId, label: &str, cc: i32, light: Option<LightSupplier>, f: impl FnMut(&mut Ctx, ButtonEvent, i32) + 'static) {
    add_button(ctx, id, label, shared(FnTrigger(f)), cc, light);
}

// ----------------------------------------------------------------------------------------------
// Former helper methods of the setup

/// Former `AbstractControllerSetup.isRecordShifted`.
pub fn is_record_shifted(ctx: &Ctx) -> bool {
    let is_shift = ctx.surface.is_shift_pressed();
    let is_flip_record = ctx.config.is_flip_record();
    is_shift && !is_flip_record || !is_shift && is_flip_record
}

/// Former `AbstractControllerSetup.getModeColor`.
pub fn mode_color(ctx: &Ctx, id: ButtonId) -> i32 {
    ctx.mode_button_color(id)
}

/// True if the mode is one of the mix modes (former `Modes.isMixMode`).
fn is_mix_mode(mode: Option<Modes>) -> bool {
    mode.is_some_and(|m| (Modes::Track..=Modes::Send).contains(&m))
}

/// True if the mode is a device mode (former `Modes.isDeviceMode`).
fn is_device_mode(mode: Option<Modes>) -> bool {
    super::view::is_layer_mode(mode) || mode == Some(Modes::DeviceParams)
}

/// True if the mode is one of the master modes (former `Modes.isMasterMode`).
fn is_master_mode(mode: Option<Modes>) -> bool {
    matches!(mode, Some(Modes::Master | Modes::MasterTemp | Modes::Frame))
}

/// The state of the mute or solo button (former `getMuteState` and `getSoloState`).
fn mute_solo_state(ctx: &Ctx, is_solo: bool) -> bool {
    let lock = if is_solo { LockState::Solo } else { LockState::Mute };
    if push::lock_state(ctx) == lock {
        return !push::is_modern(ctx);
    }
    if ctx.modes.is_active(Modes::DeviceLayer) {
        return ctx.model.cursor_device().layer_bank().selected_item().is_some_and(|layer| if is_solo { layer.is_solo() } else { layer.is_mute() });
    }
    let track = if ctx.modes.is_active(Modes::Master) { ctx.model.master_track() } else { ctx.model.cursor_track() };
    if is_solo {
        track.is_solo()
    } else {
        track.is_mute()
    }
}

/// Former `getMuteState`.
pub fn mute_state(ctx: &Ctx) -> bool {
    mute_solo_state(ctx, false)
}

/// Former `getSoloState`.
pub fn solo_state(ctx: &Ctx) -> bool {
    mute_solo_state(ctx, true)
}

/// The LED state of the page buttons (former suppliers of PAGE_LEFT/PAGE_RIGHT).
fn page_state(ctx: &Ctx, is_left: bool) -> bool {
    if ctx.views.is_active(crate::framework::ids::Views::Session) {
        let tb = ctx.model.current_track_bank();
        return if is_left { tb.can_scroll_page_backwards() } else { tb.can_scroll_page_forwards() };
    }
    let Some(view) = ctx.active_view() else {
        return false;
    };
    let Ok(view) = view.try_borrow() else {
        return false;
    };
    let Some((cols, rows)) = sequencer_clip_key(&*view) else {
        return false;
    };
    ctx.model.note_clip_ref(cols, rows).is_some_and(|clip| clip.does_exist() && if is_left { clip.can_scroll_steps_backwards() } else { clip.can_scroll_steps_forwards() })
}

/// The LED state of the octave buttons (former `TransposeView.isOctaveUp/DownButtonOn`).
fn octave_state(ctx: &Ctx, is_up: bool) -> bool {
    let Some(view) = ctx.active_view() else {
        return false;
    };
    let Ok(view) = view.try_borrow() else {
        return false;
    };
    view.as_transpose_view().is_some_and(|t| if is_up { t.is_octave_up_button_on(ctx) } else { t.is_octave_down_button_on(ctx) })
}

// ----------------------------------------------------------------------------------------------
// Registration

/// Register all buttons (former `registerTriggerCommands`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    let version = push::version(ctx);
    let is_push3 = version == PushVersion::Version3;

    add_button(ctx, ButtonId::Play, "Play", shared(PlayCommand::new()), PUSH_BUTTON_PLAY, Some(bool_light(|ctx| ctx.model.transport().is_playing(), PUSH_BUTTON_STATE_PLAY_ON, PUSH_BUTTON_STATE_PLAY_HI)));

    add_button(
        ctx,
        ButtonId::Record,
        "Record",
        shared(RecordCommand::new()),
        PUSH_BUTTON_RECORD,
        Some(int_light(
            |ctx| {
                if is_record_shifted(ctx) {
                    return if ctx.model.is_launcher_overdub() { 3 } else { 2 };
                }
                i32::from(ctx.model.transport().is_recording())
            },
            &[PUSH_BUTTON_STATE_REC_ON, PUSH_BUTTON_STATE_REC_HI, PUSH_BUTTON_STATE_OVR_ON, PUSH_BUTTON_STATE_OVR_HI],
        )),
    );

    add_button(ctx, ButtonId::New, "New", shared(NewCommand::new()), if is_push3 { PUSH_3_BUTTON_NEW } else { PUSH_BUTTON_NEW }, None);
    add_button(ctx, ButtonId::FixedLength, "Fixed Length", shared(FixedLengthCommand), PUSH_BUTTON_FIXED_LENGTH, Some(state_light(|ctx| ctx.modes.is_active(Modes::Fixed))));
    add_button(ctx, ButtonId::Duplicate, "Duplicate", shared(DuplicateCommand::new()), PUSH_BUTTON_DUPLICATE, None);
    add_button(ctx, ButtonId::Quantize, "Quantize", shared(PushQuantizeCommand), PUSH_BUTTON_QUANTIZE, None);
    add_button(ctx, ButtonId::Delete, "Delete", shared(DeleteCommand::new()), PUSH_BUTTON_DELETE, None);
    add_button(ctx, ButtonId::Double, "Double Loop", shared(DoubleCommand::new(false)), PUSH_BUTTON_DOUBLE, None);
    add_button(
        ctx,
        ButtonId::Undo,
        "Undo",
        shared(UndoCommand::new()),
        PUSH_BUTTON_UNDO,
        Some(int_light(
            |ctx| {
                let application = ctx.model.application();
                let can = if ctx.surface.is_shift_pressed() { application.can_redo() } else { application.can_undo() };
                if !can {
                    return 0;
                }
                if ctx.surface.is_pressed(ButtonId::Undo) {
                    2
                } else {
                    1
                }
            },
            &[BUTTON_STATE_OFF, BUTTON_STATE_ON, BUTTON_STATE_HI],
        )),
    );

    add_button(
        ctx,
        ButtonId::Automation,
        "Automate",
        shared(PushAutomationCommand::default()),
        PUSH_BUTTON_AUTOMATION,
        Some(int_light(
            |ctx| {
                let transport = ctx.model.transport();
                if is_record_shifted(ctx) {
                    return if transport.is_writing_clip_launcher_automation() { 3 } else { 2 };
                }
                i32::from(transport.is_writing_arranger_automation())
            },
            &[PUSH_BUTTON_STATE_REC_ON, PUSH_BUTTON_STATE_REC_HI, PUSH_BUTTON_STATE_OVR_ON, PUSH_BUTTON_STATE_OVR_HI],
        )),
    );

    let is_push1 = version == PushVersion::Version1;
    add_button(
        ctx,
        ButtonId::Track,
        if is_push1 { "Track" } else { "Mix" },
        shared(TrackCommand),
        PUSH_BUTTON_TRACK,
        Some(state_light(move |ctx| if is_push1 { ctx.modes.is_active(Modes::Track) } else { is_mix_mode(ctx.modes.active_id()) })),
    );
    add_button(ctx, ButtonId::Device, "Device", shared(DeviceCommand), PUSH_BUTTON_DEVICE, Some(state_light(|ctx| is_device_mode(ctx.modes.active_id()))));
    add_button(ctx, ButtonId::Clip, "Clip", shared(ClipCommand), PUSH_BUTTON_CLIP, Some(state_light(|ctx| ctx.modes.is_active(Modes::Clip))));

    for i in 0..8 {
        let row1 = ButtonId::get(ButtonId::Row11, i);
        add_button(ctx, row1, &format!("Row 1: {}", i + 1), shared(ButtonRowModeCommand::new(0, i)), PUSH_BUTTON_ROW1_1 + i as i32, Some(Rc::new(move |ctx: &Ctx| mode_color(ctx, row1))));
        let row2 = ButtonId::get(ButtonId::Row21, i);
        add_button(ctx, row2, &format!("Row 2: {}", i + 1), shared(ButtonRowModeCommand::new(1, i)), PUSH_BUTTON_ROW2_1 + i as i32, Some(Rc::new(move |ctx: &Ctx| mode_color(ctx, row2))));
        let scene = ButtonId::get(ButtonId::Scene1, i);
        add_button(ctx, scene, &format!("Scene {}", i + 1), shared(ViewButtonCommand::new(scene)), PUSH_BUTTON_SCENE1 + 7 - i as i32, Some(Rc::new(move |ctx: &Ctx| ctx.button_color_from_active_view(scene))));
    }

    add_button(ctx, ButtonId::Shift, "Shift", shared(PushShiftCommand), PUSH_BUTTON_SHIFT, None);
    add_button(ctx, ButtonId::Select, "Select", shared(SelectCommand), PUSH_BUTTON_SELECT, None);
    add_button(ctx, ButtonId::TapTempo, "Tap Tempo", shared(TapTempoCommand::new()), PUSH_BUTTON_TAP, None);
    add_button(ctx, ButtonId::Metronome, "Metronome", shared(PushMetronomeCommand::default()), PUSH_BUTTON_METRONOME, Some(state_light(|ctx| ctx.model.transport().is_metronome_on())));
    add_button(
        ctx,
        ButtonId::Mastertrack,
        "Mastertrack",
        shared(MastertrackCommand::default()),
        PUSH_BUTTON_MASTER,
        Some(bool_light(|ctx| is_master_mode(ctx.modes.active_id()), PUSH_BUTTON_STATE_MASTER_ON, PUSH_BUTTON_STATE_MASTER_HI)),
    );
    add_button(ctx, ButtonId::PageLeft, "Page Left", shared(PageCommand::new(true)), PUSH_BUTTON_DEVICE_LEFT, Some(bool_light(|ctx| page_state(ctx, true), BUTTON_STATE_OFF, BUTTON_STATE_ON)));
    add_button(ctx, ButtonId::PageRight, "Page Right", shared(PageCommand::new(false)), PUSH_BUTTON_DEVICE_RIGHT, Some(bool_light(|ctx| page_state(ctx, false), BUTTON_STATE_OFF, BUTTON_STATE_ON)));

    add_button(ctx, ButtonId::Mute, "Mute", shared(MuteSoloCommand::mute()), PUSH_BUTTON_MUTE, Some(bool_light(mute_state, PUSH_BUTTON_STATE_MUTE_ON, PUSH_BUTTON_STATE_MUTE_HI)));
    add_button(ctx, ButtonId::Solo, "Solo", shared(MuteSoloCommand::solo()), PUSH_BUTTON_SOLO, Some(bool_light(solo_state, PUSH_BUTTON_STATE_SOLO_ON, PUSH_BUTTON_STATE_SOLO_HI)));
    add_button(ctx, ButtonId::Scales, "Scale", shared(ScalesCommand), PUSH_BUTTON_SCALES, Some(state_light(|ctx| ctx.modes.is_active(Modes::Scales))));
    add_button(ctx, ButtonId::Accent, "Accent", shared(AccentCommand::default()), PUSH_BUTTON_ACCENT, Some(state_light(|ctx| ctx.config.is_accent_active())));
    add_button(ctx, ButtonId::AddEffect, "Add Device", shared(PushAddEffectCommand), PUSH_BUTTON_ADD_EFFECT, None);
    add_button(ctx, ButtonId::AddTrack, "Add Track", shared(ModeSelectCommand::new(Modes::AddTrack)), if is_push3 { PUSH_BUTTON_ADD } else { PUSH_BUTTON_ADD_TRACK }, None);
    add_button(ctx, ButtonId::Note, "Note", shared(SelectPlayViewCommand), PUSH_BUTTON_NOTE, Some(state_light(|ctx| !ctx.views.active_id().is_some_and(is_session_view))));

    for (id, label, direction, cc) in [
        (ButtonId::ArrowDown, "Down", Direction::Down, PUSH_BUTTON_DOWN),
        (ButtonId::ArrowUp, "Up", Direction::Up, PUSH_BUTTON_UP),
        (ButtonId::ArrowLeft, "Left", Direction::Left, PUSH_BUTTON_LEFT),
        (ButtonId::ArrowRight, "Right", Direction::Right, PUSH_BUTTON_RIGHT),
    ] {
        let light = bool_light(move |ctx| PushCursorCommand::new(direction).can_scroll(ctx), BUTTON_STATE_OFF, BUTTON_STATE_ON);
        add_button(ctx, id, label, shared(PushCursorCommand::new(direction)), cc, Some(light));
    }

    add_button(ctx, ButtonId::OctaveDown, "Octave Down", shared(OctaveCommand::new(false)), PUSH_BUTTON_OCTAVE_DOWN, Some(bool_light(|ctx| octave_state(ctx, false), BUTTON_STATE_OFF, BUTTON_STATE_ON)));
    add_button(ctx, ButtonId::OctaveUp, "Octave Up", shared(OctaveCommand::new(true)), PUSH_BUTTON_OCTAVE_UP, Some(bool_light(|ctx| octave_state(ctx, true), BUTTON_STATE_OFF, BUTTON_STATE_ON)));

    add_button(
        ctx,
        ButtonId::StopClip,
        "Stop Clip",
        shared(ClipStopCommand),
        PUSH_BUTTON_STOP_CLIP,
        Some(bool_light(|ctx| ctx.surface.is_pressed(ButtonId::StopClip), PUSH_BUTTON_STATE_STOP_ON, PUSH_BUTTON_STATE_STOP_HI)),
    );
    add_button(ctx, ButtonId::Session, "Session", shared(SelectSessionViewCommand::default()), PUSH_BUTTON_SESSION, Some(state_light(|ctx| ctx.views.active_id().is_some_and(is_session_view))));
    add_button(
        ctx,
        ButtonId::Repeat,
        "Repeat",
        shared(crate::controller::maschine::core::command::note_repeat::FillModeNoteRepeatCommand::new(true)),
        PUSH_BUTTON_REPEAT,
        Some(state_light(|ctx| ctx.config.is_note_repeat_active())),
    );
    add_button(ctx, ButtonId::Footswitch2, "Foot Controller", shared(FootswitchCommand::new(0)), PUSH_FOOTSWITCH2, None);

    if is_push1 {
        add_button(ctx, ButtonId::Volume, "Volume", shared(VolumeCommand), PUSH_BUTTON_VOLUME, Some(state_light(|ctx| ctx.modes.is_any_active(&[Modes::Volume, Modes::Crossfader]))));
        add_button(
            ctx,
            ButtonId::PanSend,
            "Pan/Send",
            shared(PanSendCommand),
            PUSH_BUTTON_PAN_SEND,
            Some(state_light(|ctx| ctx.modes.is_active(Modes::Pan) || ctx.modes.active_id().is_some_and(|m| (Modes::Send1..=Modes::Send8).contains(&m)))),
        );
        add_button(ctx, ButtonId::Setup, "User", shared(SetupCommand::new(version)), PUSH_BUTTON_USER_MODE, Some(state_light(|ctx| ctx.modes.is_any_active(&[Modes::User, Modes::Configuration]))));
    } else {
        add_button(ctx, ButtonId::Layout, "Layout", shared(LayoutCommand), PUSH_BUTTON_LAYOUT, None);
        add_button(
            ctx,
            ButtonId::Setup,
            "Setup",
            shared(SetupCommand::new(version)),
            PUSH_BUTTON_SETUP,
            Some(state_light(|ctx| ctx.modes.is_any_active(&[Modes::Setup, Modes::Configuration, Modes::Info, Modes::Audio]))),
        );
        add_button(
            ctx,
            ButtonId::Convert,
            "Convert",
            shared(ConvertCommand::new()),
            PUSH_BUTTON_CONVERT,
            Some(int_light(
                |ctx| {
                    if !ctx.model.can_convert_clip() {
                        return 0;
                    }
                    if ctx.surface.is_pressed(ButtonId::Convert) {
                        2
                    } else {
                        1
                    }
                },
                &[BUTTON_STATE_OFF, BUTTON_STATE_ON, BUTTON_STATE_HI],
            )),
        );
        add_button(ctx, ButtonId::User, "User", shared(ModeSelectCommand::new(Modes::User)), PUSH_BUTTON_USER_MODE, Some(state_light(|ctx| ctx.modes.is_active(Modes::User))));
    }

    if is_push3 {
        add_button(
            ctx,
            ButtonId::InsertScene,
            "Create Scene",
            shared(CreateSceneCommand::new()),
            PUSH_CAPTURE_MIDI,
            Some(pressed_light(ButtonId::InsertScene, PUSH_BUTTON_STATE_INSERT_SCENE_ON, PUSH_BUTTON_STATE_INSERT_SCENE_HI)),
        );
        add_button(ctx, ButtonId::Load, "Load", shared(LoadCommand), PUSH_BUTTON_FILES, None);
        add_button(ctx, ButtonId::Save, "Save", shared(SaveCommand), PUSH_BUTTON_SAVE, Some(state_light(|ctx| ctx.model.project().is_dirty())));
        add_fn_button(ctx, ButtonId::Help, "Help", PUSH_BUTTON_HELP, None, |ctx, event, _| {
            if event == ButtonEvent::Down {
                ctx.model.application_mut().show_help();
            }
        });
        add_button(
            ctx,
            ButtonId::LockMode,
            "Lock",
            shared(NopCommand),
            PUSH_BUTTON_LOCK,
            Some(bool_light(|ctx| push::lock_state(ctx) != LockState::Off, PUSH_BUTTON_STATE_LOCK_ON, PUSH_BUTTON_STATE_LOCK_HI)),
        );
        // SWING and TOGGLE_MASTER_CUE: see register_continuous_commands
        add_fn_button(ctx, ButtonId::ToggleClipView, "Toggle Scene View", PUSH_BUTTON_SESSION_DISPLAY, Some(state_light(|ctx| config::is_scenes_clip_view_selected(&ctx.config))), |ctx, event, _| {
            if event == ButtonEvent::Down {
                let mode = if ctx.modes.is_active(Modes::Session) { Modes::Markers } else { Modes::Session };
                ctx.set_active_mode(mode);
            }
        });
    }

    add_button(ctx, ButtonId::Browse, "Browse", shared(BrowserCommand::new()), if is_push3 { PUSH_BUTTON_HOT_SWAP } else { PUSH_BUTTON_BROWSE }, Some(state_light(|ctx| ctx.modes.is_active(Modes::Browser))));
}

/// Add a relative knob bound to a CC (former `addRelativeKnob`).
fn add_relative_knob(ctx: &mut Ctx, id: ContinuousId, label: &str, command: Rc<RefCell<dyn ContinuousCommand>>, cc: i32) {
    ctx.add_continuous(id, label, ContinuousKind::RelativeKnob, command, BindType::Cc, 0, cc);
}

/// Bind the touch of a control to a note on MIDI channel 1 (former `bindTouch`).
fn bind_touch(ctx: &mut Ctx, id: ContinuousId, command: SharedTrigger, note: i32) {
    if let Some(control) = ctx.surface.continuous_mut(id) {
        control.touch_command = Some(command);
    }
    ctx.surface.input.bind_touch(id, BindType::Note, 0, note);
}

/// Register all continuous controls (former `registerContinuousCommands`).
pub fn register_continuous_commands(ctx: &mut Ctx) {
    for i in 0..8 {
        let id = ContinuousId::get(ContinuousId::Knob1, i);
        add_relative_knob(ctx, id, &format!("Knob {i}"), Rc::new(RefCell::new(KnobRowModeCommand::new(i))), PUSH_KNOB1 + i as i32);
        bind_touch(ctx, id, shared(KnobRowTouchModeCommand::new(i)), PUSH_KNOB1_TOUCH + i as i32);
    }

    let is_push3 = push::version(ctx) == PushVersion::Version3;

    let master_volume = Rc::new(RefCell::new(PushMasterVolumeCommand::default()));
    add_relative_knob(ctx, ContinuousId::MasterKnob, "Master", master_volume.clone(), PUSH_KNOB9);
    bind_touch(ctx, ContinuousId::MasterKnob, shared(MastertrackTouchCommand), PUSH_KNOB9_TOUCH);

    let tempo_shuffle = Rc::new(RefCell::new(RasteredKnobCommand::default()));
    add_relative_knob(ctx, ContinuousId::Tempo, "Tempo", tempo_shuffle.clone(), PUSH_SMALL_KNOB1);
    bind_touch(ctx, ContinuousId::Tempo, tempo_shuffle.clone(), PUSH_SMALL_KNOB1_TOUCH);

    if is_push3 {
        // Former trigger commands SWING and TOGGLE_MASTER_CUE of the Push 3
        add_fn_button(ctx, ButtonId::Swing, "Toggle Tempo/Swing", PUSH_SMALL_KNOB1_PRESS, None, move |ctx, event, _| {
            if event == ButtonEvent::Down {
                if let Ok(mut command) = tempo_shuffle.try_borrow_mut() {
                    command.toggle_mode();
                    command.notify_mode(ctx);
                }
            }
        });
        add_fn_button(ctx, ButtonId::ToggleMasterCue, "Toggle Master/Cue", PUSH_BUTTON_TOGGLE_MASTER_CUE_VOLUME, None, move |ctx, event, _| {
            if event == ButtonEvent::Down {
                if let Ok(mut command) = master_volume.try_borrow_mut() {
                    command.toggle_master_volume_and_cue();
                    command.notify_mode(ctx);
                }
            }
        });

        let encoder = Rc::new(RefCell::new(Push3EncoderCommand));
        add_relative_knob(ctx, ContinuousId::PlayPosition, "Encoder", encoder.clone(), PUSH_KNOB_ENCODER);
        add_button(ctx, ButtonId::EncoderPush, "Enc.Press", encoder, PUSH_BUTTON_ENCODER, None);
        add_fn_button(ctx, ButtonId::Left, "Left", PUSH_ENCODER_LEFT, None, |ctx, event, _| Push3EncoderCommand::left(ctx, event));
        add_fn_button(ctx, ButtonId::Right, "Right", PUSH_ENCODER_RIGHT, None, |ctx, event, _| Push3EncoderCommand::right(ctx, event));
        add_fn_button(ctx, ButtonId::ArrowCenter, "Center", PUSH_BUTTON_CURSOR_CENTER, None, |ctx, event, _| Push3EncoderCommand::center(ctx, event));
        // The MPE pitch bend of the channels 2-16 is handled by the MIDI handler of the surface
    } else {
        let play_position = Rc::new(RefCell::new(PlayPositionKnobCommand));
        add_relative_knob(ctx, ContinuousId::PlayPosition, "Play Position", play_position.clone(), PUSH_SMALL_KNOB2);
        bind_touch(ctx, ContinuousId::PlayPosition, play_position, PUSH_SMALL_KNOB2_TOUCH);
    }

    // The aftertouch commands of PLAY, PIANO, DRUM and DRUM64 are part of the views

    register_touchstrip(ctx);
}

/// The touchstrip: the pitch bend of MIDI channel 1 from the wrapped MIDI handler, the touch and
/// the update of the ribbon LEDs at the flush.
fn register_touchstrip(ctx: &mut Ctx) {
    let touchstrip = Rc::new(RefCell::new(TouchstripCommand::default()));
    {
        let control = ctx.surface.create_continuous(ContinuousId::Touchstrip, "Touchstrip", ContinuousKind::Fader);
        let flush_touchstrip = touchstrip.clone();
        control.changed_observer = Some(Rc::new(move |ctx: &mut Ctx| {
            if let Ok(command) = flush_touchstrip.try_borrow() {
                command.update_value(ctx);
            }
        }));
    }
    bind_touch(ctx, ContinuousId::Touchstrip, shared(ConfigurePitchbendCommand), PUSH_RIBBON_TOUCH);

    let previous = ctx.midi_handler.clone();
    ctx.midi_handler = Some(Rc::new(move |ctx: &mut Ctx, status: i32, data1: i32, data2: i32| {
        if status == crate::midi::CMD_PITCHBEND {
            if let Ok(mut command) = touchstrip.try_borrow_mut() {
                command.on_pitchbend(ctx, data1, data2);
            }
            return;
        }
        if let Some(previous) = &previous {
            previous(ctx, status, data1, data2);
        }
    }));
}

/// Called at the end of each flush (former `PushControllerSetup.flush`:
/// `touchstripCommand.updateValue`).
pub fn flush(ctx: &mut Ctx) {
    let observer = ctx.surface.continuous(ContinuousId::Touchstrip).and_then(|c| c.changed_observer.clone());
    if let Some(observer) = observer {
        observer(ctx);
    }
}
