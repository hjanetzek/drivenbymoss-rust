// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Tests of the Launchpad sequencer and drum views.

use std::rc::Rc;
use std::time::{Duration, Instant};

use super::drum_lane_view::DrumLaneView;
use super::drum_view::DrumView;
use super::note_edit_view::NoteEditView;
use super::poly_sequencer_view::PolySequencerView;
use super::sequencer_view::SequencerView;
use crate::backend::{BackendCall, RecordingBackend, Sender};
use crate::controller::launchpad::colors::{LAUNCHPAD_COLOR_GREEN, LAUNCHPAD_COLOR_YELLOW};
use crate::controller::launchpad::definition::LaunchpadModel;
use crate::controller::launchpad::LaunchpadFactory;
use crate::controller::{ControllerFactory, MidiPorts};
use crate::daw::clip::NotePosition;
use crate::framework::ctx::Ctx;
use crate::framework::feature::View;
use crate::framework::ids::{ButtonId, Views};
use crate::framework::view::sequencer::clip_length_view::ClipLengthView;
use crate::host::{Host, MemoryLog};
use crate::midi::ButtonEvent;
use crate::parser::Parser;

fn create_ctx() -> (Ctx, Rc<RecordingBackend>) {
    let backend = Rc::new(RecordingBackend::default());
    let sender = Sender::new(backend.clone());
    let host = Host::new(Rc::new(MemoryLog::default()));
    let factory = LaunchpadFactory::new(LaunchpadModel::X);
    let config = factory.create_configuration();
    let parts = factory.create(host, sender, MidiPorts { input: 3, output: 5 }, config).expect("instance");
    let mut ctx = parts.ctx;
    feed(&mut ctx, &["/track/count 1", "/track/0/exists 1", "/track/0/type INSTRUMENT", "/track/0/select 1"]);
    backend.take_calls();
    (ctx, backend)
}

fn feed(ctx: &mut Ctx, lines: &[&str]) {
    let mut parser = Parser::new();
    for line in lines {
        parser.parse_line(&mut ctx.model, line);
    }
    ctx.model.take_events();
}

/// Feed the data of the cursor clip (start at 0, 4 beats long) with the given notes.
fn feed_clip(ctx: &mut Ctx, notes: &str) {
    let notes = format!("/clip/notes {notes}");
    feed(ctx, &["/clip/exists 1", "/clip/start 0", "/clip/end 4", "/clip/loop 1", &notes]);
}

fn clip_commands(backend: &RecordingBackend) -> Vec<(String, String)> {
    backend
        .take_calls()
        .into_iter()
        .filter_map(|c| match c {
            BackendCall::StringArg(p, command, value) if p == "clip" => Some((command, value)),
            BackendCall::DoubleArg(p, command, value) if p == "clip" => Some((command, value.to_string())),
            _ => None,
        })
        .collect()
}

/// Press and release a pad (x, y from the bottom left).
fn tap(view: &mut dyn View, ctx: &mut Ctx, x: i32, y: i32) {
    press(view, ctx, 36 + y * 8 + x, 100);
}

/// Press and release a pad (grid note) with the given velocity.
fn press(view: &mut dyn View, ctx: &mut Ctx, note: i32, velocity: i32) {
    if let Some(button) = ctx.surface.button_mut(ButtonId::get(ButtonId::Pad1, (note - 36) as usize)) {
        button.pressed_velocity = velocity;
    }
    view.on_grid_note(ctx, note, velocity);
    view.on_grid_note(ctx, note, 0);
}

fn pad_color(ctx: &Ctx, x: i32, y: i32) -> i32 {
    ctx.surface.grid_ref().light_info(36 + y * 8 + x).color
}

#[test]
fn sequencer_toggles_step_and_selects_resolution() {
    let (mut ctx, backend) = create_ctx();
    let mut view = SequencerView::new(&ctx);
    view.on_activate(&mut ctx);
    feed_clip(&mut ctx, "");
    view.update_control_surface(&mut ctx);
    clip_commands(&backend);

    // Lowest row, 3rd step
    let note = view.note_of_row(0);
    tap(&mut view, &mut ctx, 2, 0);
    let commands = clip_commands(&backend);
    assert_eq!(commands.len(), 1, "{commands:?}");
    assert_eq!(commands[0].0, format!("note/{note}/toggle"));
    assert!(commands[0].1.starts_with("0.5 0.25 100 0"), "{commands:?}");

    // Resolution: scene 1 is 1/32t (index 7), the default is 1/16 (index 4 -> scene 4)
    assert_eq!(view.button_color(&ctx, ButtonId::Scene4), LAUNCHPAD_COLOR_YELLOW);
    assert_eq!(view.button_color(&ctx, ButtonId::Scene1), LAUNCHPAD_COLOR_GREEN);
    view.on_button(&mut ctx, ButtonId::Scene1, ButtonEvent::Down, 127);
    assert_eq!(view.button_color(&ctx, ButtonId::Scene1), LAUNCHPAD_COLOR_YELLOW);

    // The pages are drawn in the top row (the clip is not playing: current step -1 is on the
    // 1st page)
    view.draw_grid(&mut ctx);
    let active_page = ctx.colors.color_index(crate::framework::color_ids::abstract_sequencer_view::COLOR_ACTIVE_PAGE);
    assert_eq!(pad_color(&ctx, 0, 7), active_page);
}

#[test]
fn sequencer_edits_notes_with_the_note_edit_view() {
    let (mut ctx, _backend) = create_ctx();
    ctx.register_view(Views::Sequencer, SequencerView::new(&ctx));
    ctx.register_view(Views::NoteEditView, NoteEditView::new(&ctx));
    ctx.set_active_view(Views::Sequencer);
    let note = {
        let view = ctx.views.get(Views::Sequencer).unwrap();
        let view = view.borrow();
        view.as_any().downcast_ref::<SequencerView>().unwrap().note_of_row(0)
    };
    feed_clip(&mut ctx, &format!("0:0:0.25:0.5:0:{note}:100"));
    ctx.flush_surface();

    // Long press on the step with the note and release
    let pad = 36 + 1;
    let view = ctx.views.get(Views::Sequencer).unwrap();
    view.borrow_mut().on_grid_note(&mut ctx, pad, 100);
    view.borrow_mut().on_grid_note_long_press(&mut ctx, pad);
    view.borrow_mut().on_grid_note(&mut ctx, pad, 0);

    let edit_view = ctx.views.get(Views::NoteEditView).unwrap();
    let notes = edit_view.borrow().note_editor().unwrap().notes().to_vec();
    assert_eq!(notes, vec![NotePosition::new(0, 1, note)]);

    // The note edit view is activated as temporary view
    ctx.run_due_tasks(Instant::now() + Duration::from_millis(1));
    assert!(ctx.views.is_active(Views::NoteEditView));
    // The velocity fader shows the velocity of the note
    let value = edit_view.borrow().as_virtual_fader_view().unwrap().fader_value(&ctx, 0);
    assert_eq!(value, 100);
}

#[test]
fn drum_view_plays_pads_and_toggles_steps() {
    let (mut ctx, backend) = create_ctx();
    let mut view = DrumView::new(&ctx);
    view.on_activate(&mut ctx);
    feed_clip(&mut ctx, "");
    view.update_control_surface(&mut ctx);
    clip_commands(&backend);

    // Select the 2nd drum pad (lower left 4x4 block)
    tap(&mut view, &mut ctx, 1, 0);
    assert_eq!(view.selected_pad(), 1);

    // Toggle the first step (top left pad)
    tap(&mut view, &mut ctx, 0, 7);
    let drum_offset = ctx.model.scales().drum_offset();
    let commands = clip_commands(&backend);
    assert_eq!(commands.len(), 1, "{commands:?}");
    assert_eq!(commands[0].0, format!("note/{}/toggle", drum_offset + 1));
    assert!(commands[0].1.starts_with("0.0 0.25 100 0"), "{commands:?}");

    // The upper right pad of the loop area toggles the extra buttons
    view.draw_grid(&mut ctx);
    let toggle_off = ctx.colors.color_index(crate::framework::color_ids::abstract_drum_ex_view::COLOR_EX_TOGGLE_OFF);
    assert_eq!(pad_color(&ctx, 7, 0), toggle_off);
    tap(&mut view, &mut ctx, 7, 0);
    view.draw_grid(&mut ctx);
    let toggle_on = ctx.colors.color_index(crate::framework::color_ids::abstract_drum_ex_view::COLOR_EX_TOGGLE_ON);
    assert_eq!(pad_color(&ctx, 7, 0), toggle_on);

    // The selected pad is drawn
    let selected = ctx.colors.color_index(crate::framework::color_ids::abstract_drum_view::COLOR_PAD_SELECTED);
    assert_eq!(pad_color(&ctx, 1, 0), selected);
}

#[test]
fn drum4_toggles_lanes() {
    let (mut ctx, backend) = create_ctx();
    let mut view = DrumLaneView::new_drum4(&ctx);
    assert_eq!(view.lanes(), 4);
    view.on_activate(&mut ctx);
    feed_clip(&mut ctx, "");
    view.update_control_surface(&mut ctx);
    clip_commands(&backend);

    // The bottom row is the 1st sound, second half of the steps
    tap(&mut view, &mut ctx, 3, 0);
    let drum_offset = ctx.model.scales().drum_offset();
    let commands = clip_commands(&backend);
    assert_eq!(commands.len(), 1, "{commands:?}");
    assert_eq!(commands[0].0, format!("note/{drum_offset}/toggle"));
    // Step 8 + 3 = 11
    assert!(commands[0].1.starts_with("2.75 0.25"), "{commands:?}");

    // The top row is the 4th sound, first half of the steps
    tap(&mut view, &mut ctx, 0, 7);
    let commands = clip_commands(&backend);
    assert_eq!(commands[0].0, format!("note/{}/toggle", drum_offset + 3));
    assert!(commands[0].1.starts_with("0.0 0.25"), "{commands:?}");
}

#[test]
fn poly_sequencer_records_played_notes() {
    let (mut ctx, backend) = create_ctx();
    let mut view = PolySequencerView::new(&ctx, true);
    view.on_activate(&mut ctx);
    feed_clip(&mut ctx, "");
    view.update_control_surface(&mut ctx);
    clip_commands(&backend);

    // Without played notes nothing is created
    tap(&mut view, &mut ctx, 0, 7);
    assert!(clip_commands(&backend).is_empty());

    // Play a note in the lower half and set the first step
    let pad = 36;
    press(&mut view, &mut ctx, pad, 90);
    tap(&mut view, &mut ctx, 0, 7);
    let commands = clip_commands(&backend);
    assert_eq!(commands.len(), 1, "{commands:?}");
    assert!(commands[0].0.ends_with("/toggle"), "{commands:?}");
    assert!(commands[0].1.starts_with("0.0 0.25 90 0"), "{commands:?}");
}

#[test]
fn clip_length_sets_the_loop() {
    let (mut ctx, backend) = create_ctx();
    let mut view = ClipLengthView::new(&ctx, true);
    view.on_activate(&mut ctx);
    feed_clip(&mut ctx, "");
    backend.take_calls();

    // Press the 1st and the 3rd pad of the top row: loop of 3 measures
    let first = 36 + 7 * 8;
    view.on_grid_note(&mut ctx, first, 100);
    view.on_grid_note(&mut ctx, first + 2, 100);
    view.on_grid_note(&mut ctx, first + 2, 0);
    view.on_grid_note(&mut ctx, first, 0);
    let clip = ctx.model.note_clip(0, 0);
    assert_eq!(clip.play_start(), 0.0);
    assert_eq!(clip.loop_length(), 12.0);
    assert!(backend.take_calls().contains(&BackendCall::DoubleArg("clip".into(), "end".into(), 12.0)));

    view.on_button(&mut ctx, ButtonId::Scene2, ButtonEvent::Down, 127);
    let selected = ctx.colors.color_index(crate::framework::color_ids::abstract_sequencer_view::COLOR_RESOLUTION_SELECTED);
    assert_eq!(view.button_color(&ctx, ButtonId::Scene2), selected);
}
