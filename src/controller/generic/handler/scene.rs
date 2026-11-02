// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The handler for scene commands (former `SceneHandler`).

use super::{is_button_pressed, to_midi_value, unsupported};
use crate::controller::generic::command::FlexiCommand;
use crate::controller::generic::slot::{KnobMode, MidiValue};
use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;

/// Former `getCommandValue`.
pub fn command_value(ctx: &Ctx, command: FlexiCommand) -> i32 {
    let scene_bank = ctx.model.scene_bank();
    match command {
        FlexiCommand::SceneSelectPreviousBank => to_midi_value(scene_bank.can_scroll_page_backwards()),
        FlexiCommand::SceneSelectNextBank => to_midi_value(scene_bank.can_scroll_page_forwards()),
        FlexiCommand::SceneScrollBankPageBy1Left => to_midi_value(scene_bank.can_scroll_backwards()),
        FlexiCommand::SceneScrollBankPageBy1Right => to_midi_value(scene_bank.can_scroll_forwards()),
        _ => -1,
    }
}

/// Former `handle`.
pub fn handle(ctx: &mut Ctx, command: FlexiCommand, knob_mode: KnobMode, value: MidiValue) {
    // All scene commands are only executed when the button is pressed
    if !is_button_pressed(knob_mode, value) {
        return;
    }

    // Scene 1-8: Launch Scene (Alternative)
    let launch = command.index_in(FlexiCommand::Scene1LaunchScene, 8).map(|index| (index, false)).or_else(|| command.index_in(FlexiCommand::Scene1LaunchAltScene, 8).map(|index| (index, true)));
    if let Some((index, is_alternative)) = launch {
        let mut scene_bank = ctx.model.scene_bank_mut();
        let scene = scene_bank.item_mut(index);
        scene.select();
        scene.launch(true, is_alternative);
        return;
    }

    match command {
        FlexiCommand::SceneSelectPreviousBank => ctx.model.scene_bank_mut().select_previous_page(),
        FlexiCommand::SceneSelectNextBank => ctx.model.scene_bank_mut().select_next_page(),
        FlexiCommand::SceneScrollBankPageBy1Left => ctx.model.scene_bank_mut().scroll_backwards(),
        FlexiCommand::SceneScrollBankPageBy1Right => ctx.model.scene_bank_mut().scroll_forwards(),
        FlexiCommand::SceneCreateScene => {
            // Former ProjectImpl.createScene
            let length = ctx.config.new_clip_length_in_beats(ctx.model.transport().quarters_per_measure());
            ctx.model.project_mut().create_scene(length);
        }
        FlexiCommand::SceneCreateSceneFromPlayingClips => ctx.model.project_mut().create_scene_from_playing_launcher_clips(),
        _ => unsupported(ctx, command),
    }
}
