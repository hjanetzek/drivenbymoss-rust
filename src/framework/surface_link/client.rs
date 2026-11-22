// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The surface side as a reusable helper: the link, the mirrored page model and the mapping of
//! the controller (which pads are slots, row and page selectors, what the encoders do). It is
//! shared by [`super::view::SurfaceLinkView`] (the grid) and [`super::view::SurfaceLinkMode`]
//! (the encoders), and polled by the controller at its tick.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::model::GRID_SIZE;
use super::{attach, Capabilities, Input, LinkState, PageModel, SurfaceEvent, SurfaceHandle};
use crate::framework::value_changer::{RelativeEncoding, ValueChanger};

/// The function of a pad of the controller's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadFunction {
    None,
    /// A slot (0..35).
    Slot(usize),
    /// Select a row (0..5).
    Row(usize),
    /// Select a page of the provider's page list.
    Page(usize),
    PagePrev,
    PageNext,
}

/// The function of an encoder (or fader / touch strip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderFunction {
    None,
    /// The slot of the active row in this column (0..5).
    RowSlot(usize),
    /// The selected slot.
    Selected { fine: bool },
    /// An extra value of the provider.
    Extra(usize),
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
    /// The cross without a provider.
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

    /// The 6x6 slots with their top left at the pad (x, y).
    pub fn slots_at(mut self, x: usize, y: usize) -> Self {
        for row in 0..GRID_SIZE {
            for column in 0..GRID_SIZE {
                self.set_pad(x + column, y + row, PadFunction::Slot(row * GRID_SIZE + column));
            }
        }
        self
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

    /// The capabilities sent on attach.
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            grid: (self.columns, self.rows),
            encoders: self.encoders.len(),
            touch_encoders: self.touch_encoders,
            main_encoder: self.main_encoder.is_some(),
            color_depth: self.color_depth,
        }
    }
}

/// An absolute control which is not moved for this time starts again (its next position is the
/// new anchor), for controls without touch.
const ANCHOR_TIMEOUT: Duration = Duration::from_millis(300);

/// The surface side of a link, shared by the view and the mode of a controller.
pub struct SurfaceLinkClient {
    handle: SurfaceHandle,
    model: PageModel,
    mapping: SurfaceMapping,
    /// The last positions of the absolute encoders and when they were sent.
    anchors: Vec<Option<(i32, Instant)>>,
}

pub type SharedClient = Rc<RefCell<SurfaceLinkClient>>;

impl SurfaceLinkClient {
    /// Attach to the link with the capabilities of the mapping.
    pub fn new(link: &str, mapping: SurfaceMapping) -> Self {
        let handle = attach(link, mapping.capabilities());
        let anchors = vec![None; mapping.encoders.len()];
        SurfaceLinkClient { handle, model: PageModel::default(), mapping, anchors }
    }

    pub fn shared(link: &str, mapping: SurfaceMapping) -> SharedClient {
        Rc::new(RefCell::new(Self::new(link, mapping)))
    }

    pub fn state(&self) -> LinkState {
        self.handle.state()
    }

    pub fn is_attached(&self) -> bool {
        self.handle.is_attached()
    }

    pub fn provider_name(&self) -> &str {
        self.handle.provider_name()
    }

    pub fn model(&self) -> &PageModel {
        &self.model
    }

    pub fn mapping(&self) -> &SurfaceMapping {
        &self.mapping
    }

    /// The tick: apply the provider's changes; returns the state changes.
    pub fn poll(&mut self) -> Vec<LinkState> {
        let mut states = Vec::new();
        for event in self.handle.poll() {
            match event {
                SurfaceEvent::State(state) => {
                    if state != LinkState::Attached {
                        self.model = PageModel::default();
                    }
                    states.push(state);
                }
                SurfaceEvent::Model(change) => {
                    let page = self.model.page;
                    self.model.apply(&change);
                    if self.model.page != page {
                        self.reset_anchors();
                    }
                }
            }
        }
        states
    }

    pub fn send(&self, input: Input) {
        self.handle.send(input);
    }

    // Gestures

    /// A pad of the grid: press (velocity > 0) or release.
    pub fn on_pad(&mut self, x: usize, y: usize, pressed: bool) {
        match self.mapping.pad_function(x, y) {
            PadFunction::None => {}
            PadFunction::Slot(id) => self.send(if pressed { Input::SlotPress { id } } else { Input::SlotRelease { id } }),
            PadFunction::Row(row) if pressed => self.select_row(row),
            PadFunction::Page(page) if pressed => {
                if page < self.model.pages.len() {
                    self.send(Input::PageSelect { page });
                }
            }
            PadFunction::PagePrev if pressed => self.send(Input::PagePrev),
            PadFunction::PageNext if pressed => self.send(Input::PageNext),
            _ => {}
        }
    }

    pub fn select_row(&mut self, row: usize) {
        if row < GRID_SIZE {
            self.reset_anchors();
            self.send(Input::RowSelect { row });
        }
    }

    /// The previous (-1) or next (+1) row.
    pub fn step_row(&mut self, step: i32) {
        let row = (self.model.active_row as i32 + step).clamp(0, GRID_SIZE as i32 - 1) as usize;
        if row != self.model.active_row {
            self.select_row(row);
        }
    }

    /// The next (+1) or previous (-1) value of the selected slot's control (e.g. the handle of
    /// an envelope).
    pub fn step_value(&self, step: i32) {
        if self.model.selected_slot.is_some() && step != 0 {
            self.send(if step > 0 { Input::ValueNext } else { Input::ValuePrev });
        }
    }

    /// The touch of an encoder: an absolute one starts again; a touch-sensitive slot is
    /// touched.
    pub fn on_encoder_touch(&mut self, index: usize, touched: bool) {
        if let Some(anchor) = self.anchors.get_mut(index) {
            *anchor = None;
        }
        if let Some(EncoderFunction::RowSlot(column)) = self.mapping.encoders.get(index) {
            let id = self.model.active_row * GRID_SIZE + column;
            if self.mapping.touch_encoders && self.model.slot(id).is_some_and(|s| s.kind.is_turnable()) {
                self.send(Input::SlotTouch { id, on: touched });
            }
        }
    }

    /// A value of an encoder (0..127); `now` for the anchor of absolute controls.
    pub fn on_encoder(&mut self, index: usize, value: i32, fine: bool, now: Instant) {
        let Some(function) = self.mapping.encoders.get(index).copied() else {
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
        match function {
            EncoderFunction::None => {}
            EncoderFunction::RowSlot(column) => {
                let id = self.model.active_row * GRID_SIZE + column;
                if self.model.slot(id).is_some_and(|s| s.kind.is_turnable() && s.enabled) {
                    self.send(Input::SlotTurn { id, delta, fine });
                }
            }
            EncoderFunction::Selected { fine: always_fine } => {
                if self.model.selected_slot.is_some() {
                    self.send(Input::SelectedTurn { delta, fine: fine || always_fine });
                }
            }
            EncoderFunction::Extra(index) => {
                if index < self.model.extras.len() {
                    self.send(Input::ExtraTurn { index, delta, fine });
                }
            }
        }
    }

    /// The main encoder (relative): `selected.turn`.
    pub fn on_main_encoder(&mut self, value: i32, fine: bool) {
        let encoding = self.mapping.main_encoder.unwrap_or(RelativeEncoding::TwosComplement);
        let delta = ValueChanger::new(encoding, 128, 1).decode(value);
        if delta != 0 && self.model.selected_slot.is_some() {
            self.send(Input::SelectedTurn { delta, fine });
        }
    }

    /// The value of an encoder for its LEDs (0..127), `None` if it has nothing.
    pub fn encoder_value(&self, index: usize) -> Option<(i32, super::Rgb)> {
        let to_midi = |value: f32| (value.clamp(0.0, 1.0) * 127.0).round() as i32;
        match self.mapping.encoders.get(index)? {
            EncoderFunction::None => None,
            EncoderFunction::RowSlot(column) => {
                let slot = self.model.slot(self.model.active_row * GRID_SIZE + column)?;
                (slot.kind != super::SlotKind::Empty && slot.enabled).then(|| (to_midi(slot.value), slot.color))
            }
            EncoderFunction::Selected { .. } => {
                let slot = self.model.slot(self.model.selected_slot?)?;
                Some((to_midi(slot.value), slot.color))
            }
            EncoderFunction::Extra(index) => self.model.extras.get(*index).map(|e| (to_midi(e.value), e.color)),
        }
    }

    fn reset_anchors(&mut self) {
        self.anchors.fill(None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{provide, ModelChange, Slot, SlotKind};
    use super::*;

    fn jam_like() -> SurfaceMapping {
        SurfaceMapping::new(8, 8)
            .slots_at(1, 1)
            .row_selectors_at(0, 1)
            .page_list_at(7, 0, 8)
            .pad(0, 0, PadFunction::PagePrev)
            .pad(0, 7, PadFunction::PageNext)
            .encoders(
                EncoderKind::Absolute,
                vec![
                    EncoderFunction::RowSlot(0),
                    EncoderFunction::RowSlot(1),
                    EncoderFunction::RowSlot(2),
                    EncoderFunction::RowSlot(3),
                    EncoderFunction::RowSlot(4),
                    EncoderFunction::RowSlot(5),
                    EncoderFunction::Selected { fine: true },
                    EncoderFunction::Extra(0),
                ],
            )
            .main_encoder(RelativeEncoding::TwosComplement)
    }

    #[test]
    fn mapping() {
        let mapping = jam_like();
        assert_eq!(mapping.pad_function(1, 1), PadFunction::Slot(0));
        assert_eq!(mapping.pad_function(6, 6), PadFunction::Slot(35));
        assert_eq!(mapping.pad_function(0, 3), PadFunction::Row(2));
        assert_eq!(mapping.pad_function(7, 2), PadFunction::Page(2));
        assert_eq!(mapping.pad_function(0, 7), PadFunction::PageNext);
        assert_eq!(mapping.pad_function(9, 9), PadFunction::None);
        let caps = mapping.capabilities();
        assert_eq!((caps.grid, caps.encoders, caps.main_encoder), ((8, 8), 8, true));
    }

    #[test]
    fn gestures() {
        let link = "test-client-gestures";
        let mut provider = provide(link, "P").unwrap();
        let mut client = SurfaceLinkClient::new(link, jam_like());
        let mut model = PageModel { pages: vec!["A".into(), "B".into()], active_row: 1, ..PageModel::default() };
        model.slots[6] = Slot { kind: SlotKind::Fader, enabled: true, value: 0.5, ..Slot::default() };
        model.slots[7] = Slot { kind: SlotKind::Pad, enabled: true, ..Slot::default() };
        provider.poll(|| model.clone());
        assert_eq!(client.poll(), [LinkState::Attached]);
        assert_eq!(client.model().active_row, 1);

        let now = Instant::now();
        client.on_pad(1, 2, true);
        client.on_pad(1, 2, false);
        client.on_pad(0, 4, true);
        client.on_pad(7, 1, true);
        // A page which does not exist
        client.on_pad(7, 5, true);
        // The first position is the anchor, then the differences
        client.on_encoder(0, 60, false, now);
        client.on_encoder(0, 64, false, now + Duration::from_millis(10));
        // Column 1 of row 1 is a pad: no turn
        client.on_encoder(1, 10, false, now);
        client.on_encoder(1, 20, false, now);
        // After the timeout: a new anchor
        client.on_encoder(0, 100, false, now + Duration::from_secs(1));
        client.on_main_encoder(127, false);
        let inputs: Vec<Input> = provider.poll(|| model.clone()).into_iter().filter_map(|e| if let super::super::ProviderEvent::Input { input, .. } = e { Some(input) } else { None }).collect();
        assert_eq!(
            inputs,
            [Input::SlotPress { id: 6 }, Input::SlotRelease { id: 6 }, Input::RowSelect { row: 3 }, Input::PageSelect { page: 1 }, Input::SlotTurn { id: 6, delta: 4, fine: false }]
        );

        // The selected slot: the main encoder and the fine strip
        provider.broadcast(&ModelChange::SelectedSlot(Some(6)));
        client.poll();
        client.on_main_encoder(127, false);
        client.on_encoder(6, 10, false, now);
        client.on_encoder(6, 12, false, now);
        assert_eq!(client.encoder_value(6), Some((64, super::super::Rgb::BLACK)));
        assert_eq!(client.encoder_value(0), Some((64, super::super::Rgb::BLACK)));
        assert_eq!(client.encoder_value(1), Some((0, super::super::Rgb::BLACK)));
        assert_eq!(client.encoder_value(7), None);
        let inputs = provider.poll(|| model.clone());
        assert_eq!(inputs.len(), 2);
        assert!(matches!(inputs[0], super::super::ProviderEvent::Input { input: Input::SelectedTurn { delta: -1, fine: false }, .. }));
        assert!(matches!(inputs[1], super::super::ProviderEvent::Input { input: Input::SelectedTurn { delta: 2, fine: true }, .. }));

        drop(provider);
        assert_eq!(client.poll(), [LinkState::Detached]);
        assert_eq!(client.model().pages.len(), 0);
    }
}
