// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The hardware controls of a control surface (former `AbstractControlSurface`, `HwButtonImpl`,
//! `HwLightImpl` and the continuous controls).

use std::collections::BTreeMap;
use std::rc::Rc;

use super::command::{ContinuousCommand, SharedTrigger};
use super::ctx::Ctx;
use super::grid::PadGrid;
use super::ids::{ButtonId, ContinuousId, OutputId};
use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};

/// The timeout after which a pressed button is considered to be long pressed.
pub const BUTTON_STATE_INTERVAL: u64 = 500;

/// What happens when a button is triggered.
#[derive(Clone)]
pub enum ButtonAction {
    None,
    /// Execute a trigger command.
    Command(SharedTrigger),
    /// Hand the event to the active view as a grid note.
    GridNote(i32),
}

/// Called after a button event was handled.
pub type ButtonEventHandler = Rc<dyn Fn(&mut Ctx, ButtonEvent)>;

/// A hardware button.
pub struct HwButton {
    pub id: ButtonId,
    pub label: String,
    pub action: ButtonAction,
    /// The MIDI binding: type, channel, control and value (-1 for any value).
    pub binding: Option<(BindType, i32, i32, i32)>,
    pub light: Option<usize>,
    pub(crate) state: Option<ButtonEvent>,
    pub(crate) is_consumed: bool,
    pub(crate) pressed_velocity: i32,
    pub(crate) schedule_counter: u32,
    pub(crate) down_handlers: Vec<ButtonEventHandler>,
    pub(crate) up_handlers: Vec<ButtonEventHandler>,
}

impl HwButton {
    fn new(id: ButtonId, label: &str) -> Self {
        HwButton {
            id,
            label: label.to_string(),
            action: ButtonAction::None,
            binding: None,
            light: None,
            state: None,
            is_consumed: false,
            pressed_velocity: 0,
            schedule_counter: 0,
            down_handlers: Vec::new(),
            up_handlers: Vec::new(),
        }
    }

    pub fn is_pressed(&self) -> bool {
        matches!(self.state, Some(ButtonEvent::Down) | Some(ButtonEvent::Long))
    }

    pub fn is_long_pressed(&self) -> bool {
        self.state == Some(ButtonEvent::Long)
    }

    pub fn is_consumed(&self) -> bool {
        self.is_consumed
    }

    pub fn pressed_velocity(&self) -> i32 {
        self.pressed_velocity
    }

    pub fn command(&self) -> Option<SharedTrigger> {
        match &self.action {
            ButtonAction::Command(command) => Some(command.clone()),
            _ => None,
        }
    }
}

/// Computes the state of a light (e.g. a color index), -1 means off.
pub type LightSupplier = Rc<dyn Fn(&Ctx) -> i32>;
/// Sends the state of a light to the hardware.
pub type LightSender = Rc<dyn Fn(&mut Ctx, i32)>;

/// A light (LED) of the controller, either of a button or standalone.
pub struct HwLight {
    pub output_id: Option<OutputId>,
    pub button: Option<ButtonId>,
    pub(crate) supplier: LightSupplier,
    pub(crate) sender: LightSender,
    pub(crate) state: Option<i32>,
}

impl HwLight {
    /// Clear the cached state which forces sending the state on the next flush.
    pub fn force_flush(&mut self) {
        self.state = None;
    }

    pub fn state(&self) -> Option<i32> {
        self.state
    }
}

/// The kind of a continuous control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuousKind {
    Fader,
    AbsoluteKnob,
    RelativeKnob,
}

/// A knob or fader.
pub struct HwContinuous {
    pub id: ContinuousId,
    pub label: String,
    pub kind: ContinuousKind,
    pub command: Option<Rc<std::cell::RefCell<dyn ContinuousCommand>>>,
    pub touch_command: Option<SharedTrigger>,
    pub binding: Option<(BindType, i32, i32)>,
    pub is_touched: bool,
}

/// Translates a trigger (button LED) update into MIDI. The default sends a note or CC depending
/// on the bind type. Controllers can replace it (e.g. the Launchpad sends scene buttons as notes).
pub type TriggerSender = Rc<dyn Fn(&MidiOutput, BindType, i32, i32, i32)>;

fn default_trigger_sender(output: &MidiOutput, bind_type: BindType, channel: i32, cc: i32, value: i32) {
    if bind_type == BindType::Note {
        output.send_note_ex(channel, cc, value);
    } else {
        output.send_cc_ex(channel, cc, value);
    }
}

/// The hardware part of a control surface: MIDI ports, buttons, lights, continuous controls and
/// the pad grid.
pub struct Surface {
    pub output: MidiOutput,
    pub input: MidiInput,
    pub pad_grid: Option<PadGrid>,
    pub(crate) buttons: BTreeMap<ButtonId, HwButton>,
    pub(crate) lights: Vec<HwLight>,
    pub(crate) continuous: BTreeMap<ContinuousId, HwContinuous>,
    pub trigger_sender: TriggerSender,
    key_translation_table: Vec<i32>,
    knob_sensitivity_is_slow: bool,
    pub(crate) update_requested: bool,
    pub(crate) is_shutting_down: bool,
    last_notification: Option<String>,
}

impl Surface {
    pub fn new(output: MidiOutput, input: MidiInput, pad_grid: Option<PadGrid>) -> Self {
        Surface {
            output,
            input,
            pad_grid,
            buttons: BTreeMap::new(),
            lights: Vec::new(),
            continuous: BTreeMap::new(),
            trigger_sender: Rc::new(default_trigger_sender),
            key_translation_table: Vec::new(),
            knob_sensitivity_is_slow: false,
            update_requested: false,
            is_shutting_down: false,
            last_notification: None,
        }
    }

    /// Get the pad grid. Panics if the surface has no grid.
    pub fn grid(&mut self) -> &mut PadGrid {
        self.pad_grid.as_mut().expect("Surface has no pad grid")
    }

    pub fn grid_ref(&self) -> &PadGrid {
        self.pad_grid.as_ref().expect("Surface has no pad grid")
    }

    pub fn create_button(&mut self, id: ButtonId, label: &str) -> &mut HwButton {
        self.buttons.insert(id, HwButton::new(id, label));
        self.buttons.get_mut(&id).unwrap()
    }

    pub fn button(&self, id: ButtonId) -> Option<&HwButton> {
        self.buttons.get(&id)
    }

    pub fn button_mut(&mut self, id: ButtonId) -> Option<&mut HwButton> {
        self.buttons.get_mut(&id)
    }

    pub fn button_ids(&self) -> Vec<ButtonId> {
        self.buttons.keys().copied().collect()
    }

    /// Bind a button to a MIDI control. A value of -1 matches all values.
    pub fn bind_button(&mut self, id: ButtonId, bind_type: BindType, channel: i32, control: i32, value: i32) {
        if let Some(button) = self.buttons.get_mut(&id) {
            button.binding = Some((bind_type, channel, control, value));
            self.input.bind_button(id, bind_type, channel, control, value);
        }
    }

    pub fn unbind_button(&mut self, id: ButtonId) {
        self.input.unbind_button(id);
    }

    /// Re-establish the MIDI binding of a button (after [`Surface::unbind_button`]).
    pub fn rebind_button(&mut self, id: ButtonId) {
        if let Some((bind_type, channel, control, value)) = self.buttons.get(&id).and_then(|b| b.binding) {
            self.input.bind_button(id, bind_type, channel, control, value);
        }
    }

    pub fn add_button_event_handler(&mut self, id: ButtonId, event: ButtonEvent, handler: ButtonEventHandler) {
        if let Some(button) = self.buttons.get_mut(&id) {
            match event {
                ButtonEvent::Down => button.down_handlers.push(handler),
                ButtonEvent::Up => button.up_handlers.push(handler),
                ButtonEvent::Long => {}
            }
        }
    }

    /// Create a light. If a button is given, the light is attached to it.
    pub fn create_light(&mut self, output_id: Option<OutputId>, button: Option<ButtonId>, supplier: LightSupplier, sender: LightSender) -> usize {
        let index = self.lights.len();
        self.lights.push(HwLight { output_id, button, supplier, sender, state: None });
        if let Some(button) = button.and_then(|id| self.buttons.get_mut(&id)) {
            button.light = Some(index);
        }
        index
    }

    pub fn light(&mut self, output_id: OutputId) -> Option<&mut HwLight> {
        self.lights.iter_mut().find(|light| light.output_id == Some(output_id))
    }

    pub fn button_light(&mut self, id: ButtonId) -> Option<&mut HwLight> {
        let index = self.buttons.get(&id)?.light?;
        self.lights.get_mut(index)
    }

    pub fn create_continuous(&mut self, id: ContinuousId, label: &str, kind: ContinuousKind) -> &mut HwContinuous {
        self.continuous.insert(
            id,
            HwContinuous { id, label: label.to_string(), kind, command: None, touch_command: None, binding: None, is_touched: false },
        );
        self.continuous.get_mut(&id).unwrap()
    }

    pub fn continuous(&self, id: ContinuousId) -> Option<&HwContinuous> {
        self.continuous.get(&id)
    }

    pub fn bind_continuous(&mut self, id: ContinuousId, bind_type: BindType, channel: i32, control: i32) {
        if let Some(control_ref) = self.continuous.get_mut(&id) {
            control_ref.binding = Some((bind_type, channel, control));
            self.input.bind_continuous(id, bind_type, channel, control, false);
        }
    }

    pub fn is_pressed(&self, id: ButtonId) -> bool {
        self.buttons.get(&id).is_some_and(|b| b.is_pressed())
    }

    pub fn is_long_pressed(&self, id: ButtonId) -> bool {
        self.buttons.get(&id).is_some_and(|b| b.is_long_pressed())
    }

    pub fn is_shift_pressed(&self) -> bool {
        self.is_pressed(ButtonId::Shift)
    }

    pub fn is_select_pressed(&self) -> bool {
        self.is_pressed(ButtonId::Select)
    }

    pub fn is_delete_pressed(&self) -> bool {
        self.is_pressed(ButtonId::Delete)
    }

    pub fn is_mute_pressed(&self) -> bool {
        self.is_pressed(ButtonId::Mute)
    }

    pub fn is_solo_pressed(&self) -> bool {
        self.is_pressed(ButtonId::Solo)
    }

    /// Mark the button as consumed, the command will not receive the UP event.
    pub fn set_trigger_consumed(&mut self, id: ButtonId) {
        if let Some(button) = self.buttons.get_mut(&id) {
            button.is_consumed = true;
        }
    }

    pub fn is_trigger_consumed(&self, id: ButtonId) -> bool {
        self.buttons.get(&id).is_some_and(|b| b.is_consumed)
    }

    /// Mark all pressed pads as consumed.
    pub fn consume_pads(&mut self) {
        for button in self.buttons.values_mut() {
            if button.id >= ButtonId::Pad1 && button.id <= ButtonId::Pad64 && button.is_pressed() {
                button.is_consumed = true;
            }
        }
    }

    pub fn set_trigger(&self, bind_type: BindType, channel: i32, cc: i32, value: i32) {
        (self.trigger_sender)(&self.output, bind_type, channel, cc, value);
    }

    pub fn set_trigger_cc(&self, cc: i32, value: i32) {
        self.set_trigger(BindType::Cc, 0, cc, value);
    }

    pub fn key_translation_table(&self) -> &[i32] {
        &self.key_translation_table
    }

    /// Set the translation table for notes played on the pads. -1 blocks a note.
    pub fn set_key_translation_table(&mut self, table: Vec<i32>) {
        self.input.default_note_input().set_key_translation_table(&table);
        self.key_translation_table = table;
    }

    pub fn set_velocity_translation_table(&mut self, table: &[i32]) {
        self.input.default_note_input().set_velocity_translation_table(table);
    }

    pub fn is_knob_sensitivity_slow(&self) -> bool {
        self.knob_sensitivity_is_slow
    }

    pub fn set_knob_sensitivity_is_slow(&mut self, is_slow: bool) {
        self.knob_sensitivity_is_slow = is_slow;
    }

    /// Send a MIDI event to Reaper as if it was played on the controller.
    pub fn send_midi_event(&self, status: i32, data1: i32, data2: i32) {
        self.input.send_raw_midi_event(status, data1, data2);
    }

    /// Clear the cache of all lights, which forces sending all states on the next flush.
    pub fn clear_cache(&mut self) {
        for light in &mut self.lights {
            light.force_flush();
        }
        if let Some(grid) = &mut self.pad_grid {
            grid.force_flush();
        }
    }

    pub fn flush_button_leds(&mut self) {
        let indices: Vec<usize> = self.buttons.values().filter_map(|b| b.light).collect();
        for index in indices {
            self.lights[index].force_flush();
        }
    }

    /// Request a flush of the surface (executed with the next timer tick).
    pub fn flush(&mut self) {
        self.update_requested = true;
    }

    /// Returns true if the message differs from the last one (the display of the Java dummy
    /// display does not repeat the same notification).
    pub(crate) fn check_notification(&mut self, message: &str) -> bool {
        if self.last_notification.as_deref() == Some(message) {
            return false;
        }
        self.last_notification = Some(message.to_string());
        true
    }

    pub fn unbind_grid(&mut self) {
        let ids: Vec<ButtonId> = self.buttons.keys().copied().filter(|id| *id >= ButtonId::Pad1 && *id <= ButtonId::Pad64).collect();
        for id in ids {
            self.unbind_button(id);
        }
    }

    pub fn rebind_grid(&mut self) {
        let ids: Vec<ButtonId> = self.buttons.keys().copied().filter(|id| *id >= ButtonId::Pad1 && *id <= ButtonId::Pad64).collect();
        for id in ids {
            self.rebind_button(id);
        }
    }
}
