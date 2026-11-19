// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Push 1, 2 and 3 (former `PushControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The Push 2/3 display is rendered and its frame handed to the host (USB bulk transfer) in
//!   `flush`, after the active mode added its components; unchanged frames are resent once per
//!   second only (Java renders on changes but sends the image at every flush), see
//!   [`super::usb_display`].
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.

use super::surface::{self as push, PushState};
use super::{command, mode, sim_values, view};
use crate::backend::Sender;
use crate::config::push::{self as config, LockState, PushVersion};
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::ctx::Ctx;
use crate::framework::graphics::{GraphicDisplay, GraphicsDimensions};
use crate::framework::grid::PadGrid;
use crate::framework::ids::{Modes, Views};
use crate::framework::setup_helpers::{self, BrowserObserver};
use crate::framework::surface::Surface;
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{DeviceInquiry, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The note input filters of the Push 1 and 2 (former `MIDI_FILTERS_CLASSIC`).
const MIDI_FILTERS_CLASSIC: [&str; 3] = [
    // Note off - channel 1
    "80????",
    // Note on - channel 1
    "90????",
    // Sustain pedal - channel 1
    "B040??",
];

/// The note input filters of the Push 3 (former `MIDI_FILTER_MPE`): the pitch bend of the MPE
/// channels is handled in the MIDI handler to support scales.
fn midi_filters_mpe() -> Vec<String> {
    let mut filters = vec!["8?????".to_string(), "9?????".to_string()];
    // Channel Aftertouch (MPE) - channel 1-15
    filters.extend((1..16).map(|i| format!("D{i:X}????")));
    // Brightness (MPE) - channel 1-15
    filters.extend((1..16).map(|i| format!("B{i:X}4A??")));
    // Sustain pedal - channel 1
    filters.push("B040??".to_string());
    filters
}

/// The name of the host on the shutdown display (Java: `IHost.getName`).
const HOST_NAME: &str = "the DAW";

/// The width of the Push 2/3 display.
pub const DISPLAY_WIDTH: i32 = 960;
/// The height of the Push 2/3 display.
pub const DISPLAY_HEIGHT: i32 = 160;

/// Create the scales of the Push (8x8 pads, starting at note 36).
pub fn create_scales(value_changer: &ValueChanger) -> Scales {
    Scales::new(value_changer, 36, 100, 8, 8)
}

/// Create a Push instance.
pub fn create(version: PushVersion, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let colors = super::colors::create_color_manager(version);
    let value_changer = ValueChanger::twos_complement(1024, 10);
    let scales = create_scales(&value_changer);
    let upper_bound = value_changer.upper_bound();

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.enable_drum64_device();
    if version.is_modern() {
        model_setup.num_filter_column_entries = 48;
        model_setup.set_num_results(48);
        model_setup.set_num_sends(4);
    }
    model_setup.set_num_markers(8);
    model_setup.set_has_flat_track_list(configuration.is_track_navigation_flat());
    model_setup.set_has_full_flat_track_list(configuration.are_master_tracks_included());
    model_setup.wants_clip_launcher_navigator = true;
    model_setup.wants_focused_parameter = true;
    model_setup.note_clips = command::NOTE_CLIPS.to_vec();
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.ensure_scene_bank(64);
    model.track_bank_mut().set_indication(true);

    // createSurface
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = if version == PushVersion::Version3 {
        let filters = midi_filters_mpe();
        let filters: Vec<&str> = filters.iter().map(String::as_str).collect();
        MidiInput::new(sender.clone(), Some(ports.input), &filters)?
    } else {
        MidiInput::new(sender.clone(), Some(ports.input), &MIDI_FILTERS_CLASSIC)?
    };
    let grid = PadGrid::new(&colors, 8, 8, push::PAD_START_NOTE);
    let surface = Surface::new(output.clone(), input, Some(grid));

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    ctx.ext = Box::new(PushState::new(version));
    // Former notifyViewChange = false
    ctx.notify_view_change = false;
    if version == PushVersion::Version1 {
        ctx.text_display = Some(super::display1::create(output));
    } else {
        let display = GraphicDisplay::new(config::graphics_configuration(&ctx.config), GraphicsDimensions::new(DISPLAY_WIDTH, DISPLAY_HEIGHT, upper_bound));
        ctx.graphics_display = Some(display);
    }
    ctx.modes.set_default_id(Modes::Track);
    ctx.midi_handler = Some(std::rc::Rc::new(push::handle_midi));
    ctx.sysex_handler = Some(std::rc::Rc::new(push::handle_sysex));
    ctx.create_pads();

    Ok(InstanceParts { ctx, setup: Box::new(PushSetup::new(version)) })
}

/// The Push controller setup.
pub struct PushSetup {
    version: PushVersion,
    browser_observer: BrowserObserver,
    last_view: Option<Views>,
}

impl PushSetup {
    pub fn new(version: PushVersion) -> Self {
        PushSetup { version, browser_observer: BrowserObserver::default(), last_view: None }
    }
}

impl ControllerSetup for PushSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        mode::register_modes(ctx);
        view::register_views(ctx);
        command::register_trigger_commands(ctx);
        command::register_continuous_commands(ctx);
        // The initial state of the settings which are observed
        config::apply_session_view(&mut ctx.config);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        if ctx.config.should_start_with_session_view() {
            ctx.set_active_view(Views::Session);
        } else {
            setup_helpers::recall_last_view(ctx);
        }

        // This can happen if there is no track in the project. It is required to have a view
        // otherwise the mode is not drawn
        if ctx.views.active_id().is_none() {
            ctx.set_active_view(Views::Play);
        }

        if self.version != PushVersion::Version3 {
            push::send_pressure_mode(ctx, true);
        }
        ctx.surface.output.send_sysex(&DeviceInquiry::QUERY);

        if self.version == PushVersion::Version1 {
            push::set_lock_state(ctx, LockState::Mute);
        } else {
            push::update_color_palette(ctx);
        }

        // Send the initial device configuration (Java: the setting observers are called
        // when they are registered)
        match self.version {
            PushVersion::Version1 => push::send_pad_sensitivity_push1(ctx),
            PushVersion::Version2 => {
                push::send_display_brightness(ctx);
                push::send_led_brightness(ctx);
                push::send_pad_sensitivity_push2(ctx);
            }
            PushVersion::Version3 => {
                push::send_display_brightness(ctx);
                push::send_led_brightness(ctx);
                push::send_pad_sensitivity_push3(ctx);
                push::update_mpe_pitchbend_range(ctx);
                let per_pad = config::is_per_pad_pitchbend(&ctx.config);
                push::send_per_pad_pitchbend_active(ctx, per_pad);
                let location = config::in_tune_location(&ctx.config);
                push::send_in_tune_location(ctx, location);
                let width = config::in_tune_width(&ctx.config);
                push::send_in_tune_width(ctx, width);
                let height = config::in_tune_slide_height(&ctx.config);
                push::send_slide_height(ctx, height);
                push::send_pedals(ctx);
                push::send_preamp1_type(ctx);
                push::send_preamp2_type(ctx);
                push::send_preamp1_gain(ctx);
                push::send_preamp2_gain(ctx);
                push::send_output_configuration(ctx);
            }
        }
        self.update_ribbon_mode(ctx);
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
        command::flush(ctx);
        if let Some(display) = ctx.text_display.as_mut() {
            display.flush();
        }
        send_display_frame(ctx);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        // Model events (former track bank and master track selection observers and the
        // observers of the views)
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for event in &events {
            if let ModelEvent::TrackSelectionChanged { index, is_selected } = event {
                if *index < 0 {
                    // Former master track selection observer
                    if *is_selected {
                        ctx.set_active_mode(Modes::Master);
                    } else if ctx.modes.is_active(Modes::Master) {
                        ctx.restore_mode();
                    }
                } else {
                    setup_helpers::handle_track_change(ctx, *is_selected);
                }
            }
        }
        mode::handle_model_events(ctx, &events);

        let changed = ctx.config.take_changed_settings();
        for id in &changed {
            self.handle_setting_change(ctx, *id);
        }
        ctx.dispatch_setting_changes_to_views(&changed);

        self.browser_observer.check_mode(ctx, Modes::Browser);

        // Former onViewChange
        let active_view = ctx.views.active_id();
        if active_view != self.last_view {
            self.last_view = active_view;
            self.on_view_change(ctx);
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        push::shutdown(ctx);
        // Java: the name of the host
        let host_name = HOST_NAME;
        if let Some(display) = ctx.text_display.as_mut() {
            super::display1::shutdown(display, host_name);
        }
        if let Some(display) = ctx.graphics_display.as_mut() {
            display.set_message(3, &format!("Please start {host_name} to play..."));
        }
        if ctx.graphics_display.is_some() {
            send_display_frame_forced(ctx);
        }
        ctx.turn_off_lights();
    }
}

impl PushSetup {
    /// React on a changed setting (former `createObservers`).
    fn handle_setting_change(&mut self, ctx: &mut Ctx, id: SettingId) {
        if setup_helpers::handle_scale_setting(ctx, id) || setup_helpers::handle_deactivated_items_setting(ctx, id) || setup_helpers::handle_note_repeat_setting(ctx, id) {
            if id == SettingId::NOTEREPEAT_ACTIVE {
                self.update_ribbon_mode(ctx);
            }
            return;
        }
        match self.version {
            PushVersion::Version1 => {
                if id == config::VELOCITY_CURVE || id == config::PAD_PUSH1_THRESHOLD {
                    push::send_pad_sensitivity_push1(ctx);
                }
            }
            PushVersion::Version2 => match id {
                config::DISPLAY_BRIGHTNESS => push::send_display_brightness(ctx),
                config::LED_BRIGHTNESS => push::send_led_brightness(ctx),
                config::PAD_PUSH2_SENSITIVITY | config::PAD_PUSH2_GAIN | config::PAD_PUSH2_DYNAMICS => push::send_pad_sensitivity_push2(ctx),
                _ => {}
            },
            PushVersion::Version3 => match id {
                config::DISPLAY_BRIGHTNESS => push::send_display_brightness(ctx),
                config::LED_BRIGHTNESS => push::send_led_brightness(ctx),
                config::PAD_PUSH3_THRESHOLD | config::PAD_PUSH3_DRIVE | config::PAD_PUSH3_COMPAND | config::PAD_PUSH3_RANGE => push::send_pad_sensitivity_push3(ctx),
                SettingId::ENABLED_MPE_ZONES => push::update_mpe(ctx),
                SettingId::MPE_PITCHBEND_RANGE => push::update_mpe_pitchbend_range(ctx),
                config::PER_PAD_PITCHBEND => {
                    let enable = config::is_per_pad_pitchbend(&ctx.config);
                    push::send_per_pad_pitchbend_active(ctx, enable);
                }
                config::IN_TUNE_LOCATION => {
                    let value = config::in_tune_location(&ctx.config);
                    push::send_in_tune_location(ctx, value);
                }
                config::IN_TUNE_WIDTH => {
                    let value = config::in_tune_width(&ctx.config);
                    push::send_in_tune_width(ctx, value);
                }
                config::IN_TUNE_SLIDE_HEIGHT => {
                    let value = config::in_tune_slide_height(&ctx.config);
                    push::send_slide_height(ctx, value);
                }
                config::PEDAL_1 | config::PEDAL_2 => push::send_pedals(ctx),
                config::PREAMP_TYPE_1 => push::send_preamp1_type(ctx),
                config::PREAMP_TYPE_2 => push::send_preamp2_type(ctx),
                config::PREAMP_GAIN_1 => push::send_preamp1_gain(ctx),
                config::PREAMP_GAIN_2 => push::send_preamp2_gain(ctx),
                config::AUDIO_OUTPUTS => push::send_output_configuration(ctx),
                _ => {}
            },
        }

        match id {
            config::RIBBON_MODE | config::RIBBON_MODE_NOTE_REPEAT => self.update_ribbon_mode(ctx),
            config::DEBUG_MODE => {
                let debug_mode = config::mixer_mode(&ctx.config);
                if ctx.modes.contains(debug_mode) {
                    ctx.set_active_mode(debug_mode);
                } else {
                    ctx.host.error(&format!("Mode {} not registered.", config::java_mode_name(debug_mode)));
                }
            }
            config::SESSION_VIEW => {
                config::apply_session_view(&mut ctx.config);
                let is_session = ctx.views.active_id().is_some_and(is_session_view);
                if is_session {
                    let view = if config::is_scenes_clip_view_selected(&ctx.config) { Views::ScenePlay } else { Views::Session };
                    ctx.set_active_view(view);
                }
            }
            config::COLOR_RESET => config::reset_colors(&mut ctx.config),
            id if config::is_color_setting(id) => {
                let configuration = config::graphics_configuration(&ctx.config);
                if let Some(display) = ctx.graphics_display.as_mut() {
                    display.set_configuration(configuration);
                }
                // Former redraw
                ctx.update_active_mode_display();
            }
            _ => {}
        }
    }

    /// Former `onViewChange`.
    fn on_view_change(&mut self, ctx: &mut Ctx) {
        // Update ribbon mode
        if ctx.views.is_active(Views::Session) {
            push::set_ribbon_mode(ctx, push::PUSH_RIBBON_PAN);
        } else {
            self.update_ribbon_mode(ctx);
        }
        push::update_mpe(ctx);
        if let Some(display) = ctx.text_display.as_mut() {
            display.cancel_notification();
        }
        if let Some(display) = ctx.graphics_display.as_mut() {
            display.cancel_notification();
        }
    }

    /// Former `updateRibbonMode`.
    fn update_ribbon_mode(&mut self, ctx: &mut Ctx) {
        push::set_ribbon_value(ctx, 0);
        let ribbon_note_repeat = config::ribbon_note_repeat(&ctx.config);
        if ctx.config.is_note_repeat_active() && ribbon_note_repeat > config::NOTE_REPEAT_OFF {
            push::set_ribbon_mode(ctx, push::PUSH_RIBBON_DISCRETE);
            return;
        }
        let ribbon_mode = config::ribbon_mode(&ctx.config);
        if ribbon_mode == config::RIBBON_MODE_CC || ribbon_mode == config::RIBBON_MODE_FADER || ribbon_mode == config::RIBBON_MODE_LAST_TOUCHED {
            push::set_ribbon_mode(ctx, push::PUSH_RIBBON_VOLUME);
        } else {
            push::set_ribbon_mode(ctx, push::PUSH_RIBBON_PITCHBEND);
        }
    }
}

/// True for the session views (former `Views.isSessionView`).
pub fn is_session_view(view: Views) -> bool {
    matches!(view, Views::Session | Views::ScenePlay)
}

/// Render the graphics display (if the active mode added new components) and hand the frame
/// to the host if one is due (former `Push2Display.send`).
fn send_display_frame(ctx: &mut Ctx) {
    let now = std::time::Instant::now();
    if ctx.graphics_display.is_none() {
        return;
    }
    // The encoder state for the simulator, from the components before the render takes them
    let sender = ctx.sender.clone();
    let encoder_states = sender.simulator_data_enabled().then(|| simulator_encoder_states(ctx));
    let Some(display) = ctx.graphics_display.as_mut() else {
        return;
    };
    display.render(now);
    let changed = display.has_changed();
    let image = display.image().clone();
    let state = ctx.ext_mut::<PushState>();
    if let Some(frame) = state.usb_display.frame(&image, changed, now) {
        sender.usb_bulk_out(0, super::usb_display::ENDPOINT, frame);
    }
    if let Some(message) = encoder_states.as_deref().and_then(|states| state.sim_values.update(states)) {
        sender.simulator_data_out(message);
    }
}

/// The state of the encoders for the simulator side channel ([`super::sim_values`]): the 8
/// encoders from the columns of the display, the master volume, the play position and the tempo
/// from the model.
pub fn simulator_encoder_states(ctx: &Ctx) -> Vec<sim_values::EncoderState> {
    use crate::framework::ids::ContinuousId;
    use sim_values::EncoderState;
    let Some(display) = ctx.graphics_display.as_ref() else {
        return Vec::new();
    };
    let max = display.dimensions().max_parameter_value - 1;
    let mut states = sim_values::encoder_states(display.components(), max);
    let touched = |id: ContinuousId| ctx.surface.continuous(id).is_some_and(|c| c.is_touched);
    let master = ctx.model.master_track();
    states.push(EncoderState::value(sim_values::MASTER, "Master", master.volume(), max, &master.volume_str(), touched(ContinuousId::MasterKnob)));
    let transport = ctx.model.transport();
    if ctx.surface.continuous(ContinuousId::PlayPosition).is_some() && push::version(ctx) != PushVersion::Version3 {
        states.push(EncoderState::text(sim_values::POSITION, "Position", transport.position_text(), touched(ContinuousId::PlayPosition)));
    }
    states.push(EncoderState::text(sim_values::TEMPO, "Tempo", &crate::daw::transport::format_tempo(transport.tempo()), touched(ContinuousId::Tempo)));
    states
}

/// Send the current display without the rate limit (on shutdown).
fn send_display_frame_forced(ctx: &mut Ctx) {
    let now = std::time::Instant::now();
    let Some(display) = ctx.graphics_display.as_mut() else {
        return;
    };
    display.force_render();
    display.render(now);
    let image = display.image().clone();
    let mut frame = Vec::new();
    super::usb_display::encode_frame(&image, &mut frame);
    ctx.sender.usb_bulk_out(0, super::usb_display::ENDPOINT, &frame);
}
