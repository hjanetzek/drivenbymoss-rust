// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The surface side as a reusable helper: the sources of the pages (each with its mirror of the
//! model), the current one, and the mapping of the controller (which pads are cells, page and
//! section selectors, what the encoders do). It is shared by [`super::view::PageSurfaceView`]
//! (the grid) and [`super::view::PageSurfaceMode`] (the encoders), and polled by the controller at
//! its tick.
//!
//! The selection (page, row, control set, selected cell) belongs to the source: the client only
//! sends the input and shows what the source reports (doc/HOST_PAGE_PLAN.md §D.4).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::{Capabilities, Input, LinkState, PageModel, PageSource, SourceEvent, TurnUnit, GRID_SIZE};
use crate::daw::model::Model;
use crate::framework::value_changer::{RelativeEncoding, ValueChanger};

/// The function of a pad of the controller's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadFunction {
    None,
    /// The cell at the row and column of the page (if the page has it).
    Cell { row: usize, column: usize },
    /// Select a row (0..5).
    Row(usize),
    /// The n-th entry of the page list; the list shows the window of pages around the current
    /// one ([`SurfaceClient::page_offset`]).
    Page(usize),
    PagePrev,
    PageNext,
    /// Select a section of the source.
    Section(usize),
}

/// The function of an encoder (or fader / touch strip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderFunction {
    None,
    /// The slot of the active row in this column.
    RowSlot(usize),
    /// The selected slot.
    Selected { fine: bool },
    /// An extra value of the source.
    Extra(usize),
    /// The slot of the active row in this column if the page is that wide, else the selected
    /// slot (the JAM's strip 7: 8-wide pages vs the Electra's 6).
    RowSlotOrSelected { column: usize, fine: bool },
    /// The slot of the active row in this column if the page is that wide, else an extra.
    RowSlotOrExtra { column: usize, index: usize },
}

impl EncoderFunction {
    /// The function for a model: the alternatives resolved by its width.
    pub fn resolve(self, model: &PageModel) -> EncoderFunction {
        match self {
            EncoderFunction::RowSlotOrSelected { column, fine } => {
                if column < model.columns {
                    EncoderFunction::RowSlot(column)
                } else {
                    EncoderFunction::Selected { fine }
                }
            }
            EncoderFunction::RowSlotOrExtra { column, index } => {
                if column < model.columns {
                    EncoderFunction::RowSlot(column)
                } else {
                    EncoderFunction::Extra(index)
                }
            }
            other => other,
        }
    }

    fn turns_row_slots(self) -> bool {
        matches!(self, EncoderFunction::RowSlot(_) | EncoderFunction::RowSlotOrSelected { .. } | EncoderFunction::RowSlotOrExtra { .. })
    }
}

/// How the encoders send their values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderKind {
    /// Relative values in this encoding.
    Relative(RelativeEncoding),
    /// Absolute positions 0..127 (faders, touch strips): the difference between two positions
    /// is the delta.
    Absolute,
}

impl EncoderKind {
    /// What a delta counts.
    pub fn unit(self) -> TurnUnit {
        match self {
            EncoderKind::Relative(_) => TurnUnit::Detent,
            EncoderKind::Absolute => TurnUnit::Travel,
        }
    }
}

/// Palette indices of the controller for the parts of the surface which are not slots; `None`
/// takes the closest color of the palette to a default RGB color (which is often too dark, a
/// controller with a fixed palette sets them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiColors {
    pub row_active: Option<i32>,
    /// The rows of the even and the odd control sets.
    pub row_set_a: Option<i32>,
    pub row_set_b: Option<i32>,
    pub page_active: Option<i32>,
    pub page: Option<i32>,
    pub navigation: Option<i32>,
    /// The blink color of the selected slot.
    pub selected: Option<i32>,
    /// The cross without a source.
    pub no_provider: Option<i32>,
    pub incompatible: Option<i32>,
    /// A slot whose color is black (e.g. a pad which is off).
    pub black_slot: Option<i32>,
}

/// What a controller supplies: the layout of its grid and its encoders.
#[derive(Debug, Clone)]
pub struct SurfaceMapping {
    pub columns: usize,
    pub rows: usize,
    /// The function of each pad, row by row from the top.
    pads: Vec<PadFunction>,
    pub encoders: Vec<EncoderFunction>,
    pub encoder_kind: EncoderKind,
    /// The encoding of the main encoder, if there is one.
    pub main_encoder: Option<RelativeEncoding>,
    pub touch_encoders: bool,
    pub color_depth: super::ColorDepth,
    pub ui_colors: UiColors,
}

impl SurfaceMapping {
    /// A grid without functions and 6 encoders for the active row.
    pub fn new(columns: usize, rows: usize) -> Self {
        SurfaceMapping {
            columns,
            rows,
            pads: vec![PadFunction::None; columns * rows],
            encoders: (0..GRID_SIZE).map(EncoderFunction::RowSlot).collect(),
            encoder_kind: EncoderKind::Relative(RelativeEncoding::TwosComplement),
            main_encoder: None,
            touch_encoders: false,
            color_depth: super::ColorDepth::Palette,
            ui_colors: UiColors::default(),
        }
    }

    /// `columns` x `rows` cells with their top left at the pad (x, y).
    pub fn cells_at(mut self, x: usize, y: usize, columns: usize, rows: usize) -> Self {
        for row in 0..rows {
            for column in 0..columns {
                self.set_pad(x + column, y + row, PadFunction::Cell { row, column });
            }
        }
        self
    }

    /// The 6x6 cells with their top left at the pad (x, y).
    pub fn slots_at(self, x: usize, y: usize) -> Self {
        self.cells_at(x, y, GRID_SIZE, GRID_SIZE)
    }

    /// The 6 row selectors in a column, from the pad (x, y) down.
    pub fn row_selectors_at(mut self, x: usize, y: usize) -> Self {
        for row in 0..GRID_SIZE {
            self.set_pad(x, y + row, PadFunction::Row(row));
        }
        self
    }

    /// A page list of `count` pads in a column, from the pad (x, y) down.
    pub fn page_list_at(mut self, x: usize, y: usize, count: usize) -> Self {
        for page in 0..count {
            self.set_pad(x, y + page, PadFunction::Page(page));
        }
        self
    }

    /// A page list of `count` pads in a row, from the pad (x, y) to the right.
    pub fn page_list_row_at(mut self, x: usize, y: usize, count: usize) -> Self {
        for page in 0..count {
            self.set_pad(x + page, y, PadFunction::Page(page));
        }
        self
    }

    /// A section list of `count` pads in a row, from the pad (x, y) to the right.
    pub fn section_list_row_at(mut self, x: usize, y: usize, count: usize) -> Self {
        for section in 0..count {
            self.set_pad(x + section, y, PadFunction::Section(section));
        }
        self
    }

    pub fn pad(mut self, x: usize, y: usize, function: PadFunction) -> Self {
        self.set_pad(x, y, function);
        self
    }

    pub fn encoders(mut self, kind: EncoderKind, encoders: Vec<EncoderFunction>) -> Self {
        self.encoder_kind = kind;
        self.encoders = encoders;
        self
    }

    pub fn main_encoder(mut self, encoding: RelativeEncoding) -> Self {
        self.main_encoder = Some(encoding);
        self
    }

    fn set_pad(&mut self, x: usize, y: usize, function: PadFunction) {
        if x < self.columns && y < self.rows {
            self.pads[y * self.columns + x] = function;
        }
    }

    /// The function of the pad (x, y), y = 0 is the top row.
    pub fn pad_function(&self, x: usize, y: usize) -> PadFunction {
        if x < self.columns && y < self.rows {
            self.pads[y * self.columns + x]
        } else {
            PadFunction::None
        }
    }

    /// The number of pads of the page list.
    pub fn page_list_len(&self) -> usize {
        self.pads.iter().filter(|f| matches!(f, PadFunction::Page(_))).count()
    }

    /// The cells the grid shows (columns, rows).
    pub fn slot_area(&self) -> (usize, usize) {
        self.pads.iter().fold((0, 0), |(columns, rows), f| match f {
            PadFunction::Cell { row, column } => (columns.max(column + 1), rows.max(row + 1)),
            _ => (columns, rows),
        })
    }

    /// The number of encoders which turn the slots of the active row (and of the rows below it:
    /// `RowSlot(6..11)` is the next row).
    pub fn slot_encoders(&self) -> usize {
        self.encoders.iter().filter(|f| f.turns_row_slots()).count()
    }

    /// True if the slot encoders cover a whole control set of `rows_per_set` rows (12 encoders
    /// for the 2 rows of the Electra's sets): only then the grouping of the rows into control
    /// sets matters for the surface (the row selectors show it). A surface with one row of
    /// encoders (the JAM's strips) selects each row on its own.
    pub fn covers_control_set(&self, rows_per_set: usize) -> bool {
        rows_per_set > 1 && self.slot_encoders() >= rows_per_set * GRID_SIZE
    }

    /// The capabilities sent on attach.
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            grid: (self.columns, self.rows),
            encoders: self.encoders.len(),
            touch_encoders: self.touch_encoders,
            main_encoder: self.main_encoder.is_some(),
            color_depth: self.color_depth,
            slot_area: self.slot_area(),
            row_encoders: self.slot_encoders(),
        }
    }
}

/// An absolute control which is not moved for this time starts again (its next position is the
/// new anchor), for controls without touch.
const ANCHOR_TIMEOUT: Duration = Duration::from_millis(300);

/// A source with the surface's mirror of its model.
struct Entry {
    source: Box<dyn PageSource>,
    model: PageModel,
}

/// The surface side of a controller, shared by its view and its mode.
pub struct SurfaceClient {
    entries: Vec<Entry>,
    current: usize,
    mapping: SurfaceMapping,
    /// True while the controller shows the current source.
    shown: bool,
    /// The last positions of the absolute encoders and when they were sent.
    anchors: Vec<Option<(i32, Instant)>>,
    /// The model without a source.
    empty: PageModel,
}

pub type SharedClient = Rc<RefCell<SurfaceClient>>;

impl SurfaceClient {
    /// A client with its sources (the first is the current one).
    pub fn new(mapping: SurfaceMapping, sources: Vec<Box<dyn PageSource>>) -> Self {
        let anchors = vec![None; mapping.encoders.len()];
        let entries = sources.into_iter().map(|source| Entry { source, model: PageModel::default() }).collect();
        SurfaceClient { entries, current: 0, mapping, shown: false, anchors, empty: PageModel::default() }
    }

    /// A client with the surface link as its only source.
    pub fn link(link: &str, mapping: SurfaceMapping) -> Self {
        let source = crate::framework::surface_link::LinkSource::new(link, mapping.capabilities());
        Self::new(mapping, vec![Box::new(source)])
    }

    pub fn shared(mapping: SurfaceMapping, sources: Vec<Box<dyn PageSource>>) -> SharedClient {
        Rc::new(RefCell::new(Self::new(mapping, sources)))
    }

    /// The state of the current source.
    pub fn state(&self) -> LinkState {
        self.source_state(self.current)
    }

    pub fn is_attached(&self) -> bool {
        self.state() == LinkState::Attached
    }

    /// The name of the current source.
    pub fn provider_name(&self) -> &str {
        self.source_name(self.current)
    }

    /// The model of the current source.
    pub fn model(&self) -> &PageModel {
        self.entries.get(self.current).map_or(&self.empty, |e| &e.model)
    }

    pub fn mapping(&self) -> &SurfaceMapping {
        &self.mapping
    }

    // Sources

    pub fn source_count(&self) -> usize {
        self.entries.len()
    }

    pub fn current_source(&self) -> usize {
        self.current
    }

    pub fn source_state(&self, index: usize) -> LinkState {
        self.entries.get(index).map_or(LinkState::Missing, |e| e.source.state())
    }

    pub fn source_name(&self, index: usize) -> &str {
        self.entries.get(index).map_or("", |e| e.source.name())
    }

    /// The first attached source after `after` (from the first with None), in their order.
    pub fn next_available(&self, after: Option<usize>) -> Option<usize> {
        let start = after.map_or(0, |a| a + 1);
        (start..self.entries.len()).find(|&i| self.source_state(i) == LinkState::Attached)
    }

    /// Make a source the current one (a shown surface tells the old one that it no longer shows
    /// it and the new one that it does).
    pub fn select_source(&mut self, index: usize) {
        if index >= self.entries.len() || index == self.current {
            return;
        }
        if self.shown {
            self.entries[self.current].source.send(Input::Shown { on: false });
        }
        self.current = index;
        self.reset_anchors();
        if self.shown {
            self.entries[self.current].source.send(Input::Shown { on: true });
        }
    }

    /// The controller shows (or no longer shows) the current source.
    pub fn set_shown(&mut self, on: bool) {
        if self.shown != on {
            self.shown = on;
            self.reset_anchors();
            if let Some(entry) = self.entries.get_mut(self.current) {
                entry.source.send(Input::Shown { on });
            }
        }
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// The tick: poll all sources and apply their changes; returns the state changes of the
    /// current one.
    pub fn poll(&mut self, daw: &Model) -> Vec<LinkState> {
        let mut states = Vec::new();
        let mut reset = false;
        for (index, entry) in self.entries.iter_mut().enumerate() {
            for event in entry.source.poll(daw) {
                match event {
                    SourceEvent::State(state) => {
                        if state != LinkState::Attached {
                            entry.model = PageModel::default();
                        }
                        if index == self.current {
                            states.push(state);
                        }
                    }
                    SourceEvent::Model(change) => {
                        let page = entry.model.page;
                        entry.model.apply(&change);
                        reset |= index == self.current && entry.model.page != page;
                    }
                }
            }
        }
        if reset {
            self.reset_anchors();
        }
        states
    }

    /// Send input to the current source.
    pub fn send(&mut self, input: Input) {
        if let Some(entry) = self.entries.get_mut(self.current) {
            entry.source.send(input);
        }
    }

    // Gestures

    /// The first page of the page list (a window of its length around the current page).
    pub fn page_offset(&self) -> usize {
        let count = self.mapping.page_list_len();
        self.model().page.checked_div(count).map_or(0, |window| window * count)
    }

    /// A pad of the grid: press (velocity > 0) or release.
    pub fn on_pad(&mut self, x: usize, y: usize, pressed: bool) {
        match self.mapping.pad_function(x, y) {
            PadFunction::None => {}
            PadFunction::Cell { row, column } => {
                if let Some(id) = self.model().slot_id(row, column) {
                    self.send(if pressed { Input::SlotPress { id } } else { Input::SlotRelease { id } });
                }
            }
            PadFunction::Row(row) if pressed => self.select_row(row),
            PadFunction::Page(index) if pressed => {
                let page = self.page_offset() + index;
                if page < self.model().pages.len() {
                    self.send(Input::PageSelect { page });
                }
            }
            PadFunction::PagePrev if pressed => self.send(Input::PagePrev),
            PadFunction::PageNext if pressed => self.send(Input::PageNext),
            PadFunction::Section(section) if pressed && section < self.model().sections.len() => self.send(Input::SectionSelect { section }),
            _ => {}
        }
    }

    pub fn select_row(&mut self, row: usize) {
        if row < self.model().rows {
            self.reset_anchors();
            self.send(Input::RowSelect { row });
        }
    }

    /// The previous (-1) or next (+1) row.
    pub fn step_row(&mut self, step: i32) {
        let model = self.model();
        let row = (model.active_row as i32 + step).clamp(0, model.rows as i32 - 1) as usize;
        if row != model.active_row {
            self.select_row(row);
        }
    }

    /// The previous (-1) or next (+1) control set of a model with sets; else the next (+1) or
    /// previous (-1) value of the selected slot's control (e.g. the handle of an envelope).
    pub fn step_value(&mut self, step: i32) {
        if step == 0 {
            return;
        }
        if self.model().has_sets() {
            self.reset_anchors();
            self.send(Input::SetStep { delta: step });
        } else if self.model().selected_slot.is_some() {
            self.send(if step > 0 { Input::ValueNext } else { Input::ValuePrev });
        }
    }

    /// The function of an encoder for the current model.
    fn encoder_function(&self, index: usize) -> Option<EncoderFunction> {
        self.mapping.encoders.get(index).map(|f| f.resolve(self.model()))
    }

    /// The slot of the active row in a column.
    /// Assign the active row's cell in `column` to slot `slot` of the source's remote control
    /// page (e.g. Select + a strip touch on the JAM); the name of the cell for a notification
    /// (empty for an empty cell, which clears the slot), `None` without such a cell.
    pub fn assign_row_cell(&mut self, column: usize, slot: i32) -> Option<String> {
        let (id, name) = {
            let model = self.model();
            let id = model.slot_id(model.active_row, column)?;
            (id, model.slot(id).map(|s| s.name.clone()).unwrap_or_default())
        };
        self.send(Input::SlotAssign { id, slot });
        Some(name)
    }

    fn row_slot(&self, column: usize) -> Option<usize> {
        let model = self.model();
        model.slot_id(model.active_row, column)
    }

    /// The touch of an encoder: an absolute one starts again; a touch-sensitive slot is
    /// touched.
    pub fn on_encoder_touch(&mut self, index: usize, touched: bool) {
        if let Some(anchor) = self.anchors.get_mut(index) {
            *anchor = None;
        }
        if let Some(EncoderFunction::RowSlot(column)) = self.encoder_function(index) {
            if let Some(id) = self.row_slot(column) {
                if self.mapping.touch_encoders && self.model().slot(id).is_some_and(|s| s.kind.is_turnable()) {
                    self.send(Input::SlotTouch { id, on: touched });
                }
            }
        }
    }

    /// A value of an encoder (0..127); `now` for the anchor of absolute controls.
    pub fn on_encoder(&mut self, index: usize, value: i32, fine: bool, now: Instant) {
        let Some(function) = self.encoder_function(index) else {
            return;
        };
        let delta = match self.mapping.encoder_kind {
            EncoderKind::Relative(encoding) => ValueChanger::new(encoding, 128, 1).decode(value),
            EncoderKind::Absolute => {
                let anchor = self.anchors[index].filter(|(_, time)| now.saturating_duration_since(*time) < ANCHOR_TIMEOUT);
                self.anchors[index] = Some((value, now));
                match anchor {
                    Some((last, _)) => value - last,
                    None => 0,
                }
            }
        };
        if delta == 0 {
            return;
        }
        let unit = self.mapping.encoder_kind.unit();
        match function {
            EncoderFunction::RowSlot(column) => {
                if let Some(id) = self.row_slot(column) {
                    if self.model().slot(id).is_some_and(|s| s.kind.is_turnable() && s.enabled) {
                        self.send(Input::SlotTurn { id, delta, fine, unit });
                    }
                }
            }
            EncoderFunction::Selected { fine: always_fine } => {
                if self.model().selected_slot.is_some() {
                    self.send(Input::SelectedTurn { delta, fine: fine || always_fine, unit });
                }
            }
            EncoderFunction::Extra(index) if index < self.model().extras.len() => self.send(Input::ExtraTurn { index, delta, fine }),
            _ => {}
        }
    }

    /// The main encoder (relative): `selected.turn`.
    pub fn on_main_encoder(&mut self, value: i32, fine: bool) {
        let encoding = self.mapping.main_encoder.unwrap_or(RelativeEncoding::TwosComplement);
        let delta = ValueChanger::new(encoding, 128, 1).decode(value);
        if delta != 0 && self.model().selected_slot.is_some() {
            self.send(Input::SelectedTurn { delta, fine, unit: TurnUnit::Detent });
        }
    }

    /// The value of an encoder for its LEDs (0..127), `None` if it has nothing.
    pub fn encoder_value(&self, index: usize) -> Option<(i32, super::Rgb)> {
        let to_midi = |value: f32| (value.clamp(0.0, 1.0) * 127.0).round() as i32;
        let model = self.model();
        match self.encoder_function(index)? {
            EncoderFunction::RowSlot(column) => {
                let slot = model.slot(self.row_slot(column)?)?;
                (slot.kind != super::SlotKind::Empty && slot.enabled).then(|| (to_midi(slot.value), slot.color))
            }
            EncoderFunction::Selected { .. } => {
                let slot = model.slot(model.selected_slot?)?;
                Some((to_midi(slot.value), slot.color))
            }
            EncoderFunction::Extra(index) => model.extras.get(index).map(|e| (to_midi(e.value), e.color)),
            _ => None,
        }
    }

    fn reset_anchors(&mut self) {
        self.anchors.fill(None);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::{ControlSet, ModelChange, Rgb, Section, Slot, SlotKind};
    use super::*;
    use crate::framework::surface_link::{provide, LinkSource, ProviderEvent};

    /// A DAW model for polling the sources.
    pub(crate) fn daw() -> Model {
        use crate::backend::{RecordingBackend, Sender};
        use crate::daw::model_setup::ModelSetup;
        use crate::host::{Host, MemoryLog};
        use crate::scale::Scales;
        let vc = ValueChanger::twos_complement(128, 1);
        let scales = Scales::new(&vc, 36, 100, 8, 8);
        Model::new(Host::new(Rc::new(MemoryLog::default())), Sender::new(Rc::new(RecordingBackend::default())), ModelSetup::default(), vc, scales)
    }

    /// The JAM's layout (doc/HOST_PAGE_PLAN.md §5).
    pub(crate) fn jam_like() -> SurfaceMapping {
        SurfaceMapping::new(8, 8)
            .page_list_row_at(0, 0, 6)
            .pad(6, 0, PadFunction::PagePrev)
            .pad(7, 0, PadFunction::PageNext)
            .cells_at(0, 1, 8, 6)
            .section_list_row_at(0, 7, 8)
            .encoders(
                EncoderKind::Absolute,
                vec![
                    EncoderFunction::RowSlot(0),
                    EncoderFunction::RowSlot(1),
                    EncoderFunction::RowSlot(2),
                    EncoderFunction::RowSlot(3),
                    EncoderFunction::RowSlot(4),
                    EncoderFunction::RowSlot(5),
                    EncoderFunction::RowSlotOrSelected { column: 6, fine: true },
                    EncoderFunction::RowSlotOrExtra { column: 7, index: 0 },
                ],
            )
            .main_encoder(RelativeEncoding::TwosComplement)
    }

    fn inputs(events: Vec<ProviderEvent>) -> Vec<Input> {
        events.into_iter().filter_map(|e| if let ProviderEvent::Input { input, .. } = e { Some(input) } else { None }).collect()
    }

    #[test]
    fn mapping() {
        let mapping = jam_like();
        assert_eq!(mapping.pad_function(0, 1), PadFunction::Cell { row: 0, column: 0 });
        assert_eq!(mapping.pad_function(7, 6), PadFunction::Cell { row: 5, column: 7 });
        assert_eq!(mapping.pad_function(2, 0), PadFunction::Page(2));
        assert_eq!(mapping.pad_function(7, 0), PadFunction::PageNext);
        assert_eq!(mapping.pad_function(3, 7), PadFunction::Section(3));
        assert_eq!(mapping.pad_function(9, 9), PadFunction::None);
        assert_eq!(mapping.page_list_len(), 6);
        let caps = mapping.capabilities();
        assert_eq!((caps.grid, caps.encoders, caps.main_encoder, caps.slot_area, caps.row_encoders), ((8, 8), 8, true, (8, 6), 8));
    }

    #[test]
    fn gestures() {
        let link = "test-client-gestures";
        let daw = daw();
        let mut provider = provide(link, "P").unwrap();
        let mut client = SurfaceClient::link(link, jam_like());
        let mut model = PageModel { pages: (0..8).map(|p| format!("P{p}")).collect(), active_row: 1, ..PageModel::default() };
        model.slots[6] = Slot { kind: SlotKind::Fader, enabled: true, value: 0.5, ..Slot::default() };
        model.slots[7] = Slot { kind: SlotKind::Pad, enabled: true, ..Slot::default() };
        provider.poll(|| model.clone());
        assert_eq!(client.poll(&daw), [LinkState::Attached]);
        assert_eq!(client.model().active_row, 1);

        let now = Instant::now();
        // Row 1, column 0 is slot 6
        client.on_pad(0, 2, true);
        client.on_pad(0, 2, false);
        // Column 7 is beyond the 6-wide page
        client.on_pad(7, 2, true);
        client.on_pad(1, 0, true);
        client.on_pad(6, 0, true);
        // No sections
        client.on_pad(0, 7, true);
        // The first position is the anchor, then the differences
        client.on_encoder(0, 60, false, now);
        client.on_encoder(0, 64, false, now + Duration::from_millis(10));
        // Column 1 of row 1 is a pad: no turn
        client.on_encoder(1, 10, false, now);
        client.on_encoder(1, 20, false, now);
        // After the timeout: a new anchor
        client.on_encoder(0, 100, false, now + Duration::from_secs(1));
        client.on_main_encoder(127, false);
        client.step_row(1);
        // No sets and no selection: LEFT/RIGHT do nothing
        client.step_value(1);
        assert_eq!(
            inputs(provider.poll(|| model.clone())),
            [
                Input::SlotPress { id: 6 },
                Input::SlotRelease { id: 6 },
                Input::PageSelect { page: 1 },
                Input::PagePrev,
                Input::SlotTurn { id: 6, delta: 4, fine: false, unit: TurnUnit::Travel },
                Input::RowSelect { row: 2 },
            ]
        );

        // The page list shows the window of the current page: page 7 is its second entry
        model.page = 7;
        provider.broadcast(&ModelChange::Snapshot(Box::new(model.clone())));
        client.poll(&daw);
        assert_eq!(client.page_offset(), 6);
        client.on_pad(1, 0, true);
        assert_eq!(inputs(provider.poll(|| model.clone())), [Input::PageSelect { page: 7 }]);

        // The selected slot: the main encoder and the fine strip (6-wide page)
        provider.broadcast(&ModelChange::SelectedSlot(Some(6)));
        client.poll(&daw);
        client.on_main_encoder(127, false);
        client.on_encoder(6, 10, false, now);
        client.on_encoder(6, 12, false, now);
        client.step_value(1);
        assert_eq!(client.encoder_value(6), Some((64, Rgb::BLACK)));
        assert_eq!(client.encoder_value(0), Some((64, Rgb::BLACK)));
        assert_eq!(client.encoder_value(1), Some((0, Rgb::BLACK)));
        assert_eq!(client.encoder_value(7), None);
        assert_eq!(
            inputs(provider.poll(|| model.clone())),
            [Input::SelectedTurn { delta: -1, fine: false, unit: TurnUnit::Detent }, Input::SelectedTurn { delta: 2, fine: true, unit: TurnUnit::Travel }, Input::ValueNext]
        );

        drop(provider);
        assert_eq!(client.poll(&daw), [LinkState::Detached]);
        assert_eq!(client.model().pages.len(), 0);
    }

    #[test]
    fn wide_pages_with_sets() {
        let link = "test-client-wide";
        let daw = daw();
        let mut provider = provide(link, "P").unwrap();
        let mut client = SurfaceClient::link(link, jam_like());
        let mut model = PageModel { columns: 8, rows: 6, slots: vec![Slot::default(); 48], active_row: 2, ..PageModel::default() };
        model.sets = vec![ControlSet::default(), ControlSet::default()];
        model.sections = vec![Section::default(); 2];
        model.slots[23] = Slot { kind: SlotKind::Fader, enabled: true, value: 1.0, set: Some(1), ..Slot::default() };
        provider.poll(|| model.clone());
        client.poll(&daw);
        let now = Instant::now();
        // Strip 8 is column 7 of the 8-wide page
        client.on_encoder(7, 10, false, now);
        client.on_encoder(7, 11, false, now);
        assert_eq!(client.encoder_value(7), Some((127, Rgb::BLACK)));
        assert_eq!(client.encoder_value(6), None);
        client.on_pad(7, 3, true);
        client.on_pad(1, 7, true);
        client.on_pad(2, 7, true);
        // LEFT/RIGHT: the control sets
        client.step_value(-1);
        assert_eq!(
            inputs(provider.poll(|| model.clone())),
            [Input::SlotTurn { id: 23, delta: 1, fine: false, unit: TurnUnit::Travel }, Input::SlotPress { id: 23 }, Input::SectionSelect { section: 1 }, Input::SetStep { delta: -1 }]
        );
    }

    #[test]
    fn sources_and_shown() {
        let daw = daw();
        let (a, b) = ("test-client-source-a", "test-client-source-b");
        let mapping = jam_like();
        let sources: Vec<Box<dyn PageSource>> = vec![Box::new(LinkSource::new(a, mapping.capabilities())), Box::new(LinkSource::new(b, mapping.capabilities()))];
        let mut client = SurfaceClient::new(mapping, sources);
        assert_eq!(client.next_available(None), None);
        let mut second = provide(b, "B").unwrap();
        second.poll(PageModel::default);
        client.poll(&daw);
        assert_eq!(client.next_available(None), Some(1));
        assert_eq!(client.next_available(Some(1)), None);
        assert_eq!(client.state(), LinkState::Missing);
        client.select_source(1);
        assert_eq!(client.provider_name(), "B");
        client.set_shown(true);
        client.set_shown(true);
        let mut first = provide(a, "A").unwrap();
        first.poll(PageModel::default);
        client.poll(&daw);
        client.select_source(0);
        assert_eq!(inputs(second.poll(PageModel::default)), [Input::Shown { on: true }, Input::Shown { on: false }]);
        assert_eq!(inputs(first.poll(PageModel::default)), [Input::Shown { on: true }]);
    }
}
