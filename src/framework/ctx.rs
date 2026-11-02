// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The context of a controller instance. In the Java version views, modes and commands keep
//! references to the surface and the model; in Rust all of them receive the context as a mutable
//! reference instead.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use super::color::ColorManager;
use super::command::SharedTrigger;
use super::feature::{FeatureGroupManager, Mode, ModeManager, View, ViewManager};
use super::ids::{ButtonId, ContinuousId, Modes, Views};
use super::surface::{ButtonAction, ContinuousKind, Surface, BUTTON_STATE_INTERVAL};
use crate::backend::Sender;
use crate::config::Configuration;
use crate::daw::bank::BankItem;
use crate::daw::model::Model;
use crate::host::{Host, Scheduler};
use crate::midi::{BindType, ButtonEvent, MidiDispatch, CMD_CC, CMD_CHANNEL_AFTERTOUCH, CMD_NOTE_OFF, CMD_NOTE_ON, CMD_POLY_AFTERTOUCH};

/// The index of a mode manager, see [`Ctx::mode_manager`].
pub type ModeManagerId = usize;
/// The index of the main mode manager ([`Ctx::modes`]).
pub const MAIN_MODE_MANAGER: ModeManagerId = 0;

/// Handles MIDI messages which are not bound to a control (status, data1, data2).
pub type MidiHandler = Rc<dyn Fn(&mut Ctx, i32, i32, i32)>;
/// Handles incoming system exclusive messages (as an upper case hex string).
pub type SysexHandler = Rc<dyn Fn(&mut Ctx, &str)>;

/// Everything a controller instance consists of.
pub struct Ctx {
    pub host: Host,
    pub sender: Sender,
    pub model: Model,
    pub config: Configuration,
    pub surface: Surface,
    pub views: ViewManager,
    pub modes: ModeManager,
    /// Additional mode managers (index 1.. of [`Ctx::mode_manager`]), e.g. for the button rows
    /// or the faders of a controller. Index 0 is [`Ctx::modes`].
    pub extra_modes: Vec<ModeManager>,
    pub colors: ColorManager,
    pub scheduler: Scheduler<Ctx>,
    /// Handler for MIDI messages which are not bound to a control.
    pub midi_handler: Option<MidiHandler>,
    pub sysex_handler: Option<SysexHandler>,
    /// Notify the name of the view when the preferred view of a track is recalled.
    pub notify_view_change: bool,
    /// When the notification on the text display ends: checked against the time, not by counting
    /// the watch intervals, since the scheduled tasks only run with the ticks
    pub(crate) notification_end: Option<Instant>,
    /// Controller specific state, use [`Ctx::ext`] to access it.
    pub ext: Box<dyn Any>,
    /// The text display of the controller, if any. Notifications are shown on it instead of
    /// the host.
    pub text_display: Option<super::display::TextDisplay>,
    /// Set by the controller to request a restart of its instance (former `IHost.restart`),
    /// executed by the app after the current tick.
    pub restart_requested: bool,
    /// The other surfaces of an instance with several surfaces (e.g. MCU extenders), see
    /// [`super::multi_surface`].
    pub surfaces: super::multi_surface::SurfaceSet,
}

impl Ctx {
    pub fn new(host: Host, sender: Sender, model: Model, config: Configuration, surface: Surface, colors: ColorManager) -> Self {
        Ctx {
            host,
            sender,
            model,
            config,
            surface,
            views: FeatureGroupManager::default(),
            modes: FeatureGroupManager::default(),
            extra_modes: Vec::new(),
            colors,
            scheduler: Scheduler::default(),
            midi_handler: None,
            sysex_handler: None,
            notify_view_change: true,
            notification_end: None,
            ext: Box::new(()),
            text_display: None,
            restart_requested: false,
            surfaces: super::multi_surface::SurfaceSet::default(),
        }
    }

    /// Typed access to the controller specific state.
    pub fn ext<T: 'static>(&self) -> &T {
        self.ext.downcast_ref::<T>().expect("Wrong controller extension type")
    }

    pub fn ext_mut<T: 'static>(&mut self) -> &mut T {
        self.ext.downcast_mut::<T>().expect("Wrong controller extension type")
    }

    /// Removes the notification from the text display when the notification time is over.
    fn watch_notification(ctx: &mut Ctx) {
        let now = ctx.host.now();
        let Some(end) = ctx.notification_end else {
            return;
        };
        if now >= end {
            ctx.notification_end = None;
            if let Some(display) = &mut ctx.text_display {
                display.end_notification();
            }
            return;
        }
        let remaining = end.duration_since(now).as_millis() as u64;
        ctx.schedule(remaining.min(super::display::NOTIFICATION_WATCH_INTERVAL as u64), Self::watch_notification);
    }

    /// Execute a task after the given delay in milliseconds.
    pub fn schedule(&mut self, delay_ms: u64, task: impl FnOnce(&mut Ctx) + 'static) {
        // With several surfaces the task runs with the surface which is current now
        if let Some(index) = self.surfaces.task_surface() {
            self.scheduler.schedule(self.host.now(), delay_ms, move |ctx: &mut Ctx| ctx.with_surface(index, task));
            return;
        }
        self.scheduler.schedule(self.host.now(), delay_ms, task);
    }

    /// Execute all scheduled tasks which are due.
    pub fn run_due_tasks(&mut self, now: Instant) {
        self.model.run_due_tasks(now);
        for task in self.scheduler.take_due(now) {
            task(self);
        }
    }

    /// Show a notification on the text display. Without a text display (e.g. the Launchpad) the
    /// notification is shown by the host.
    pub fn notify(&mut self, message: &str) {
        if let Some(display) = &mut self.text_display {
            let start_timer = display.notify(message);
            self.notification_end = Some(self.host.now() + std::time::Duration::from_millis(super::display::NOTIFICATION_TIME as u64));
            if start_timer {
                self.schedule(super::display::NOTIFICATION_WATCH_INTERVAL as u64, Self::watch_notification);
            }
            return;
        }
        if self.surface.check_notification(message) {
            self.host.show_notification(message);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Views

    pub fn register_view(&mut self, id: Views, view: impl View + 'static) {
        self.views.register(id, Rc::new(RefCell::new(view)));
    }

    pub fn active_view(&self) -> Option<Rc<RefCell<dyn View>>> {
        self.views.active()
    }

    pub fn set_active_view(&mut self, id: Views) {
        set_active::<Views, dyn View, _>(self, Some(id), Self::views_of, activate_view, |v, c| v.on_deactivate(c));
    }

    pub fn set_temporary_view(&mut self, id: Views) {
        set_temporary::<Views, dyn View, _>(self, id, Self::views_of, activate_view, |v, c| v.on_deactivate(c));
    }

    pub fn restore_view(&mut self) {
        restore::<Views, dyn View, _>(self, Self::views_of, activate_view, |v, c| v.on_deactivate(c));
    }

    fn views_of(ctx: &mut Ctx) -> &mut ViewManager {
        &mut ctx.views
    }

    /// Activate the view and store it as the preferred view of the cursor track.
    pub fn activate_preferred_view(&mut self, id: Views) {
        if !self.views.contains(id) {
            return;
        }
        self.set_active_view(id);
        let cursor_track = self.model.cursor_track();
        if cursor_track.does_exist() {
            let position = cursor_track.position();
            self.views.set_preferred(position, id);
        }
    }

    /// Activate the preferred view of the cursor track.
    pub fn recall_preferred_view(&mut self) {
        let track = self.model.cursor_track();
        if !track.does_exist() {
            return;
        }
        let preferred = self.views.preferred(track.position()).unwrap_or_else(|| {
            if track.can_hold_notes() {
                self.config.startup_view()
            } else {
                self.config.preferred_audio_view()
            }
        });
        let Some(view) = self.views.get(preferred) else {
            return;
        };
        self.set_active_view(preferred);
        if self.notify_view_change {
            let name = view.borrow().name().to_string();
            self.notify(&name);
        }
    }

    /// Call `update_note_mapping` on the active view.
    pub fn update_view_note_mapping(&mut self) {
        if let Some(view) = self.active_view() {
            view.borrow_mut().update_note_mapping(self);
        }
    }

    /// Hand the model events to all registered views (see [`View::on_model_event`]). Views which
    /// are currently borrowed are skipped.
    pub fn dispatch_model_events_to_views(&mut self, events: &[crate::daw::events::ModelEvent]) {
        if events.is_empty() {
            return;
        }
        let views: Vec<Rc<RefCell<dyn View>>> = self.views.groups.values().cloned().collect();
        for view in views {
            if let Ok(mut view) = view.try_borrow_mut() {
                for event in events {
                    view.on_model_event(self, event);
                }
            }
        }
    }

    /// Hand the IDs of changed settings to all registered views (see
    /// [`View::on_setting_changed`]). Views which are currently borrowed are skipped.
    pub fn dispatch_setting_changes_to_views(&mut self, ids: &[crate::config::SettingId]) {
        if ids.is_empty() {
            return;
        }
        let views: Vec<Rc<RefCell<dyn View>>> = self.views.groups.values().cloned().collect();
        for view in views {
            if let Ok(mut view) = view.try_borrow_mut() {
                for id in ids {
                    view.on_setting_changed(self, *id);
                }
            }
        }
    }

    /// Get the color of a button from the active view.
    pub fn button_color_from_active_view(&self, id: ButtonId) -> i32 {
        self.views.active().map(|v| v.borrow().button_color(self, id)).unwrap_or(0)
    }

    // ---------------------------------------------------------------------------------------
    // Modes

    pub fn register_mode(&mut self, id: Modes, mode: impl Mode + 'static) {
        self.modes.register(id, Rc::new(RefCell::new(mode)));
    }

    pub fn active_mode(&self) -> Option<Rc<RefCell<dyn Mode>>> {
        self.modes.active()
    }

    pub fn set_active_mode(&mut self, id: Modes) {
        set_active::<Modes, dyn Mode, _>(self, Some(id), Self::modes_of, |m, c| m.on_activate(c), |m, c| m.on_deactivate(c));
    }

    pub fn set_temporary_mode(&mut self, id: Modes) {
        set_temporary::<Modes, dyn Mode, _>(self, id, Self::modes_of, |m, c| m.on_activate(c), |m, c| m.on_deactivate(c));
    }

    pub fn restore_mode(&mut self) {
        restore::<Modes, dyn Mode, _>(self, Self::modes_of, |m, c| m.on_activate(c), |m, c| m.on_deactivate(c));
    }

    fn modes_of(ctx: &mut Ctx) -> &mut ModeManager {
        &mut ctx.modes
    }

    /// Get the color of a button from the active mode.
    pub fn mode_button_color(&self, id: ButtonId) -> i32 {
        self.modes.active().map(|m| m.borrow().button_color(self, id)).unwrap_or(0)
    }

    // ---------------------------------------------------------------------------------------
    // Additional mode managers (e.g. the Java `getTrackButtonModeManager` and
    // `getFaderModeManager` of the Launch Control XL). Index [`MAIN_MODE_MANAGER`] (0) is
    // [`Ctx::modes`].

    /// Add a mode manager and return its index.
    pub fn add_mode_manager(&mut self) -> ModeManagerId {
        self.extra_modes.push(FeatureGroupManager::default());
        self.extra_modes.len()
    }

    /// Get the mode manager with the given index (0 is the main mode manager).
    pub fn mode_manager(&self, manager: ModeManagerId) -> &ModeManager {
        if manager == MAIN_MODE_MANAGER {
            &self.modes
        } else {
            &self.extra_modes[manager - 1]
        }
    }

    pub fn mode_manager_mut(&mut self, manager: ModeManagerId) -> &mut ModeManager {
        if manager == MAIN_MODE_MANAGER {
            &mut self.modes
        } else {
            &mut self.extra_modes[manager - 1]
        }
    }

    /// Register a mode in the given mode manager.
    pub fn register_mode_in(&mut self, manager: ModeManagerId, id: Modes, mode: impl Mode + 'static) {
        self.mode_manager_mut(manager).register(id, Rc::new(RefCell::new(mode)));
    }

    pub fn active_mode_in(&self, manager: ModeManagerId) -> Option<Rc<RefCell<dyn Mode>>> {
        self.mode_manager(manager).active()
    }

    pub fn set_active_mode_in(&mut self, manager: ModeManagerId, id: Modes) {
        set_active::<Modes, dyn Mode, _>(self, Some(id), move |ctx: &mut Ctx| ctx.mode_manager_mut(manager), |m, c| m.on_activate(c), |m, c| m.on_deactivate(c));
    }

    pub fn set_temporary_mode_in(&mut self, manager: ModeManagerId, id: Modes) {
        set_temporary::<Modes, dyn Mode, _>(self, id, move |ctx: &mut Ctx| ctx.mode_manager_mut(manager), |m, c| m.on_activate(c), |m, c| m.on_deactivate(c));
    }

    pub fn restore_mode_in(&mut self, manager: ModeManagerId) {
        restore::<Modes, dyn Mode, _>(self, move |ctx: &mut Ctx| ctx.mode_manager_mut(manager), |m, c| m.on_activate(c), |m, c| m.on_deactivate(c));
    }

    /// Get the color of a button from the active mode of the given mode manager.
    pub fn mode_button_color_in(&self, manager: ModeManagerId, id: ButtonId) -> i32 {
        self.mode_manager(manager).active().and_then(|m| m.try_borrow().ok().map(|m| m.button_color(self, id))).unwrap_or(0)
    }

    pub fn update_active_mode_display(&mut self) {
        if let Some(mode) = self.active_mode() {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.update_display(self);
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Buttons

    /// Get the command bound to a button.
    pub fn button_command(&self, id: ButtonId) -> Option<SharedTrigger> {
        self.surface.button(id).and_then(|b| b.command())
    }

    /// Returns true if the button is pressed and marks it as consumed in that case.
    pub fn is_button_combination(&mut self, id: ButtonId) -> bool {
        if self.surface.is_pressed(id) {
            self.surface.set_trigger_consumed(id);
            return true;
        }
        false
    }

    /// Trigger a button event as if it was received from the hardware (value in [0..1]).
    pub fn trigger_button(&mut self, id: ButtonId, event: ButtonEvent, value: f64) {
        match event {
            ButtonEvent::Down => self.handle_button_pressed(id, value),
            ButtonEvent::Up => self.handle_button_release(id),
            ButtonEvent::Long => {
                if let Some(button) = self.surface.button_mut(id) {
                    button.state = Some(ButtonEvent::Long);
                }
                let velocity = self.surface.button(id).map(|b| b.pressed_velocity).unwrap_or(0);
                self.execute_button_action(id, ButtonEvent::Long, velocity);
            }
        }
    }

    fn handle_button_pressed(&mut self, id: ButtonId, value: f64) {
        if value == 0.0 {
            return;
        }
        let velocity = (value * 127.0) as i32;
        self.surface.button_press_count = self.surface.button_press_count.wrapping_add(1);
        let handlers = {
            let Some(button) = self.surface.button_mut(id) else {
                return;
            };
            button.state = Some(ButtonEvent::Down);
            button.is_consumed = false;
            button.schedule_counter += 1;
            button.pressed_velocity = velocity;
            button.down_handlers.clone()
        };
        self.schedule(BUTTON_STATE_INTERVAL, move |ctx| ctx.check_button_state(id));
        self.execute_button_action(id, ButtonEvent::Down, velocity);
        for handler in handlers {
            handler(self, ButtonEvent::Down);
        }
    }

    fn handle_button_release(&mut self, id: ButtonId) {
        let (handlers, is_consumed) = {
            let Some(button) = self.surface.button_mut(id) else {
                return;
            };
            button.state = Some(ButtonEvent::Up);
            (button.up_handlers.clone(), button.is_consumed)
        };
        if !is_consumed {
            self.execute_button_action(id, ButtonEvent::Up, 0);
        }
        for handler in handlers {
            handler(self, ButtonEvent::Up);
        }
    }

    fn check_button_state(&mut self, id: ButtonId) {
        let velocity = {
            let Some(button) = self.surface.button_mut(id) else {
                return;
            };
            button.schedule_counter = button.schedule_counter.saturating_sub(1);
            if button.schedule_counter > 0 || !button.is_pressed() {
                return;
            }
            button.state = Some(ButtonEvent::Long);
            button.pressed_velocity
        };
        self.execute_button_action(id, ButtonEvent::Long, velocity);
    }

    fn execute_button_action(&mut self, id: ButtonId, event: ButtonEvent, velocity: i32) {
        let action = match self.surface.button(id) {
            Some(button) => button.action.clone(),
            None => return,
        };
        match action {
            ButtonAction::None => {}
            ButtonAction::Command(command) => match command.try_borrow_mut() {
                Ok(mut command) => command.execute(self, event, velocity),
                Err(_) => self.host.error(&format!("Recursive execution of the command of button {}", id.name())),
            },
            ButtonAction::GridNote(note) => self.handle_grid_note(event, note, velocity),
        }
    }

    fn handle_grid_note(&mut self, event: ButtonEvent, note: i32, velocity: i32) {
        let Some(view) = self.active_view() else {
            return;
        };
        let Ok(mut view) = view.try_borrow_mut() else {
            return;
        };
        if event == ButtonEvent::Long {
            view.on_grid_note_long_press(self, note);
        } else {
            view.on_grid_note(self, note, velocity);
        }
    }

    /// Execute the trigger command of a button as if the button was pressed and released.
    pub fn simulate_button_press(&mut self, id: ButtonId, velocity: i32) {
        if let Some(command) = self.button_command(id) {
            command.borrow_mut().execute(self, ButtonEvent::Down, velocity);
            if velocity != 0 {
                command.borrow_mut().execute(self, ButtonEvent::Up, 0);
            }
        }
    }

    pub fn simulate_normal_button_press(&mut self, id: ButtonId) {
        if let Some(command) = self.button_command(id) {
            command.borrow_mut().execute_normal(self, ButtonEvent::Down);
            command.borrow_mut().execute_normal(self, ButtonEvent::Up);
        }
    }

    pub fn simulate_shifted_button_press(&mut self, id: ButtonId) {
        if let Some(command) = self.button_command(id) {
            command.borrow_mut().execute_shifted(self, ButtonEvent::Down);
            command.borrow_mut().execute_shifted(self, ButtonEvent::Up);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Hardware setup

    /// Create the pad buttons of the grid with their lights and MIDI bindings.
    pub fn create_pads(&mut self) {
        let Some(grid) = &self.surface.pad_grid else {
            return;
        };
        let size = grid.rows() * grid.cols();
        let start_note = grid.start_note();
        for i in 0..size {
            let note = start_note + i;
            let id = ButtonId::get(ButtonId::Pad1, i as usize);
            let (channel, control) = self.surface.grid_ref().translate_to_controller(note);
            let button = self.surface.create_button(id, &format!("P {}", i + 1));
            button.action = ButtonAction::GridNote(note);
            self.surface.create_light(
                None,
                Some(id),
                Rc::new(move |ctx: &Ctx| ctx.surface.grid_ref().light_info(note).encoded()),
                Rc::new(move |ctx: &mut Ctx, _state| {
                    let surface = &mut ctx.surface;
                    if let Some(grid) = &mut surface.pad_grid {
                        grid.send_state(&surface.output, note);
                    }
                }),
            );
            self.surface.bind_button(id, BindType::Note, channel, control, -1);
        }
    }

    /// Add a button with a command, a MIDI binding and a light. The light state is the result of
    /// the supplier, which is sent as the value of the trigger. With a negative MIDI control, the
    /// button is not bound to MIDI (virtual button) and has no light.
    #[allow(clippy::too_many_arguments)]
    pub fn add_button(
        &mut self,
        id: ButtonId,
        label: &str,
        command: Option<SharedTrigger>,
        bind_type: BindType,
        midi_channel: i32,
        midi_control: i32,
        supplier: Option<super::surface::LightSupplier>,
    ) {
        let button = self.surface.create_button(id, label);
        if let Some(command) = command {
            button.action = ButtonAction::Command(command);
        }
        if midi_control < 0 {
            return;
        }
        self.surface.bind_button(id, bind_type, midi_channel, midi_control, -1);
        let supplier = supplier.unwrap_or_else(|| Rc::new(move |ctx: &Ctx| if ctx.surface.is_pressed(id) { 1 } else { 0 }));
        self.surface.create_light(
            None,
            Some(id),
            supplier,
            Rc::new(move |ctx: &mut Ctx, state| ctx.surface.set_trigger(bind_type, midi_channel, midi_control, state)),
        );
    }

    /// Bind a continuous command to a knob or fader.
    #[allow(clippy::too_many_arguments)]
    pub fn add_continuous(
        &mut self,
        id: ContinuousId,
        label: &str,
        kind: ContinuousKind,
        command: Rc<RefCell<dyn super::command::ContinuousCommand>>,
        bind_type: BindType,
        midi_channel: i32,
        midi_control: i32,
    ) {
        let control = self.surface.create_continuous(id, label, kind);
        control.command = Some(command);
        self.surface.bind_continuous(id, bind_type, midi_channel, midi_control);
    }

    // ---------------------------------------------------------------------------------------
    // MIDI

    /// Handle a short MIDI message from the controller.
    pub fn handle_midi(&mut self, status: i32, data1: i32, data2: i32) {
        let (dispatch, forward) = self.surface.input.handle_short_message(status, data1, data2);
        match dispatch {
            Some(MidiDispatch::Button(id, event, value)) => self.trigger_button(id, event, value),
            Some(MidiDispatch::Continuous(id, value)) => self.handle_continuous(id, value),
            Some(MidiDispatch::Touch(id, is_touched)) => self.handle_touch(id, is_touched),
            None => {}
        }
        if !forward {
            return;
        }

        let code = status & 0xF0;
        match code {
            CMD_POLY_AFTERTOUCH => self.handle_aftertouch(data1, data2),
            CMD_CHANNEL_AFTERTOUCH => self.handle_aftertouch(-1, data1),
            CMD_NOTE_ON | CMD_NOTE_OFF => {
                if let Some(handler) = self.midi_handler.clone() {
                    handler(self, status, data1, data2);
                }
            }
            CMD_CC => match self.midi_handler.clone() {
                Some(handler) => handler(self, status, data1, data2),
                None => self.host.error(&format!("CC {data1} on MIDI channel {} should be handled in framework...", status & 0xF)),
            },
            _ => {
                if let Some(handler) = self.midi_handler.clone() {
                    handler(self, status, data1, data2);
                }
            }
        }
    }

    /// Handle a system exclusive message from the controller.
    pub fn handle_sysex(&mut self, data: &[u8]) {
        if let Some(handler) = self.sysex_handler.clone() {
            handler(self, &crate::midi::to_hex_str(data));
        }
    }

    fn handle_aftertouch(&mut self, note: i32, value: i32) {
        if let Some(view) = self.active_view() {
            if let Ok(mut view) = view.try_borrow_mut() {
                view.execute_aftertouch_command(self, note, value);
            }
        }
    }

    fn handle_continuous(&mut self, id: ContinuousId, value: f64) {
        let Some(control) = self.surface.continuous(id) else {
            return;
        };
        if let Some(observer) = control.changed_observer.clone() {
            observer(self);
        }
        let Some(control) = self.surface.continuous(id) else {
            return;
        };
        // A control which is bound to a parameter changes the parameter directly (former
        // `IHwContinuousControl.bind (IParameter)`)
        if let Some(parameter) = control.parameter.clone() {
            match control.kind {
                ContinuousKind::Fader | ContinuousKind::AbsoluteKnob => parameter.set_normalized_value(self, value),
                ContinuousKind::RelativeKnob => parameter.change_value(self, (value * 127.0).round() as i32),
            }
            return;
        }
        let Some(command) = control.command.clone() else {
            return;
        };
        let value = match control.kind {
            ContinuousKind::Fader | ContinuousKind::AbsoluteKnob | ContinuousKind::RelativeKnob => (value * 127.0).round() as i32,
        };
        command.borrow_mut().execute(self, value);
    }

    fn handle_touch(&mut self, id: ContinuousId, is_touched: bool) {
        let Some(control) = self.surface.continuous.get_mut(&id) else {
            return;
        };
        control.is_touched = is_touched;
        if let Some(command) = control.touch_command.clone() {
            let event = if is_touched { ButtonEvent::Down } else { ButtonEvent::Up };
            command.borrow_mut().execute(self, event, if is_touched { 127 } else { 0 });
        }
    }

    // ---------------------------------------------------------------------------------------
    // Flush

    /// Update the controller: let the active view update its controls and draw the grid, then
    /// send all changed light states.
    pub fn flush_surface(&mut self) {
        if self.surface.is_shutting_down {
            return;
        }
        if let Some(view) = self.active_view() {
            if let Ok(mut view) = view.try_borrow_mut() {
                view.update_control_surface(self);
                view.draw_grid(self);
            }
        }
        self.flush_lights();
        if let Some(display) = &mut self.text_display {
            display.flush();
        }
    }

    /// Send all changed light states.
    pub fn flush_lights(&mut self) {
        for index in 0..self.surface.lights.len() {
            let (supplier, sender, state) = {
                let light = &self.surface.lights[index];
                (light.supplier.clone(), light.sender.clone(), light.state)
            };
            let new_state = supplier(self);
            if state == Some(new_state) {
                continue;
            }
            self.surface.lights[index].state = Some(new_state);
            sender(self, new_state);
        }
    }

    /// Turn off all lights (on shutdown).
    pub fn turn_off_lights(&mut self) {
        for index in 0..self.surface.lights.len() {
            let sender = self.surface.lights[index].sender.clone();
            sender(self, 0);
            self.surface.lights[index].state = Some(0);
        }
    }
}

// -------------------------------------------------------------------------------------------
// Feature group manager functions which need access to the context

type Handler<F> = fn(&mut F, &mut Ctx);

fn call<F: ?Sized>(ctx: &mut Ctx, group: Option<Rc<RefCell<F>>>, handler: Handler<F>) {
    if let Some(group) = group {
        match group.try_borrow_mut() {
            Ok(mut group) => handler(&mut group, ctx),
            Err(_) => ctx.host.error("Feature group is already in use while (de-)activating it."),
        }
    }
}

fn notify<E: Copy + Eq + std::hash::Hash + 'static, F: ?Sized, A: Fn(&mut Ctx) -> &mut FeatureGroupManager<E, F> + Copy>(
    ctx: &mut Ctx,
    access: A,
    old: Option<E>,
    new: E,
) {
    let listeners = access(ctx).listeners.clone();
    for listener in listeners {
        listener(ctx, old, new);
    }
}

fn set_active<E: Copy + Eq + std::hash::Hash + 'static, F: ?Sized, A: Fn(&mut Ctx) -> &mut FeatureGroupManager<E, F> + Copy>(
    ctx: &mut Ctx,
    id: Option<E>,
    access: A,
    activate: Handler<F>,
    deactivate: Handler<F>,
) {
    let manager = access(ctx);
    let Some(id) = id.or(manager.default_id) else {
        ctx.host.error("Attempt to set the active feature group to null and no default feature group is registered.");
        return;
    };
    if manager.is_active(id) || !manager.contains(id) {
        return;
    }
    let old = manager.active();
    call(ctx, old, deactivate);
    let manager = access(ctx);
    manager.temporary_id = None;
    manager.previous_id = manager.active_id;
    manager.active_id = Some(id);
    let previous = manager.previous_id;
    let new = manager.get(id);
    call(ctx, new, activate);
    notify(ctx, access, previous, id);
}

fn set_temporary<E: Copy + Eq + std::hash::Hash + 'static, F: ?Sized, A: Fn(&mut Ctx) -> &mut FeatureGroupManager<E, F> + Copy>(
    ctx: &mut Ctx,
    id: E,
    access: A,
    activate: Handler<F>,
    deactivate: Handler<F>,
) {
    let manager = access(ctx);
    if manager.is_active(id) || !manager.contains(id) {
        return;
    }
    let old = manager.active();
    call(ctx, old, deactivate);
    let manager = access(ctx);
    manager.temporary_id = Some(id);
    let active = manager.active_id;
    let new = manager.get(id);
    call(ctx, new, activate);
    notify(ctx, access, active, id);
}

fn restore<E: Copy + Eq + std::hash::Hash + 'static, F: ?Sized, A: Fn(&mut Ctx) -> &mut FeatureGroupManager<E, F> + Copy>(
    ctx: &mut Ctx,
    access: A,
    activate: Handler<F>,
    deactivate: Handler<F>,
) {
    let manager = access(ctx);
    let old_id;
    if let Some(temporary) = manager.temporary_id {
        old_id = temporary;
        let old = manager.get(temporary);
        call(ctx, old, deactivate);
        let manager = access(ctx);
        manager.temporary_id = None;
        if manager.active_id.is_none_or(|id| !manager.contains(id)) {
            manager.active_id = manager.default_id;
        }
    } else if let (Some(previous), Some(active)) = (manager.previous_id, manager.active_id) {
        old_id = active;
        let old = manager.get(active);
        call(ctx, old, deactivate);
        let manager = access(ctx);
        manager.active_id = Some(previous);
        if !manager.contains(previous) {
            manager.active_id = manager.default_id;
        }
    } else {
        return;
    }
    let manager = access(ctx);
    let new_id = manager.active_id;
    let new = new_id.and_then(|id| manager.get(id));
    call(ctx, new, activate);
    if let Some(new_id) = new_id {
        notify(ctx, access, Some(old_id), new_id);
    }
}

/// Activate a view: `on_activate`, then `update_note_mapping` (former `AbstractView.onActivate`,
/// which calls `updateNoteMapping`).
fn activate_view(view: &mut dyn View, ctx: &mut Ctx) {
    view.on_activate(ctx);
    view.update_note_mapping(ctx);
}
