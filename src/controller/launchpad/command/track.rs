// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The mode selection buttons of the Launchpad (former `AbstractTrackCommand`, `MuteCommand`,
//! `PanCommand`, `RecordArmCommand`, `SendsCommand`, `SoloCommand`, `StopClipCommand`,
//! `TrackCommand`, `TrackSelectCommand` and `VolumeCommand`).
//!
//! All of them are one [`TrackButtonCommand`] which is created with the function of the button.

use crate::controller::launchpad::surface as lp;
use crate::framework::command::TriggerCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, Views};
use crate::midi::ButtonEvent;

/// The function of a mode selection button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackButton {
    /// Record arm mode, undo with Shift on the Pro models.
    RecordArm,
    /// Track select mode.
    TrackSelect,
    /// Mute mode, redo with Shift on the Pro models.
    Mute,
    /// Solo mode, toggles the metronome with Shift on the Pro models.
    Solo,
    /// Volume view.
    Volume,
    /// Panorama view.
    Pan,
    /// Sends view, tap tempo with Shift on the Pro models.
    Sends,
    /// Stop clip mode, the shuffle view with Shift on the Pro models.
    StopClip,
    /// Former `TrackCommand` (not used by the setup): mute mode.
    Track,
}

/// The state and the functions of the former `AbstractTrackCommand`.
#[derive(Default)]
pub struct AbstractTrackCommand {
    first_row_used: bool,
    was_session: bool,
    temporary_view: bool,
}

impl AbstractTrackCommand {
    /// Toggle the mode of the first button row. If the button is held long, the mode is only
    /// active as long as the button is pressed.
    pub fn on_mode_button(&mut self, ctx: &mut Ctx, event: ButtonEvent, control_mode: Modes, notification: &str) {
        match event {
            ButtonEvent::Down => {
                self.first_row_used = false;
                if ctx.modes.is_active(control_mode) {
                    ctx.set_active_mode(Modes::Dummy);
                    return;
                }
                ctx.set_active_mode(control_mode);
                self.was_session = ctx.views.is_active(Views::Session);
                if !self.was_session {
                    ctx.set_active_view(Views::Session);
                }
                ctx.notify(notification);

                // Prevent overriding active view restored from track change!
                if ctx.surface.is_pressed(ButtonId::Track) {
                    self.was_session = true;
                }
            }
            ButtonEvent::Long => self.first_row_used = true,
            ButtonEvent::Up => {
                if self.first_row_used {
                    ctx.set_active_mode(Modes::Dummy);
                    if !self.was_session {
                        ctx.restore_view();
                    }
                }
            }
        }
    }

    /// Toggle a fader view. If the button is held long, the view is only active as long as the
    /// button is pressed.
    pub fn on_fader_mode_button(&mut self, ctx: &mut Ctx, event: ButtonEvent, view: Views, notification: &str) {
        match event {
            ButtonEvent::Down => {
                if ctx.views.is_active(view) {
                    ctx.set_active_view(Views::Session);
                    return;
                }
                self.temporary_view = false;
                ctx.set_active_mode(Modes::Dummy);
                ctx.set_active_view(view);
                ctx.notify(notification);
            }
            ButtonEvent::Long => self.temporary_view = true,
            ButtonEvent::Up => {
                if self.temporary_view {
                    ctx.restore_view();
                }
            }
        }
    }
}

/// A mode selection button of the Launchpad.
pub struct TrackButtonCommand {
    function: TrackButton,
    base: AbstractTrackCommand,
}

impl TrackButtonCommand {
    pub fn new(function: TrackButton) -> Self {
        TrackButtonCommand { function, base: AbstractTrackCommand::default() }
    }

    /// Execute the Shift function of the Pro models. Returns false if the button has none.
    fn execute_pro_shifted(&self, ctx: &mut Ctx, event: ButtonEvent) -> bool {
        match self.function {
            TrackButton::RecordArm => {
                if event == ButtonEvent::Down {
                    ctx.model.application_mut().undo();
                }
            }
            TrackButton::Mute => {
                if event == ButtonEvent::Down {
                    ctx.model.application_mut().redo();
                }
            }
            TrackButton::Solo => {
                if event == ButtonEvent::Down {
                    ctx.model.transport_mut().toggle_metronome();
                }
            }
            TrackButton::Sends => {
                if event == ButtonEvent::Down {
                    ctx.model.transport_mut().tap_tempo();
                }
            }
            TrackButton::StopClip => {
                if event == ButtonEvent::Down {
                    if ctx.views.is_active(Views::Shift) {
                        ctx.restore_view();
                    }
                    ctx.set_active_view(Views::Shuffle);
                }
            }
            TrackButton::TrackSelect | TrackButton::Volume | TrackButton::Pan | TrackButton::Track => return false,
        }
        true
    }
}

impl TriggerCommand for TrackButtonCommand {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, _velocity: i32) {
        if lp::is_pro(ctx) && ctx.surface.is_shift_pressed() && self.execute_pro_shifted(ctx, event) {
            return;
        }

        match self.function {
            TrackButton::RecordArm => self.base.on_mode_button(ctx, event, Modes::RecArm, "Record Arm"),
            TrackButton::TrackSelect => self.base.on_mode_button(ctx, event, Modes::TrackSelect, "Track Select"),
            TrackButton::Mute | TrackButton::Track => self.base.on_mode_button(ctx, event, Modes::Mute, "Mute"),
            TrackButton::Solo => self.base.on_mode_button(ctx, event, Modes::Solo, "Solo"),
            TrackButton::StopClip => self.base.on_mode_button(ctx, event, Modes::StopClip, "Stop Clip"),
            TrackButton::Volume => self.base.on_fader_mode_button(ctx, event, Views::TrackVolume, "Volume"),
            TrackButton::Pan => self.base.on_fader_mode_button(ctx, event, Views::TrackPan, "Pan"),
            TrackButton::Sends => self.base.on_fader_mode_button(ctx, event, Views::TrackSends, "Sends"),
        }
    }
}
