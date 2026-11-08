// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The context which is shared by all parts of the DAW model (former `DataSetupEx` + `BaseImpl`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use crate::backend::Sender;
use crate::framework::value_changer::ValueChanger;
use crate::host::{Host, Scheduler};
use crate::protocol::Cmd;

use super::constants::RecordQuantization;
use super::model::Model;
use super::events::ModelEvent;

/// State which is shared between all model items. Only used internally for state which is
/// required by item command methods but owned by another part of the model (e.g. whether
/// automation is currently written, which influences track commands).
/// A task which is executed with access to the whole model.
pub type ModelTask = Box<dyn FnOnce(&mut Model)>;

struct Shared {
    automation_rec_active: Cell<bool>,
    cursor_track_position: Cell<i32>,
    pinned_track: Cell<Option<i32>>,
    transport: Cell<TransportSnapshot>,
    scene_ranges: RefCell<Vec<(bool, f64, f64)>>,
    tasks: RefCell<Scheduler<Model>>,
    events: RefCell<Vec<ModelEvent>>,
    /// The host pages the banks (Bitwig, protocol v2 host paging).
    host_paging: Cell<bool>,
    /// The command forms the host reported with `/host/features` (bit set of [`HostFeature`]),
    /// None if it did not report them.
    host_features: Cell<Option<u32>>,
    /// The host reports the VU hold values (Reaper: `vuholdleft|vuholdright` in dB).
    host_vu_hold: Cell<bool>,
    /// The global record quantization of the host (Bitwig: note length, grid), None if the host
    /// has it per track (Reaper).
    host_record_quantization: Cell<Option<(bool, RecordQuantization)>>,
}

/// Cheap to clone context which every model item holds to send commands to the backend.
#[derive(Clone)]
pub struct DawContext {
    sender: Sender,
    host: Host,
    value_changer: ValueChanger,
    shared: Rc<Shared>,
    muted: bool,
}

/// The transport state which is required by clip slots to calculate their playing state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TransportSnapshot {
    pub is_playing: bool,
    pub is_recording: bool,
    pub position: f64,
}

/// A command form which the host understands in addition to the basic (Reaper) commands.
///
/// - **Command features** ([`HostFeature::is_command_feature`]: the `feature =` of the commands in
///   the protocol table): a host on the binary command form (`dbm_set_command_callback`) has
///   those whose commands it handles (the handled bitset, [`command_features`]). A text host
///   reports them with `/host/features <names>`.
/// - **Value operation features** (`valueInc`, `valueReset`, `immediately`, `indication`): the
///   operations of the value commands (the `op` of the record, not a command of its own), so the
///   bitset of the command IDs cannot express them; every host reports them with
///   `/host/features` (see HOST_BRIDGE_GUIDE.md).
///
/// Features which the core detects from the host's own reports (host paging from
/// `/track/offset`, the automation and overdub switches, the pinned track, ...) are no host
/// features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFeature {
    /// Relative value changes `<value command>/inc <normalized delta>` (Bitwig
    /// `SettableRangedValue.inc`); the host's values are also shown while a value is touched.
    ValueInc,
    /// `<value command>/reset`: the host's default value (Bitwig `parameter.reset ()`).
    ValueReset,
    /// `<value command>/immediately <value>`: set a value without the host's takeover mode.
    Immediately,
    /// `<value command>/indicate` for volume, panorama and sends, and `track indicate`.
    Indication,
    /// `track|master <p>/solo/toggle` (the host's exclusive solo preference).
    SoloToggle,
    /// `transport play|stop|restart|stopAndRewind|togglePunchIn|togglePunchOut|toggleFill|
    /// toggleMetronomeTicks`, `transport punchIn|punchOut|fill|metronomeTicks <0|1>`,
    /// `transport metronomeVolume[/inc]`.
    TransportCommands,
    /// Commands to the device itself instead of `device <index>/…`: `device|primary|drum|eq
    /// window/toggle|expand/toggle|…`, `device sibling/<i>/…`, `drum|primary pad/<p>/select`.
    DeviceCommands,
    /// `application layout/-|+` (previous/next panel layout of the host).
    LayoutStep,
    /// The clip launcher navigator of the host (`navigator selectTrack <position>`,
    /// `navigator tracks|clips|scenes/-|+`), as `ClipLauncherNavigatorImpl` of the Java version
    /// for Bitwig: the host keeps the selected slot when another track is selected. Without it
    /// the core moves the slot itself (the Reaper version).
    ClipLauncherNavigator,
}

impl HostFeature {
    /// All features.
    pub const ALL: [HostFeature; 9] = [
        HostFeature::ValueInc,
        HostFeature::ValueReset,
        HostFeature::Immediately,
        HostFeature::Indication,
        HostFeature::SoloToggle,
        HostFeature::TransportCommands,
        HostFeature::DeviceCommands,
        HostFeature::LayoutStep,
        HostFeature::ClipLauncherNavigator,
    ];

    /// The name of the feature in `/host/features`.
    pub fn name(self) -> &'static str {
        match self {
            HostFeature::ValueInc => "valueInc",
            HostFeature::ValueReset => "valueReset",
            HostFeature::Immediately => "immediately",
            HostFeature::Indication => "indication",
            HostFeature::SoloToggle => "soloToggle",
            HostFeature::TransportCommands => "transportCommands",
            HostFeature::DeviceCommands => "deviceCommands",
            HostFeature::LayoutStep => "layoutStep",
            HostFeature::ClipLauncherNavigator => "clipLauncherNavigator",
        }
    }

    /// The feature with the given name, None if unknown.
    pub fn lookup(name: &str) -> Option<HostFeature> {
        HostFeature::ALL.into_iter().find(|f| f.name() == name)
    }

    pub(crate) fn bit(self) -> u32 {
        1 << (self as u32)
    }

    /// True for the features of command forms (the `feature =` of commands in the protocol
    /// table, a test keeps this in step): a binary host has them if it handles their commands.
    /// The others are value operations, reported with `/host/features` by every host.
    pub fn is_command_feature(self) -> bool {
        matches!(self, HostFeature::SoloToggle | HostFeature::TransportCommands | HostFeature::DeviceCommands | HostFeature::LayoutStep | HostFeature::ClipLauncherNavigator)
    }
}

/// The command features (bit set of [`HostFeature`]) of a host which handles the commands of the
/// bitset (word i bit j = ID 0x8000 + 64*i + j; empty: all commands): a feature is supported if
/// the host handles at least one of its commands. The commands of a feature are alternative forms
/// of the basic commands, and a host handles those its DAW has (zynthian: `transport play`, but no
/// punch or fill), as `/host/features` named a feature for the forms a host knows.
pub fn command_features(handled: &[u64]) -> u32 {
    let is_handled = |id: u16| {
        let bit = usize::from(id & 0x7FFF);
        handled.is_empty() || handled.get(bit / 64).is_some_and(|word| word & (1 << (bit % 64)) != 0)
    };
    crate::protocol::COMMANDS.iter().filter(|info| is_handled(info.id as u16)).filter_map(|info| info.feature).fold(0, |set, f| set | f.bit())
}

/// Marks the master track in places where a track position is stored.
pub(crate) const MASTER_POSITION: i32 = i32::MIN;

impl DawContext {
    pub fn new(host: Host, sender: Sender, value_changer: ValueChanger) -> Self {
        DawContext {
            sender,
            host,
            value_changer,
            shared: Rc::new(Shared {
                automation_rec_active: Cell::new(false),
                cursor_track_position: Cell::new(-1),
                pinned_track: Cell::new(None),
                transport: Cell::new(TransportSnapshot::default()),
                scene_ranges: RefCell::new(Vec::new()),
                tasks: RefCell::new(Scheduler::default()),
                events: RefCell::new(Vec::new()),
                host_paging: Cell::new(false),
                host_features: Cell::new(None),
                host_vu_hold: Cell::new(false),
                host_record_quantization: Cell::new(None),
            }),
            muted: false,
        }
    }

    /// True if the host pages the banks (Bitwig, protocol v2): the banks then behave as in the
    /// Java version for Bitwig, e.g. selecting the previous page selects its first item (Reaper:
    /// its last item).
    pub fn is_host_paging(&self) -> bool {
        self.shared.host_paging.get()
    }

    pub(crate) fn set_host_paging(&self) {
        self.shared.host_paging.set(true);
    }

    /// True if the host understands the command form of the feature. A command feature of a
    /// host on the binary command form comes from the commands it handles ([`command_features`],
    /// `Backend::command_features`); a command feature in its `/host/features` is ignored.
    /// Otherwise: a host which reported `/host/features` supports exactly the listed ones. For
    /// compatibility a host which pages (`/track/offset`) but never reported its features supports
    /// all of them (the Bitwig extension before `/host/features`); Reaper reports neither and
    /// supports none.
    pub fn host_supports(&self, feature: HostFeature) -> bool {
        if feature.is_command_feature() {
            if let Some(features) = self.sender.backend().command_features() {
                return features & feature.bit() != 0;
            }
        }
        match self.shared.host_features.get() {
            Some(features) => features & feature.bit() != 0,
            None => self.is_host_paging(),
        }
    }

    /// The host reported its features (`/host/features <space separated names>`); unknown names
    /// are ignored. Replaces earlier reports.
    pub(crate) fn set_host_features(&self, names: &str) {
        let features = names.split_whitespace().filter_map(HostFeature::lookup).fold(0, |set, f| set | f.bit());
        self.shared.host_features.set(Some(features));
    }

    /// True if the host reports the VU hold values (Reaper): the clip state of a channel is then
    /// its hold value above 0 dB, otherwise its VU above the clip border (Bitwig).
    pub fn host_reports_vu_hold(&self) -> bool {
        self.shared.host_vu_hold.get()
    }

    pub(crate) fn set_host_reports_vu_hold(&self) {
        self.shared.host_vu_hold.set(true);
    }

    /// The global record quantization of the host (note length, grid), if it has one (Bitwig:
    /// ApplicationImpl `recordQuantizeNoteLength ()`, `recordQuantizationGrid ()`).
    pub fn host_record_quantization(&self) -> Option<(bool, RecordQuantization)> {
        self.shared.host_record_quantization.get()
    }

    pub(crate) fn update_host_record_quantization(&self, note_length: Option<bool>, grid: Option<RecordQuantization>) {
        let (old_length, old_grid) = self.shared.host_record_quantization.get().unwrap_or_default();
        self.shared.host_record_quantization.set(Some((note_length.unwrap_or(old_length), grid.unwrap_or(old_grid))));
    }

    /// A copy of the context which does not send anything to the backend, does not schedule
    /// tasks and does not post events. Used for the "empty" placeholder items of the banks.
    pub fn muted(&self) -> Self {
        let mut ctx = self.clone();
        ctx.muted = true;
        ctx
    }

    /// True if this is the context of an "empty" placeholder item.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn sender(&self) -> &Sender {
        &self.sender
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn value_changer(&self) -> &ValueChanger {
        &self.value_changer
    }

    /// True if the transport currently writes arranger automation.
    pub fn is_automation_rec_active(&self) -> bool {
        self.shared.automation_rec_active.get()
    }

    pub(crate) fn set_automation_rec_active(&self, active: bool) {
        self.shared.automation_rec_active.set(active);
    }

    /// The position of the pinned or selected track (-1 if none, [`MASTER_POSITION`] for the
    /// master track). Used by parameters of the track processor.
    pub(crate) fn cursor_track_position(&self) -> i32 {
        self.shared.cursor_track_position.get()
    }

    pub(crate) fn set_cursor_track_position(&self, position: i32) {
        self.shared.cursor_track_position.set(position);
    }

    /// The position of the pinned track if the cursor track is pinned.
    pub(crate) fn pinned_track(&self) -> Option<i32> {
        self.shared.pinned_track.get()
    }

    pub(crate) fn set_pinned_track(&self, position: Option<i32>) {
        self.shared.pinned_track.set(position);
    }

    pub(crate) fn transport_snapshot(&self) -> TransportSnapshot {
        self.shared.transport.get()
    }

    pub(crate) fn update_transport_snapshot(&self, f: impl FnOnce(&mut TransportSnapshot)) {
        let mut snapshot = self.shared.transport.get();
        f(&mut snapshot);
        self.shared.transport.set(snapshot);
    }

    /// Get the (exists, begin, end) range of the scene at the given position.
    pub(crate) fn scene_range(&self, position: i32) -> Option<(bool, f64, f64)> {
        if position < 0 {
            return None;
        }
        self.shared.scene_ranges.borrow().get(position as usize).copied()
    }

    pub(crate) fn update_scene_range(&self, position: i32, f: impl FnOnce(&mut (bool, f64, f64))) {
        if position < 0 || self.muted {
            return;
        }
        let mut ranges = self.shared.scene_ranges.borrow_mut();
        let position = position as usize;
        if ranges.len() <= position {
            ranges.resize(position + 1, (false, 0.0, 0.0));
        }
        f(&mut ranges[position]);
    }

    /// Schedule a task which gets access to the whole model after the given delay (former
    /// `host.scheduleTask`). Executed by [`Model::run_due_tasks`].
    pub fn schedule(&self, delay_ms: u64, task: impl FnOnce(&mut Model) + 'static) {
        if self.muted {
            return;
        }
        self.shared.tasks.borrow_mut().schedule(self.host.now(), delay_ms, task);
    }

    pub(crate) fn take_due_tasks(&self, now: Instant) -> Vec<ModelTask> {
        self.shared.tasks.borrow_mut().take_due(now)
    }

    pub(crate) fn take_all_tasks(&self) -> Vec<ModelTask> {
        self.shared.tasks.borrow_mut().take_all()
    }

    pub(crate) fn has_tasks(&self) -> bool {
        !self.shared.tasks.borrow().is_empty()
    }

    /// Record an event for the controller layer.
    pub fn post_event(&self, event: ModelEvent) {
        if self.muted {
            return;
        }
        self.shared.events.borrow_mut().push(event);
    }

    pub(crate) fn take_events(&self) -> Vec<ModelEvent> {
        std::mem::take(&mut *self.shared.events.borrow_mut())
    }

    /// Send a command to the host (former `BaseImpl.sendOSC`); nothing for the "empty"
    /// placeholder items ([`DawContext::muted`]).
    pub fn send(&self, cmd: Cmd) {
        if !self.muted {
            self.sender.send(&cmd);
        }
    }
}
