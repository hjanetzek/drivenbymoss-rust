// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The function of a knob touch combination (former `ElectraOneTouchCombinationCommand` and
//! `ElectraOneMiniTouchCombinationCommand`, both a `FootswitchCommand`).
//!
//! As in Java, the functions after the modes are the footswitch functions with the index of the
//! setting minus 10. That is right for the normal model; on the Mini (whose footswitch functions
//! start at 4) it shifts them by 6, and "Action" is never reached (see
//! doc/UPSTREAM_CANDIDATES.md).

use super::definition::ElectraVariant;
use super::surface;
use crate::config::electra as config;
use crate::framework::commands::misc::FootswitchCommand;
use crate::framework::ctx::Ctx;
use crate::midi::ButtonEvent;

/// The function of a knob touch combination.
pub struct TouchCombinationCommand {
    variant: ElectraVariant,
    footswitch: FootswitchCommand,
}

impl TouchCombinationCommand {
    pub fn new(variant: ElectraVariant, index: usize) -> Self {
        TouchCombinationCommand { variant, footswitch: FootswitchCommand::new(index) }
    }

    pub fn index(&self) -> usize {
        self.footswitch.index
    }

    /// Former `getSetting`: the footswitch function (only used by the base class).
    fn setting(&self, ctx: &Ctx) -> i32 {
        config::assignable(&ctx.config, self.index()) as i32 - 10
    }

    /// Former `execute`.
    pub fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent) {
        let assignable = config::assignable(&ctx.config, self.index());
        let is_down = event == ButtonEvent::Down;
        let page = match self.variant {
            ElectraVariant::Normal => match assignable {
                config::ELECTRA_ONE_FUNC_OFF => return,
                config::ELECTRA_ONE_FUNC_SHIFT_BUTTON => {
                    surface::update_shift(ctx, is_down);
                    return;
                }
                config::ELECTRA_ONE_FUNC_MODE_MIXER => 0,
                config::ELECTRA_ONE_FUNC_MODE_SENDS => 1,
                config::ELECTRA_ONE_FUNC_MODE_DEVICE => 2,
                config::ELECTRA_ONE_FUNC_MODE_EQ => 3,
                config::ELECTRA_ONE_FUNC_MODE_TRANSPORT => 4,
                config::ELECTRA_ONE_FUNC_MODE_SESSION => 5,
                config::ELECTRA_ONE_FUNC_MODE_PARAMS => 6,
                config::ELECTRA_ONE_FUNC_MODE_SYNTH_PRESET => {
                    if is_down {
                        surface::switch_to_specific_device_preset(ctx);
                    }
                    return;
                }
                config::ELECTRA_ONE_FUNC_MODE_ACTION => {
                    self.invoke_action(ctx, event);
                    return;
                }
                _ => -1,
            },
            ElectraVariant::Mini => match assignable {
                config::MINI_FUNC_OFF => return,
                config::MINI_FUNC_MODE_MIXER => 0,
                config::MINI_FUNC_MODE_DEVICE => 1,
                config::MINI_FUNC_MODE_TRANSPORT => 2,
                config::MINI_FUNC_MODE_ACTION => {
                    self.invoke_action(ctx, event);
                    return;
                }
                _ => -1,
            },
        };
        if page >= 0 {
            if is_down {
                surface::select_page(ctx, page);
            }
            return;
        }
        let setting = self.setting(ctx);
        self.footswitch.execute_setting(ctx, event, setting);
    }

    /// Invoke the action of the setting (as in Java also an empty one).
    fn invoke_action(&self, ctx: &mut Ctx, event: ButtonEvent) {
        if event != ButtonEvent::Down {
            return;
        }
        let action = config::assignable_action(&ctx.config, self.index());
        ctx.model.application_mut().invoke_action(&action);
    }
}
