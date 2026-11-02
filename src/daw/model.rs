// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The root of the DAW model (former `IModel`, `AbstractModel`, `ModelImpl` and the cursor track
//! `ICursorTrack` / `CursorTrackImpl`).
//!
//! # Usage
//!
//! * The model is updated by the [`crate::parser::Parser`] with the messages from Reaper.
//!   After each message `Model::after_parse` is called, which keeps derived state consistent.
//! * Read access: e.g. `model.current_track_bank().item(i).is_mute()`,
//!   `model.cursor_track().slot_bank().item(s).is_playing()`.
//! * Commands: e.g. `model.current_track_bank_mut().item_mut(i).toggle_mute()`. Commands are
//!   sent to Reaper immediately, the model state is updated when Reaper sends the new state.
//! * Delayed tasks: some commands schedule follow-up tasks (as `host.scheduleTask` in Java).
//!   Call [`Model::run_due_tasks`] regularly (e.g. on every flush/timer tick).
//! * Events: instead of observers, the model records [`ModelEvent`]s which the controller layer
//!   drains with [`Model::take_events`].

use std::time::Instant;

use crate::backend::{Processor, Sender};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::scale::Scales;

use super::application::{Application, Arranger, Mixer};
use super::bank::{bank_locator, BankItem};
use super::browser::Browser;
use super::clip::{CursorClip, Note};
use super::color::ColorEx;
use super::constants::DeviceId;
use super::context::{DawContext, MASTER_POSITION};
use super::device::{DeviceKind, SpecificDevice};
use super::events::ModelEvent;
use super::groove::Groove;
use super::marker::MarkerBank;
use super::model_setup::ModelSetup;
use super::parameter::Parameter;
use super::parameter_bank::ParameterBank;
use super::project::Project;
use super::scene::SceneBank;
use super::slot::{Slot, SlotBank};
use super::track::Track;
use super::track_bank::{SceneBankGuard, TrackBank, TrackRef};
use super::transport::Transport;

/// The DAW model.
pub struct Model {
    ctx: DawContext,
    setup: ModelSetup,
    scales: Scales,
    application: Application,
    arranger: Arranger,
    mixer: Mixer,
    project: Project,
    transport: Transport,
    groove: Groove,
    marker_bank: MarkerBank,
    track_bank: TrackBank,
    pinned_track: Option<TrackRef>,
    /// `cursorTrack.hasPrevious ()`, `hasNext ()` reported by the host (Bitwig).
    host_cursor_track_neighbours: Option<(bool, bool)>,
    /// The host reports its pinned cursor track (`/track/pinned`, Bitwig) and handles the pin
    /// commands.
    host_pins_track: bool,
    cursor_device: SpecificDevice,
    first_instrument: Option<SpecificDevice>,
    eq_device: Option<SpecificDevice>,
    drum_device: Option<SpecificDevice>,
    additional_drum_devices: Vec<(usize, SpecificDevice)>,
    extra_scene_banks: Vec<(usize, SceneBank)>,
    cursor_clips: Vec<((usize, usize), CursorClip)>,
    browser: Option<Browser>,
    empty_track: Track,
    empty_slot_bank: SlotBank,
}

impl Model {
    /// Create the model.
    pub fn new(host: Host, sender: Sender, setup: ModelSetup, value_changer: ValueChanger, scales: Scales) -> Self {
        let ctx = DawContext::new(host, sender, value_changer);
        let num_params = setup.num_params.max(0) as usize;
        let num_sends = setup.num_sends;
        let num_scenes = setup.num_scenes;

        let mut drum_device = None;
        let mut additional_drum_devices = Vec::new();
        if setup.wants_drum_device {
            drum_device = Some(SpecificDevice::new(ctx.clone(), DeviceKind::Drum, num_sends, setup.num_params, setup.num_devices_in_bank, setup.num_drum_pad_layers));
            for page_size in &setup.additional_drum_devices {
                let device = SpecificDevice::new(ctx.clone(), DeviceKind::Drum, num_sends, 0, 0, *page_size as i32);
                additional_drum_devices.push((*page_size, device));
            }
        }

        let mut first_instrument = None;
        let mut eq_device = None;
        for id in &setup.device_ids {
            match id {
                DeviceId::FirstInstrument | DeviceId::NiKomplete => {
                    if first_instrument.is_none() {
                        first_instrument = Some(SpecificDevice::new(
                            ctx.clone(),
                            DeviceKind::FirstInstrument,
                            num_sends,
                            setup.num_params,
                            setup.num_devices_in_bank,
                            setup.num_drum_pad_layers,
                        ));
                    }
                }
                DeviceId::Eq => eq_device = Some(SpecificDevice::new(ctx.clone(), DeviceKind::Eq, 0, setup.num_params, 0, 0)),
            }
        }

        let mut track_bank =
            TrackBank::new(ctx.clone(), setup.num_tracks, num_scenes, num_sends, num_params, setup.has_flat_track_list, setup.has_full_flat_track_list);
        track_bank.recalc_if_dirty();
        track_bank.scene_bank_unguarded_mut().set_locator(bank_locator(|m: &mut Model| Some(&mut **m.track_bank_mut().scene_bank_unguarded_mut())));

        Model {
            application: Application::new(ctx.clone()),
            arranger: Arranger::new(ctx.clone()),
            mixer: Mixer::new(ctx.clone()),
            project: Project::new(ctx.clone(), num_params),
            transport: Transport::new(ctx.clone()),
            groove: Groove::new(ctx.clone()),
            marker_bank: MarkerBank::new(ctx.clone(), setup.num_markers),
            cursor_device: SpecificDevice::new(ctx.clone(), DeviceKind::Cursor, num_sends, setup.num_params, setup.num_devices_in_bank, setup.num_drum_pad_layers),
            browser: if setup.num_results > 0 { Some(Browser::new(ctx.clone(), setup.num_filter_column_entries, setup.num_results)) } else { None },
            empty_track: Track::empty(&ctx, num_sends),
            empty_slot_bank: SlotBank::new(ctx.muted(), num_scenes, -1),
            track_bank,
            pinned_track: None,
            host_cursor_track_neighbours: None,
            host_pins_track: false,
            first_instrument,
            eq_device,
            drum_device,
            additional_drum_devices,
            extra_scene_banks: Vec::new(),
            cursor_clips: Vec::new(),
            setup,
            scales,
            ctx,
        }
    }

    // ----------------------------------------------------------------------------------------
    // General

    /// The shared context (sender, host, value changer, scheduling).
    pub fn ctx(&self) -> &DawContext {
        &self.ctx
    }

    pub fn host(&self) -> &Host {
        self.ctx.host()
    }

    pub fn value_changer(&self) -> &ValueChanger {
        self.ctx.value_changer()
    }

    pub fn setup(&self) -> &ModelSetup {
        &self.setup
    }

    /// The setup as JSON for the host (see [`ModelSetup::to_json`]), with the sizes of the note
    /// clips which were created so far, also the 0x0 clip of [`Self::ensure_clip`] (Bitwig: a
    /// launcher cursor clip without a step grid, which follows the selection as in Java).
    pub fn setup_json(&self) -> String {
        let mut setup = self.setup.clone();
        setup.note_clips = self.cursor_clips.iter().map(|(key, _)| *key).collect();
        setup.to_json()
    }

    pub fn scales(&self) -> &Scales {
        &self.scales
    }

    pub fn scales_mut(&mut self) -> &mut Scales {
        &mut self.scales
    }

    /// Run all scheduled tasks which are due (also the delayed tasks of the cursor clips).
    pub fn run_due_tasks(&mut self, now: Instant) {
        for task in self.ctx.take_due_tasks(now) {
            task(self);
        }
        for (_, clip) in &mut self.cursor_clips {
            clip.run_due_tasks(now);
        }
        self.after_parse();
    }

    /// Run all scheduled tasks regardless of their due time (for tests).
    pub fn run_all_tasks(&mut self) {
        while self.ctx.has_tasks() {
            for task in self.ctx.take_all_tasks() {
                task(self);
            }
        }
        self.after_parse();
    }

    /// True if there are scheduled tasks.
    pub fn has_pending_tasks(&self) -> bool {
        self.ctx.has_tasks()
    }

    /// Take all events which were recorded since the last call.
    pub fn take_events(&mut self) -> Vec<ModelEvent> {
        self.ctx.take_events()
    }

    /// Must be called after the model was updated from the DAW (done by the parser). Updates the
    /// track list and the state which depends on the cursor track.
    pub fn after_parse(&mut self) {
        self.track_bank.recalc_if_dirty();
        let cursor = self.cursor_track_ref();
        let position = match cursor {
            Some(TrackRef::Master) => self.track_bank.master().position(),
            Some(TrackRef::Position(p)) => p as i32,
            None => -1,
        };
        self.ctx.set_cursor_track_position(position);
        let color = cursor.map(|t| self.track_bank.track(t).color());
        self.set_drum_pad_colors(color);
    }

    fn set_drum_pad_colors(&mut self, color: Option<ColorEx>) {
        if let Some(device) = &mut self.drum_device {
            device.drum_pad_bank_mut().set_pad_color(color);
        }
        for (_, device) in &mut self.additional_drum_devices {
            device.drum_pad_bank_mut().set_pad_color(color);
        }
    }

    /// Clean up on shutdown.
    pub fn cleanup(&mut self) {}

    // ----------------------------------------------------------------------------------------
    // Application parts

    pub fn application(&self) -> &Application {
        &self.application
    }

    pub fn application_mut(&mut self) -> &mut Application {
        &mut self.application
    }

    pub fn arranger(&self) -> &Arranger {
        &self.arranger
    }

    pub fn arranger_mut(&mut self) -> &mut Arranger {
        &mut self.arranger
    }

    pub fn mixer(&self) -> &Mixer {
        &self.mixer
    }

    pub fn mixer_mut(&mut self) -> &mut Mixer {
        &mut self.mixer
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn project_mut(&mut self) -> &mut Project {
        &mut self.project
    }

    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }

    pub fn groove(&self) -> &Groove {
        &self.groove
    }

    pub fn groove_mut(&mut self) -> &mut Groove {
        &mut self.groove
    }

    pub fn marker_bank(&self) -> &MarkerBank {
        &self.marker_bank
    }

    pub fn marker_bank_mut(&mut self) -> &mut MarkerBank {
        &mut self.marker_bank
    }

    /// The browser (None if `num_results` is 0 in the setup).
    pub fn browser(&self) -> Option<&Browser> {
        self.browser.as_ref()
    }

    pub fn browser_mut(&mut self) -> Option<&mut Browser> {
        self.browser.as_mut()
    }

    /// Browse the presets of the cursor device.
    pub fn browse_cursor_device_presets(&mut self) {
        let name = self.cursor_device.name().to_string();
        let position = self.cursor_device.position();
        if let Some(browser) = &mut self.browser {
            browser.replace_device(&name, position);
        }
    }

    // ----------------------------------------------------------------------------------------
    // Tracks

    pub fn track_bank(&self) -> &TrackBank {
        &self.track_bank
    }

    pub fn track_bank_mut(&mut self) -> &mut TrackBank {
        &mut self.track_bank
    }

    /// The current track bank (there are no effect track banks in Reaper, therefore always the
    /// track bank).
    pub fn current_track_bank(&self) -> &TrackBank {
        &self.track_bank
    }

    pub fn current_track_bank_mut(&mut self) -> &mut TrackBank {
        &mut self.track_bank
    }

    /// Reaper has no effect track bank.
    pub fn effect_track_bank(&self) -> Option<&TrackBank> {
        None
    }

    /// No effect track bank in Reaper; does nothing.
    pub fn toggle_current_track_bank(&mut self) {}

    pub fn is_effect_track_bank_active(&self) -> bool {
        false
    }

    pub fn master_track(&self) -> &Track {
        self.track_bank.master()
    }

    pub fn master_track_mut(&mut self) -> &mut Track {
        self.track_bank.master_mut()
    }

    // ----------------------------------------------------------------------------------------
    // Cursor track

    fn selected_track_ref(&self) -> Option<TrackRef> {
        if let Some(r) = self.track_bank.selected_ref() {
            return Some(r);
        }
        if self.track_bank.master().is_selected() {
            Some(TrackRef::Master)
        } else {
            None
        }
    }

    /// The pinned or selected track (None if no track is selected).
    pub fn cursor_track_ref(&self) -> Option<TrackRef> {
        if self.pinned_track.is_some() {
            return self.pinned_track;
        }
        self.selected_track_ref()
    }

    /// The pinned or selected track (an empty track if none is selected).
    pub fn cursor_track(&self) -> &Track {
        match self.cursor_track_ref() {
            Some(r) => self.track_bank.track(r),
            None => &self.empty_track,
        }
    }

    /// The pinned or selected track for modification (a fresh empty track if none is selected;
    /// its commands do nothing).
    pub fn cursor_track_mut(&mut self) -> &mut Track {
        match self.cursor_track_ref() {
            Some(r) => self.track_bank.track_mut(r),
            None => {
                self.empty_track = Track::empty(&self.ctx, self.setup.num_sends);
                &mut self.empty_track
            }
        }
    }

    /// True if there is a pinned or selected track.
    pub fn cursor_track_exists(&self) -> bool {
        self.cursor_track().does_exist()
    }

    /// The parameter bank of the cursor track (former `ParameterBankDelegator`).
    pub fn cursor_track_parameter_bank(&self) -> Option<&ParameterBank> {
        self.cursor_track().parameter_bank()
    }

    pub fn cursor_track_parameter_bank_mut(&mut self) -> Option<&mut ParameterBank> {
        self.cursor_track_mut().parameter_bank_mut()
    }

    /// The slot bank of the cursor track (an empty slot bank if there is no cursor track).
    pub fn cursor_track_slot_bank(&self) -> &SlotBank {
        match self.cursor_track_ref() {
            Some(r) => self.track_bank.track(r).slot_bank(),
            None => &self.empty_slot_bank,
        }
    }

    /// True if the host pins its cursor track itself and reports it (`/track/pinned`, Bitwig).
    pub fn host_pins_track(&self) -> bool {
        self.host_pins_track
    }

    pub fn is_cursor_track_pinned(&self) -> bool {
        self.pinned_track.is_some()
    }

    /// The pinned track.
    pub fn pinned_track(&self) -> Option<TrackRef> {
        self.pinned_track
    }

    pub fn toggle_cursor_track_pinned(&mut self) {
        // CursorTrackImpl.togglePinned: isPinned ().toggle () on a host which reports the pinned
        // track (Bitwig)
        if self.host_pins_track {
            self.ctx.send(Processor::Track, "pinned/toggle");
            return;
        }
        let pinned = self.pinned_track.is_none();
        self.set_cursor_track_pinned(pinned);
    }

    /// The host reported the position of its pinned cursor track, -1 if it is not pinned
    /// (Bitwig: `cursorTrack.isPinned ()`; the cursor track then stays on that track, also for the
    /// cursor device and the remote controls).
    pub fn set_host_pinned_track(&mut self, position: i32) {
        self.host_pins_track = true;
        let pinned = (position >= 0).then_some(TrackRef::Position(position as usize));
        if pinned == self.pinned_track {
            return;
        }
        self.pinned_track = pinned;
        self.ctx.set_pinned_track(pinned.map(|_| position));
        self.after_parse();
    }

    /// The host reported `cursorTrack.hasPrevious ()` / `hasNext ()` (Bitwig).
    pub fn set_host_cursor_track_neighbours(&mut self, has_previous: Option<bool>, has_next: Option<bool>) {
        let (previous, next) = self.host_cursor_track_neighbours.unwrap_or((false, false));
        self.host_cursor_track_neighbours = Some((has_previous.unwrap_or(previous), has_next.unwrap_or(next)));
    }

    /// Pin the currently selected track (or unpin it). Bitwig: CursorTrackImpl.setPinned,
    /// `cursorTrack.isPinned ().set (pinned)`; the host reports the pinned track.
    pub fn set_cursor_track_pinned(&mut self, pinned: bool) {
        if self.host_pins_track {
            self.ctx.send_bool(Processor::Track, "pinned", pinned);
            return;
        }
        if pinned {
            let selected = self.selected_track_ref();
            match selected {
                Some(r) if self.track_bank.track(r).does_exist() => self.pinned_track = Some(r),
                _ => {
                    self.set_cursor_track_pinned(false);
                    return;
                }
            }
        } else {
            self.pinned_track = None;
        }
        self.ctx.set_pinned_track(self.pinned_track.map(|r| match r {
            TrackRef::Master => MASTER_POSITION,
            TrackRef::Position(p) => p as i32,
        }));
        let pin_position = self.pinned_track.map(|r| self.track_bank.track(r).position()).unwrap_or(-1);
        if let Some(selected) = self.selected_track_ref() {
            self.track_bank.track(selected).channel().send_int("pin", pin_position);
        }
        self.after_parse();
    }

    /// CursorTrackImpl.canSelectPrevious: `cursorTrack.hasPrevious ()` (Bitwig); Reaper: the
    /// bank.
    pub fn cursor_track_can_select_previous(&self) -> bool {
        match self.host_cursor_track_neighbours {
            Some((previous, _)) => previous,
            None => self.track_bank.can_scroll_backwards(),
        }
    }

    /// CursorTrackImpl.canSelectNext: `cursorTrack.hasNext ()` (Bitwig).
    pub fn cursor_track_can_select_next(&self) -> bool {
        match self.host_cursor_track_neighbours {
            Some((_, next)) => next,
            None => self.track_bank.can_scroll_forwards(),
        }
    }

    /// CursorTrackImpl.selectPrevious: `cursorTrack.selectPrevious ()` on a host which reports
    /// the neighbours of its cursor track (Bitwig).
    pub fn cursor_track_select_previous(&mut self) {
        if self.host_cursor_track_neighbours.is_some() {
            self.ctx.send(Processor::Track, "selectPrevious");
            return;
        }
        self.track_bank.select_previous_item();
    }

    /// CursorTrackImpl.selectNext: `cursorTrack.selectNext ()` (Bitwig).
    pub fn cursor_track_select_next(&mut self) {
        if self.host_cursor_track_neighbours.is_some() {
            self.ctx.send(Processor::Track, "selectNext");
            return;
        }
        self.track_bank.select_next_item();
    }

    /// Move the selected track one up and select the track at the previous index.
    pub fn cursor_track_swap_with_previous(&mut self) {
        let Some(selected) = self.selected_track_ref() else {
            return;
        };
        self.track_bank.track(selected).channel().send_cmd("movePrev");
        let index = (self.cursor_track().index() - 1).max(0) as usize;
        self.track_bank.item_mut(index).select();
    }

    /// Move the selected track one down and select the track at the next index.
    pub fn cursor_track_swap_with_next(&mut self) {
        let Some(selected) = self.selected_track_ref() else {
            return;
        };
        self.track_bank.track(selected).channel().send_cmd("moveNext");
        let max = self.track_bank.page_size().saturating_sub(1) as i32;
        let index = (self.cursor_track().index() + 1).min(max).max(0) as usize;
        self.track_bank.item_mut(index).select();
    }

    /// True if the cursor track is muted because another track is soloed.
    pub fn is_cursor_track_muted_by_solo(&self) -> bool {
        match self.cursor_track_ref() {
            Some(TrackRef::Position(p)) => self.track_bank.is_muted_by_solo(p),
            _ => false,
        }
    }

    // ----------------------------------------------------------------------------------------
    // Scenes

    /// The scene bank of the track bank.
    pub fn scene_bank(&self) -> &SceneBank {
        self.track_bank.scene_bank()
    }

    /// Modify the scene bank of the track bank (slot banks follow its page when the guard is
    /// dropped).
    pub fn scene_bank_mut(&mut self) -> SceneBankGuard<'_> {
        self.track_bank.scene_bank_mut()
    }

    /// Create a scene bank with a different page size if it does not exist yet (former
    /// `getSceneBank(numScenes)`). The bank with the configured size is the one of the track
    /// bank (use [`Model::scene_bank`] / [`Model::scene_bank_mut`] for it).
    pub fn ensure_scene_bank(&mut self, num_scenes: usize) {
        if num_scenes != self.setup.num_scenes && !self.extra_scene_banks.iter().any(|(n, _)| *n == num_scenes) {
            let mut bank = SceneBank::new(self.ctx.clone(), num_scenes);
            bank.set_locator(bank_locator(move |m: &mut Model| m.scene_bank_sized_mut(num_scenes).map(|b| &mut **b)));
            self.extra_scene_banks.push((num_scenes, bank));
        }
    }

    /// Get the scene bank with the given page size (see [`Model::ensure_scene_bank`]).
    pub fn scene_bank_sized(&self, num_scenes: usize) -> Option<&SceneBank> {
        if num_scenes == self.setup.num_scenes {
            return Some(self.track_bank.scene_bank());
        }
        self.extra_scene_banks.iter().find(|(n, _)| *n == num_scenes).map(|(_, b)| b)
    }

    /// Get an additional scene bank (not the one of the track bank) for modification.
    pub fn scene_bank_sized_mut(&mut self, num_scenes: usize) -> Option<&mut SceneBank> {
        self.extra_scene_banks.iter_mut().find(|(n, _)| *n == num_scenes).map(|(_, b)| b)
    }

    /// Apply a function to all scene banks (used by the parser).
    pub(crate) fn for_each_scene_bank(&mut self, mut f: impl FnMut(&mut SceneBank)) {
        {
            let mut guard = self.track_bank.scene_bank_mut();
            f(&mut guard);
        }
        for (_, bank) in &mut self.extra_scene_banks {
            f(bank);
        }
    }

    /// The selected slot of the cursor track.
    pub fn selected_slot(&self) -> Option<&Slot> {
        self.cursor_track_slot_bank().selected_item()
    }

    // ----------------------------------------------------------------------------------------
    // Devices

    pub fn cursor_device(&self) -> &SpecificDevice {
        &self.cursor_device
    }

    pub fn cursor_device_mut(&mut self) -> &mut SpecificDevice {
        &mut self.cursor_device
    }

    pub fn has_selected_device(&self) -> bool {
        self.cursor_device.does_exist()
    }

    /// Get a specific device (if enabled in the setup). `NiKomplete` returns the first
    /// instrument.
    pub fn specific_device(&self, id: DeviceId) -> Option<&SpecificDevice> {
        match id {
            DeviceId::FirstInstrument | DeviceId::NiKomplete => self.first_instrument.as_ref(),
            DeviceId::Eq => self.eq_device.as_ref(),
        }
    }

    pub fn specific_device_mut(&mut self, id: DeviceId) -> Option<&mut SpecificDevice> {
        match id {
            DeviceId::FirstInstrument | DeviceId::NiKomplete => self.first_instrument.as_mut(),
            DeviceId::Eq => self.eq_device.as_mut(),
        }
    }

    /// The main drum device (if enabled in the setup).
    pub fn drum_device(&self) -> Option<&SpecificDevice> {
        self.drum_device.as_ref()
    }

    pub fn drum_device_mut(&mut self) -> Option<&mut SpecificDevice> {
        self.drum_device.as_mut()
    }

    /// An additional drum device with the given number of pads per page (see
    /// `ModelSetup::additional_drum_devices`).
    pub fn drum_device_sized(&self, page_size: usize) -> Option<&SpecificDevice> {
        self.additional_drum_devices.iter().find(|(n, _)| *n == page_size).map(|(_, d)| d)
    }

    pub fn drum_device_sized_mut(&mut self, page_size: usize) -> Option<&mut SpecificDevice> {
        self.additional_drum_devices.iter_mut().find(|(n, _)| *n == page_size).map(|(_, d)| d)
    }

    /// Call the function for the main and all additional drum devices: they show the same drum
    /// machine with different page sizes.
    pub(crate) fn for_each_drum_device(&mut self, mut f: impl FnMut(&mut SpecificDevice)) {
        if let Some(device) = &mut self.drum_device {
            f(device);
        }
        for (_, device) in &mut self.additional_drum_devices {
            f(device);
        }
    }

    /// True if the selected track is the master track.
    pub fn is_cursor_device_on_master_track(&self) -> bool {
        self.track_bank.master().is_selected()
    }

    // ----------------------------------------------------------------------------------------
    // Clips

    /// Get (or create) the cursor clip with the given number of steps and rows.
    pub fn note_clip(&mut self, cols: usize, rows: usize) -> &mut CursorClip {
        if let Some(i) = self.cursor_clips.iter().position(|(k, _)| *k == (cols, rows)) {
            return &mut self.cursor_clips[i].1;
        }
        self.cursor_clips.push(((cols, rows), CursorClip::new(self.ctx.clone(), cols, rows)));
        &mut self.cursor_clips.last_mut().expect("just added").1
    }

    /// The note clip with the given grid ID ("<steps>x<rows>", see [`CursorClip::grid_id`]).
    pub fn note_clip_by_grid_id_mut(&mut self, id: &str) -> Option<&mut CursorClip> {
        let (cols, rows) = id.split_once('x')?;
        let key = (cols.parse().ok()?, rows.parse().ok()?);
        self.cursor_clips.iter_mut().find(|(k, _)| *k == key).map(|(_, c)| c)
    }

    /// Get the cursor clip with the given number of steps and rows if it was created.
    pub fn note_clip_ref(&self, cols: usize, rows: usize) -> Option<&CursorClip> {
        self.cursor_clips.iter().find(|(k, _)| *k == (cols, rows)).map(|(_, c)| c)
    }

    /// The first created cursor clip (former `getCursorClip`).
    pub fn cursor_clip(&self) -> Option<&CursorClip> {
        self.cursor_clips.first().map(|(_, c)| c)
    }

    pub fn cursor_clip_mut(&mut self) -> Option<&mut CursorClip> {
        self.cursor_clips.first_mut().map(|(_, c)| c)
    }

    /// Make sure that there is at least one cursor clip.
    pub fn ensure_clip(&mut self) {
        self.note_clip(0, 0);
    }

    /// Create a new clip on the track and select it. If `overdub` is set, recording is started.
    pub fn create_note_clip(&mut self, track: TrackRef, slot_index: usize, length_in_beats: i32, overdub: bool) {
        // ModelImpl.createNoteClip: track.createClip; slot.select (); slot.launch (true, false);
        // if (overdub) transport.setLauncherOverdub (true)
        let t = self.track_bank.track_mut(track);
        let position = t.slot_bank().item(slot_index).position();
        t.create_clip(position, length_in_beats);
        let slot = t.slot_bank_mut().item_mut(slot_index);
        slot.select();
        slot.launch(true, false);
        if overdub {
            self.set_launcher_overdub(true);
        }
    }

    /// Record a new clip in the slot of the track (ModelImpl.recordNoteClip: start recording if
    /// the slot does not record yet, then launch it).
    pub fn record_note_clip(&mut self, track: TrackRef, slot_index: usize) {
        let slot = self.track_bank.track_mut(track).slot_bank_mut().item_mut(slot_index);
        if !slot.is_recording() {
            slot.start_recording();
        }
        slot.launch(true, false);
    }

    pub fn can_convert_clip(&self) -> bool {
        true
    }

    // ----------------------------------------------------------------------------------------
    // Functions which combine several parts of the model

    /// True if the transport records or the launcher overdub is active.
    pub fn has_recording_state(&self) -> bool {
        self.transport.is_recording() || self.is_launcher_overdub() || self.track_bank.is_clip_recording()
    }

    /// True if the selected track can hold notes.
    pub fn can_selected_track_hold_notes(&self) -> bool {
        self.track_bank.selected_item().map(|t| t.can_hold_notes()).unwrap_or(false)
    }

    /// True if overdub is enabled on the selected track (former `ITransport.isLauncherOverdub`).
    pub fn is_launcher_overdub(&self) -> bool {
        if let Some((_, launcher)) = self.transport.host_overdub() {
            return launcher;
        }
        self.track_bank.selected_item().map(|t| t.is_overdub()).unwrap_or(false)
    }

    /// Same as [`Model::is_launcher_overdub`] (Reaper); the arranger overdub switch on Bitwig.
    pub fn is_arranger_overdub(&self) -> bool {
        if let Some((arranger, _)) = self.transport.host_overdub() {
            return arranger;
        }
        self.is_launcher_overdub()
    }

    /// Enable overdub on the selected track (Reaper), the clip launcher overdub (Bitwig).
    pub fn set_launcher_overdub(&mut self, on: bool) {
        if self.transport.host_overdub().is_some() {
            self.transport.send_overdub(true, on);
            return;
        }
        if let Some(track) = self.track_bank.selected_item() {
            let cmd = format!("{}/overdub", track.position());
            self.ctx.send_bool(Processor::Track, &cmd, on);
        }
    }

    pub fn toggle_launcher_overdub(&mut self) {
        let on = !self.is_launcher_overdub();
        self.set_launcher_overdub(on);
    }

    /// Same as [`Model::toggle_launcher_overdub`] (former `ITransport.toggleOverdub`); the
    /// arranger overdub switch on Bitwig.
    pub fn toggle_overdub(&mut self) {
        if let Some((arranger, _)) = self.transport.host_overdub() {
            self.transport.send_overdub(false, !arranger);
            return;
        }
        self.toggle_launcher_overdub();
    }

    /// The crossfader (user parameter 0 of the master track).
    pub fn crossfade_parameter(&self) -> Option<&Parameter> {
        self.track_bank.master().crossfader_parameter()
    }

    pub fn crossfade_parameter_mut(&mut self) -> Option<&mut Parameter> {
        self.track_bank.master_mut().crossfader_parameter_mut()
    }

    /// The crossfader value (center if it does not exist).
    pub fn crossfade(&self) -> i32 {
        match self.crossfade_parameter() {
            Some(p) if p.does_exist() => p.value(),
            _ => self.value_changer().upper_bound() / 2,
        }
    }

    pub fn set_crossfade(&mut self, value: i32) {
        if let Some(p) = self.crossfade_parameter_mut() {
            p.set_value(value);
        }
    }

    pub fn change_crossfade(&mut self, control: i32) {
        if let Some(p) = self.crossfade_parameter_mut() {
            p.change_value(control);
        }
    }

    // ----------------------------------------------------------------------------------------
    // Internal updates (parser)

    pub(crate) fn for_each_cursor_clip(&mut self, mut f: impl FnMut(&mut CursorClip)) {
        for (_, clip) in &mut self.cursor_clips {
            f(clip);
        }
    }

    pub fn set_cursor_clip_notes(&mut self, notes: Vec<Note>) {
        self.for_each_cursor_clip(|c| c.set_notes(notes.clone()));
    }

    pub fn set_cursor_clip_exists(&mut self, exists: bool) {
        self.for_each_cursor_clip(|c| c.set_exists_value(exists));
    }

    pub fn set_cursor_clip_play_start(&mut self, start: f64) {
        self.for_each_cursor_clip(|c| c.set_play_start_intern(start));
    }

    pub fn set_cursor_clip_play_end(&mut self, end: f64) {
        self.for_each_cursor_clip(|c| c.set_play_end_intern(end));
    }

    pub fn set_cursor_clip_play_position(&mut self, position: f64) {
        self.for_each_cursor_clip(|c| c.set_play_position(position));
    }

    pub fn set_cursor_clip_color_value(&mut self, color: [f64; 3]) {
        self.for_each_cursor_clip(|c| c.set_color_value(color));
    }

    pub fn set_cursor_clip_loop_is_enabled(&mut self, enabled: bool) {
        self.for_each_cursor_clip(|c| c.set_loop_enabled_state(enabled));
    }

    /// Set all clips (slots) of all tracks. Format:
    /// `trackIndex;numClips;name;isSelected;color;isMuted;...;trackIndex;...`.
    pub fn set_clips(&mut self, clips_str: &str) {
        let parts: Vec<&str> = clips_str.trim().split(';').collect();
        let mut pos = 0;
        let mut max_slot_count = 0;
        let int_at = |parts: &[&str], i: usize| -> Option<i64> { parts.get(i).and_then(|s| s.trim().parse::<i64>().ok()) };
        while pos < parts.len() {
            let Some(track_index) = int_at(&parts, pos) else {
                break;
            };
            pos += 1;
            let Some(num_clips) = int_at(&parts, pos) else {
                break;
            };
            pos += 1;
            if track_index < 0 || num_clips < 0 {
                break;
            }
            let num_clips = num_clips as usize;
            max_slot_count = max_slot_count.max(num_clips);
            let mut slots = Vec::with_capacity(num_clips);
            for _ in 0..num_clips {
                if pos + 4 > parts.len() {
                    break;
                }
                let name = parts[pos].to_string();
                let is_selected = int_at(&parts, pos + 1).unwrap_or(0) > 0;
                let color = self.parse_color(parts[pos + 2]);
                let is_muted = int_at(&parts, pos + 3).unwrap_or(0) > 0;
                pos += 4;
                slots.push((name, is_selected, color, is_muted));
            }
            let slot_bank = self.track_bank.unpaged_item_mut(track_index as usize).slot_bank_mut();
            slot_bank.set_item_count(num_clips);
            for (i, (name, is_selected, color, is_muted)) in slots.into_iter().enumerate() {
                let slot = slot_bank.unpaged_item_mut(i);
                slot.set_position(i as i32);
                slot.set_selected(is_selected);
                slot.set_muted(is_muted);
                slot.set_internal_name(&name);
                if let Some(c) = color {
                    slot.set_color_state(ColorEx::from_array(c));
                }
                slot.set_exists(true);
                slot.set_has_content(true);
            }
        }

        // Set all slot banks to the same size
        let size = self.track_bank.track_count();
        for i in 0..size {
            self.track_bank.unpaged_item_mut(i).slot_bank_mut().set_max_slot_count(max_slot_count);
        }
    }

    /// Parse three integer values (0..255) separated by a space. Returns None for negative
    /// values (no color) or on error.
    pub fn parse_color(&self, value: &str) -> Option<[f64; 3]> {
        let values: Vec<&str> = value.split(' ').collect();
        if values.len() != 3 {
            self.host().error(&format!("Color: Wrong number of arguments: {}", values.len()));
            self.host().error(&values.iter().map(|v| format!("{v}:")).collect::<String>());
            return None;
        }
        let parse = |s: &str| s.trim().parse::<f64>().ok();
        let (Some(r), Some(g), Some(b)) = (parse(values[0]), parse(values[1]), parse(values[2])) else {
            self.host().error(&format!("Color: Could not parse: {value}"));
            return None;
        };
        if r < 0.0 {
            return None;
        }
        Some([r / 255.0, g / 255.0, b / 255.0])
    }
}
