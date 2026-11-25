// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The host's parameter pages as a page source (doc/HOST_PAGE_PLAN.md §3.4; not part of Java):
//! the pages which the host reports (`/hostpage/…`, [`HostPage`]) in the same core instance, and
//! the surface's input sent straight to the host as `hostpage …` commands.
//!
//! - **Available** while the host supports [`HostFeature::HostPages`] (a binary host which
//!   handles the commands; never a text host). It does not depend on reports: the host reports
//!   its pages only while a surface shows them (`Input::Shown` → `HostPageShow`).
//! - **The selection is the host's** (§D.4): page, row, control set and selected cell are what
//!   the host reports. The input which selects becomes a command; the core never changes the
//!   selection itself, so there is nothing to echo.
//! - **Epochs:** the cell commands carry the host's epoch of the shown cells, so that the host
//!   drops a turn meant for the cells before a change.

use super::{Capabilities, ControlSet, Input, LinkState, PageModel, PageSource, Rgb, Section, Slot, SlotKind, SourceEvent, TurnUnit};
use crate::daw::context::HostFeature;
use crate::daw::host_page::HostPage;
use crate::daw::model::Model;
use crate::protocol::{Cmd, Resolution};

/// The name of the source while the host reports none.
const DEFAULT_NAME: &str = "Host";

/// The host's pages as a source.
pub struct HostSource {
    state: LinkState,
    name: String,
    /// The model the surface has.
    mirror: PageModel,
    /// The host page's generation of the mirror.
    generation: Option<u64>,
    epoch: usize,
    /// The input until the next poll.
    queue: Vec<Input>,
    /// The shape of the surface for `HostPageShow`: columns, rows, encoders of a row, rows of a
    /// control set.
    shape: [usize; 4],
}

impl HostSource {
    /// A source for a surface with these capabilities.
    pub fn new(capabilities: Capabilities) -> Self {
        let (columns, rows) = capabilities.slot_area;
        let encoders = capabilities.row_encoders;
        let rows_per_set = if columns > 0 && encoders > columns { encoders.div_ceil(columns) } else { 1 };
        HostSource { state: LinkState::Missing, name: DEFAULT_NAME.to_string(), mirror: PageModel::default(), generation: None, epoch: 0, queue: Vec::new(), shape: [columns, rows, encoders, rows_per_set] }
    }

    /// The command of an input, None if the host has none for it.
    fn command(&self, input: Input) -> Option<Cmd<'static>> {
        let model = &self.mirror;
        let epoch = self.epoch;
        let turn = |cell: usize, delta: i32, fine: bool, unit: TurnUnit| Cmd::HostPageCellTurn {
            cell,
            epoch,
            unit: match unit {
                TurnUnit::Travel => crate::protocol::TurnUnit::Travel,
                TurnUnit::Detent => crate::protocol::TurnUnit::Detent,
            },
            resolution: if fine { Resolution::Fine } else { Resolution::Normal },
            delta,
        };
        let step = |current: Option<usize>, delta: i32, count: usize| -> Option<i32> {
            let count = count as i32;
            (count > 0).then(|| (current.map_or(if delta > 0 { -1 } else { count }, |c| c as i32) + delta).clamp(0, count - 1))
        };
        let [columns, rows, encoders, rows_per_set] = self.shape;
        Some(match input {
            Input::Shown { on } => Cmd::HostPageShow { columns, rows, encoders, rows_per_set, value: on },
            Input::SlotTurn { id, delta, fine, unit } => turn(id, delta, fine, unit),
            Input::SelectedTurn { delta, fine, unit } => turn(model.selected_slot?, delta, fine, unit),
            Input::SlotPress { id } => Cmd::HostPageCellPress { cell: id, epoch, value: true },
            Input::SlotRelease { id } => Cmd::HostPageCellPress { cell: id, epoch, value: false },
            Input::SlotTouch { id, on } => Cmd::HostPageCellTouch { cell: id, epoch, value: on },
            Input::SelectedReset => Cmd::HostPageCellReset { cell: model.selected_slot?, epoch },
            Input::RowSelect { row } => Cmd::HostPageRow { row: row as i32 },
            Input::PageSelect { page } => Cmd::HostPagePage { page: page as i32 },
            Input::PageNext | Input::PagePrev => {
                let delta = if input == Input::PageNext { 1 } else { -1 };
                let page = step(Some(model.page), delta, model.pages.len())?;
                if page as usize == model.page {
                    return None;
                }
                Cmd::HostPagePage { page }
            }
            Input::SetSelect { set } => Cmd::HostPageSet { set: set as i32 },
            Input::SetStep { delta } => {
                let set = step(model.active_set, delta, model.sets.len())?;
                if Some(set as usize) == model.active_set {
                    return None;
                }
                Cmd::HostPageSet { set }
            }
            Input::SectionSelect { section } => Cmd::HostPageSection { section: section as i32 },
            Input::SlotAssign { id, slot } => Cmd::HostPageCellAssign { cell: id, epoch, slot },
            Input::ValueNext | Input::ValuePrev | Input::ExtraTurn { .. } => return None,
        })
    }
}

/// An RGB color of the host (0..1), black for none.
fn rgb(color: Option<[f64; 3]>) -> Rgb {
    let channel = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    color.map_or(Rgb::BLACK, |[r, g, b]| Rgb::new(channel(r), channel(g), channel(b)))
}

/// An index of the host (-1: none) if it is below `count`.
fn index(value: i32, count: usize) -> Option<usize> {
    usize::try_from(value).ok().filter(|v| *v < count)
}

/// The page model of the host's pages.
pub fn page_model(page: &HostPage) -> PageModel {
    let (columns, rows) = (page.columns, page.rows);
    let mut model = PageModel { columns, rows, rows_per_set: 1, slots: vec![Slot::default(); columns * rows], ..PageModel::default() };
    if !page.exists {
        return model;
    }
    model.pages = page.pages.clone();
    model.page = index(page.page, model.pages.len()).unwrap_or(0);
    model.sections = page.sections.iter().map(|s| Section { name: s.name.clone(), color: rgb(s.color) }).collect();
    model.sets = page.sets.iter().map(|s| ControlSet { name: s.name.clone(), section: index(s.section, model.sections.len()), color: rgb(s.color) }).collect();
    for (slot, cell) in model.slots.iter_mut().zip(&page.cells) {
        let kind = match cell.kind {
            1 => SlotKind::Fader,
            2 => SlotKind::List,
            3 | 4 | 6 => SlotKind::Pad,
            5 => SlotKind::Relative,
            _ => SlotKind::Empty,
        };
        *slot = Slot {
            kind,
            name: cell.name.clone(),
            color: rgb(cell.color),
            value: cell.value.clamp(0.0, 1.0) as f32,
            value_text: cell.text.clone(),
            steps: cell.steps.max(0) as u32,
            on: cell.on,
            enabled: cell.enabled && kind != SlotKind::Empty,
            set: index(cell.set, model.sets.len()),
        };
    }
    model.active_set = index(page.set, model.sets.len());
    model.active_section = index(page.section, model.sections.len());
    model.active_row = index(page.row, rows).unwrap_or(0);
    model.selected_slot = index(page.selected_cell, model.slots.len());
    model
}

impl PageSource for HostSource {
    fn state(&self) -> LinkState {
        self.state
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn poll(&mut self, daw: &Model) -> Vec<SourceEvent> {
        let mut events = Vec::new();
        let state = if daw.ctx().host_supports(HostFeature::HostPages) { LinkState::Attached } else { LinkState::Missing };
        if state != self.state {
            self.state = state;
            // The surface starts from an empty model on every state change
            self.mirror = PageModel::default();
            self.generation = None;
            events.push(SourceEvent::State(state));
        }
        if state != LinkState::Attached {
            self.queue.clear();
            return events;
        }
        let page = daw.host_page();
        if self.generation != Some(page.generation()) {
            self.generation = Some(page.generation());
            let model = page_model(page);
            events.extend(model.diff(&self.mirror).into_iter().map(SourceEvent::Model));
            self.mirror = model;
            self.epoch = (page.epoch as u32 & 0xFFFF) as usize;
            self.name = if page.title.is_empty() { DEFAULT_NAME.to_string() } else { page.title.clone() };
        }
        for input in std::mem::take(&mut self.queue) {
            if let Some(cmd) = self.command(input) {
                daw.ctx().send(cmd);
            }
        }
        events
    }

    fn send(&mut self, input: Input) {
        if self.state == LinkState::Attached {
            self.queue.push(input);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ModelChange;
    use super::*;
    use crate::backend::{RecordedCalls, RecordingBackend, Sender};
    use crate::daw::model_setup::ModelSetup;
    use crate::framework::value_changer::ValueChanger;
    use crate::host::{Host, MemoryLog};
    use crate::scale::Scales;
    use std::rc::Rc;

    fn daw(host_pages: bool) -> (Model, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        if host_pages {
            *backend.handled.borrow_mut() = Some(Vec::new());
        }
        let vc = ValueChanger::twos_complement(128, 1);
        let scales = Scales::new(&vc, 36, 100, 8, 8);
        (Model::new(Host::new(Rc::new(MemoryLog::default())), Sender::new(backend.clone()), ModelSetup::default(), vc, scales), backend)
    }

    fn jam() -> Capabilities {
        Capabilities { slot_area: (8, 6), row_encoders: 8, ..Capabilities::default() }
    }

    #[test]
    fn needs_the_feature() {
        let (model, backend) = daw(false);
        let mut source = HostSource::new(jam());
        assert!(source.poll(&model).is_empty());
        assert_eq!(source.state(), LinkState::Missing);
        source.send(Input::Shown { on: true });
        source.poll(&model);
        assert!(backend.take_commands().is_empty());
    }

    #[test]
    fn model_and_commands() {
        let (mut model, backend) = daw(true);
        let mut source = HostSource::new(Capabilities { slot_area: (6, 6), row_encoders: 12, ..Capabilities::default() });
        // Attached, with the empty page of the host's default size
        let events = source.poll(&model);
        assert_eq!(events[0], SourceEvent::State(LinkState::Attached));
        assert!(matches!(&events[1..], [SourceEvent::Model(ModelChange::Snapshot(m))] if m.columns == 8 && m.pages.is_empty()));
        source.send(Input::Shown { on: true });
        source.poll(&model);
        assert_eq!(backend.take_commands().commands().collect::<Vec<_>>(), [Cmd::HostPageShow { columns: 6, rows: 6, encoders: 12, rows_per_set: 2, value: true }]);

        // A page: 4x2, two sets, a toggle and a trigger
        let page = model.host_page_mut();
        page.exists = true;
        page.title = "zynthian · Synth".into();
        page.epoch = 0x1_0005;
        page.set_columns(4);
        page.set_rows(2);
        page.set_page_count(3);
        page.page = 1;
        page.set_set_count(2);
        page.set = 1;
        page.row = 1;
        page.selected_cell = 5;
        for (cell, kind) in [(0, 1), (1, 3), (2, 4), (5, 2), (6, 9)] {
            let c = page.cell_mut(cell).unwrap();
            c.kind = kind;
            c.enabled = true;
            c.value = 2.0;
            c.set = if cell < 4 { 0 } else { 1 };
            c.color = Some([1.0, 0.5, 0.0]);
        }
        let events = source.poll(&model);
        assert!(matches!(events.as_slice(), [SourceEvent::Model(ModelChange::Snapshot(_))]), "{events:?}");
        assert_eq!(source.name(), "zynthian · Synth");
        let built = page_model(model.host_page());
        assert_eq!((built.columns, built.rows, built.slots.len(), built.page, built.active_set, built.active_row, built.selected_slot), (4, 2, 8, 1, Some(1), 1, Some(5)));
        assert_eq!(built.slots.iter().map(|s| s.kind).collect::<Vec<_>>()[..7], [SlotKind::Fader, SlotKind::Pad, SlotKind::Pad, SlotKind::Empty, SlotKind::Empty, SlotKind::List, SlotKind::Empty]);
        assert_eq!((built.slots[0].value, built.slots[0].color, built.slots[6].enabled), (1.0, Rgb::new(255, 128, 0), false));
        // Nothing changed: nothing new
        assert!(source.poll(&model).is_empty());

        // The input: the epoch of the cells (16 bits), steps from the reported selection
        for input in [
            Input::SelectedTurn { delta: -2, fine: true, unit: TurnUnit::Detent },
            Input::SetStep { delta: 1 },
            Input::SetStep { delta: -1 },
            Input::PageNext,
            Input::PagePrev,
            Input::SelectedReset,
            Input::ValueNext,
            Input::Shown { on: false },
        ] {
            source.send(input);
        }
        source.poll(&model);
        assert_eq!(
            backend.take_commands().commands().collect::<Vec<_>>(),
            [
                Cmd::HostPageCellTurn { cell: 5, epoch: 5, unit: crate::protocol::TurnUnit::Detent, resolution: Resolution::Fine, delta: -2 },
                Cmd::HostPageSet { set: 0 },
                Cmd::HostPagePage { page: 2 },
                Cmd::HostPagePage { page: 0 },
                Cmd::HostPageCellReset { cell: 5, epoch: 5 },
                Cmd::HostPageShow { columns: 6, rows: 6, encoders: 12, rows_per_set: 2, value: false },
            ]
        );
        // Without a page: an empty model of the host's size
        model.host_page_mut().exists = false;
        let events = source.poll(&model);
        assert!(matches!(events.as_slice(), [SourceEvent::Model(ModelChange::Snapshot(m))] if m.pages.is_empty() && m.slots.len() == 8));
    }
}
