// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Parameters of the transport, the project and the groove which the model has no
//! [`ParamRef`](super::ParamRef) for, as [`CustomParameter`]s: the play position and the tempo
//! (former `framework.parameter.PlayPositionParameter` and `TempoParameter`), the cue volume of
//! the project (former `IProject.getCueVolumeParameter`) and a groove parameter (former
//! `IGroove.getParameter`). Wrap them with `ParamRef::Custom(Rc::new(...))`.

use crate::daw::constants::GrooveParameterId;
use crate::framework::ctx::Ctx;

use super::CustomParameter;

/// Moves the play position with a relative knob (former `PlayPositionParameter`). It has no
/// value (Java throws on `getValue`; here it is 0); its text is the position. The knob
/// sensitivity of the surface (slow) moves it by the zoom resolution, otherwise by bars.
pub struct PlayPositionParameter;

impl CustomParameter for PlayPositionParameter {
    fn does_exist(&self, _ctx: &Ctx) -> bool {
        true
    }

    fn name(&self, _ctx: &Ctx) -> String {
        "Play Position".to_string()
    }

    fn value(&self, _ctx: &Ctx) -> i32 {
        0
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        ctx.model.transport().position_text().to_string()
    }

    /// Not supported (Java throws).
    fn set_value(&self, _ctx: &mut Ctx, _value: i32) {}

    /// Not supported (Java throws).
    fn set_normalized_value(&self, _ctx: &mut Ctx, _value: f64) {}

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        let increase = ctx.model.value_changer().is_increase(control);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_position(increase, slow);
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        ctx.model.transport_mut().set_position(0.0);
    }
}

/// Changes the tempo with a relative knob (former `framework.parameter.TempoParameter`), by
/// 0.01 BPM with the slow knob sensitivity of the surface, otherwise by 1 BPM. It has no value
/// (Java throws on `getValue`; here it is 0); its text is the formatted tempo.
pub struct TempoParameter;

impl CustomParameter for TempoParameter {
    fn does_exist(&self, _ctx: &Ctx) -> bool {
        true
    }

    fn name(&self, _ctx: &Ctx) -> String {
        "Tempo".to_string()
    }

    fn value(&self, _ctx: &Ctx) -> i32 {
        0
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        let transport = ctx.model.transport();
        transport.format_tempo(transport.tempo())
    }

    /// Not supported (Java throws).
    fn set_value(&self, _ctx: &mut Ctx, _value: i32) {}

    /// Not supported (Java throws).
    fn set_normalized_value(&self, _ctx: &mut Ctx, _value: f64) {}

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        let increase = ctx.model.value_changer().is_increase(control);
        let slow = ctx.surface.is_knob_sensitivity_slow();
        ctx.model.transport_mut().change_tempo(increase, slow);
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        ctx.model.transport_mut().set_tempo(120.0);
    }
}

/// The cue volume of the project (former `IProject.getCueVolumeParameter`).
pub struct CueVolumeParameter;

impl CustomParameter for CueVolumeParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool {
        ctx.model.project().cue_volume_parameter().does_exist()
    }

    fn name(&self, ctx: &Ctx) -> String {
        ctx.model.project().cue_volume_parameter().name().to_string()
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        ctx.model.project().cue_volume_parameter().value()
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        ctx.model.project().cue_volume_parameter().displayed_value()
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        ctx.model.project_mut().cue_volume_parameter_mut().set_value(value);
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        ctx.model.project_mut().cue_volume_parameter_mut().set_normalized_value(value);
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        ctx.model.project_mut().change_cue_volume(control);
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        ctx.model.project_mut().cue_volume_parameter_mut().reset_value();
    }

    fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        ctx.model.project_mut().cue_volume_parameter_mut().touch_value(is_touched);
    }
}

/// A parameter of the groove (former `IGroove.getParameter`), e.g. the shuffle amount.
pub struct GrooveParameter(pub GrooveParameterId);

impl CustomParameter for GrooveParameter {
    fn does_exist(&self, ctx: &Ctx) -> bool {
        ctx.model.groove().parameter(self.0).does_exist()
    }

    fn name(&self, ctx: &Ctx) -> String {
        ctx.model.groove().parameter(self.0).name().to_string()
    }

    fn value(&self, ctx: &Ctx) -> i32 {
        ctx.model.groove().parameter(self.0).value()
    }

    fn displayed_value(&self, ctx: &Ctx) -> String {
        ctx.model.groove().parameter(self.0).displayed_value()
    }

    fn set_value(&self, ctx: &mut Ctx, value: i32) {
        ctx.model.groove_mut().parameter_mut(self.0).set_value(value);
    }

    fn set_normalized_value(&self, ctx: &mut Ctx, value: f64) {
        ctx.model.groove_mut().parameter_mut(self.0).set_normalized_value(value);
    }

    fn change_value(&self, ctx: &mut Ctx, control: i32) {
        ctx.model.groove_mut().parameter_mut(self.0).change_value(control);
    }

    fn reset_value(&self, ctx: &mut Ctx) {
        ctx.model.groove_mut().parameter_mut(self.0).reset_value();
    }

    fn touch_value(&self, ctx: &mut Ctx, is_touched: bool) {
        ctx.model.groove_mut().parameter_mut(self.0).touch_value(is_touched);
    }
}
