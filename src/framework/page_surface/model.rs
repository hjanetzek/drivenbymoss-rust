// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The page model of a page surface (doc/SURFACE_LINK.md §2, doc/HOST_PAGE_PLAN.md §3.3): the
//! model which a source (the surface link's provider, the host) gives a surface, its changes, the
//! input of the surfaces and their capabilities. Nothing here knows a concrete controller.
//!
//! 1.2 (HOST_PAGE_PLAN): sizes up to 8x8 (`columns`, `rows`), explicit control sets and sections,
//! the turn unit, `Shown`, `SetSelect`/`SetStep`/`SectionSelect`.

/// The number of slot columns and rows of a default (Electra) page.
pub const GRID_SIZE: usize = 6;
/// The number of slots of a default page (row by row, the ID of a slot is its index).
pub const NUM_SLOTS: usize = GRID_SIZE * GRID_SIZE;
/// The largest number of columns and rows of a page.
pub const MAX_SIZE: usize = 8;

/// An RGB color (8 bits per channel); the surface maps it to its palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Parse `RRGGBB` (as in the presets of the Electra One), black if it is not a color.
    pub fn from_hex(hex: &str) -> Rgb {
        let parse = |range: std::ops::Range<usize>| hex.get(range).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
        Rgb::new(parse(0..2), parse(2..4), parse(4..6))
    }

    pub fn is_black(self) -> bool {
        self == Rgb::BLACK
    }
}

impl From<crate::daw::color::ColorEx> for Rgb {
    fn from(color: crate::daw::color::ColorEx) -> Self {
        let [r, g, b] = color.to_int_rgb255();
        Rgb::new(r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8)
    }
}

impl From<Rgb> for crate::daw::color::ColorEx {
    fn from(color: Rgb) -> Self {
        crate::daw::color::ColorEx::from_rgb(color.r as i32, color.g as i32, color.b as i32)
    }
}

/// The kind of a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotKind {
    /// A continuous value (0..1).
    Fader,
    /// A value with `steps` discrete values.
    List,
    /// A button; `on` is its state, as far as the provider knows it.
    Pad,
    /// An endless control without a value (e.g. the play position); only the text is shown.
    Relative,
    /// Nothing at this position.
    #[default]
    Empty,
}

impl SlotKind {
    /// True if the slot is turned (it takes `slot.turn`).
    pub fn is_turnable(self) -> bool {
        matches!(self, SlotKind::Fader | SlotKind::List | SlotKind::Relative)
    }
}

/// One slot of the 6x6 grid.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Slot {
    pub kind: SlotKind,
    pub name: String,
    pub color: Rgb,
    /// The normalized value (0..1).
    pub value: f32,
    /// The value as the provider formats it.
    pub value_text: String,
    /// The number of steps of a list (0 for the other kinds).
    pub steps: u32,
    /// The state of a pad.
    pub on: bool,
    /// False if the slot is greyed out (e.g. no track at this position).
    pub enabled: bool,
    /// The control set of the slot (an index into [`PageModel::sets`]), if the model has sets.
    pub set: Option<usize>,
}

impl Slot {
    /// True if only the value (and its text) differs.
    fn differs_in_value_only(&self, other: &Slot) -> bool {
        self.kind == other.kind
            && self.name == other.name
            && self.color == other.color
            && self.steps == other.steps
            && self.on == other.on
            && self.enabled == other.enabled
            && self.set == other.set
            && (self.value != other.value || self.value_text != other.value_text)
    }
}

/// A provider-defined value for an encoder beyond the 6 of a row (§2 `extras`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Extra {
    pub name: String,
    pub color: Rgb,
    pub value: f32,
    pub value_text: String,
}

/// A control set: the slots which the encoders of a surface turn together (a zynthian page of
/// 4 controllers). Its slots name it in [`Slot::set`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ControlSet {
    pub name: String,
    /// The section of the set (an index into [`PageModel::sections`]).
    pub section: Option<usize>,
    pub color: Rgb,
}

/// A section of the pages (zynthian: the chain's controllers, a processor).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Section {
    pub name: String,
    pub color: Rgb,
}

/// The page model of a source (§2).
#[derive(Debug, Clone, PartialEq)]
pub struct PageModel {
    /// The index of the page in `pages`.
    pub page: usize,
    /// The names of all pages (for a page selector).
    pub pages: Vec<String>,
    /// The number of slot columns and rows (6x6 for the Electra, up to [`MAX_SIZE`]).
    pub columns: usize,
    pub rows: usize,
    /// The `columns * rows` slots, row by row.
    pub slots: Vec<Slot>,
    /// The number of rows which form a control set (rows 1/2, 3/4 and 5/6 with 2).
    pub rows_per_set: usize,
    /// The row which the encoders of a surface drive (0..5).
    pub active_row: usize,
    /// The slot which the main encoder fine-tunes.
    pub selected_slot: Option<usize>,
    pub extras: Vec<Extra>,
    /// The explicit control sets (empty: the sets are the rows grouped by `rows_per_set`).
    pub sets: Vec<ControlSet>,
    /// The set which the encoders turn (with explicit sets).
    pub active_set: Option<usize>,
    pub sections: Vec<Section>,
    pub active_section: Option<usize>,
}

impl Default for PageModel {
    fn default() -> Self {
        PageModel {
            page: 0,
            pages: Vec::new(),
            columns: GRID_SIZE,
            rows: GRID_SIZE,
            slots: vec![Slot::default(); NUM_SLOTS],
            rows_per_set: 2,
            active_row: 0,
            selected_slot: None,
            extras: Vec::new(),
            sets: Vec::new(),
            active_set: None,
            sections: Vec::new(),
            active_section: None,
        }
    }
}

impl PageModel {
    /// The name of the current page.
    pub fn page_name(&self) -> &str {
        self.pages.get(self.page).map(String::as_str).unwrap_or("")
    }

    pub fn slot(&self, id: usize) -> Option<&Slot> {
        self.slots.get(id)
    }

    /// The ID of the slot at the row and column, if the page has it.
    pub fn slot_id(&self, row: usize, column: usize) -> Option<usize> {
        (row < self.rows && column < self.columns).then_some(row * self.columns + column).filter(|id| *id < self.slots.len())
    }

    /// The slot at the row and column.
    pub fn slot_at(&self, row: usize, column: usize) -> Option<&Slot> {
        self.slot_id(row, column).and_then(|id| self.slots.get(id))
    }

    /// True if the model has explicit control sets.
    pub fn has_sets(&self) -> bool {
        !self.sets.is_empty()
    }

    /// The control set of a row.
    pub fn control_set(&self, row: usize) -> usize {
        row / self.rows_per_set.max(1)
    }

    /// Apply a change (a surface mirrors the provider's model with it).
    pub fn apply(&mut self, change: &ModelChange) {
        match change {
            ModelChange::Snapshot(model) => *self = (**model).clone(),
            ModelChange::Slot { id, slot } => {
                if let Some(s) = self.slots.get_mut(*id) {
                    *s = slot.clone();
                }
            }
            ModelChange::Value { id, value, value_text } => {
                if let Some(s) = self.slots.get_mut(*id) {
                    s.value = *value;
                    s.value_text.clone_from(value_text);
                }
            }
            ModelChange::ActiveRow(row) => self.active_row = *row,
            ModelChange::SelectedSlot(slot) => self.selected_slot = *slot,
            ModelChange::ActiveSet(set) => self.active_set = *set,
            ModelChange::ActiveSection(section) => self.active_section = *section,
            ModelChange::Extra { index, extra } => {
                if let Some(e) = self.extras.get_mut(*index) {
                    *e = extra.clone();
                }
            }
        }
    }

    /// The changes which turn `old` into `self`: a snapshot if the page, the page list, the
    /// size, the number of slots or extras, the row grouping, the sets or the sections differ,
    /// otherwise one message per changed part.
    pub fn diff(&self, old: &PageModel) -> Vec<ModelChange> {
        if self.page != old.page
            || self.pages != old.pages
            || self.columns != old.columns
            || self.rows != old.rows
            || self.sets != old.sets
            || self.sections != old.sections
            || self.slots.len() != old.slots.len()
            || self.extras.len() != old.extras.len()
            || self.rows_per_set != old.rows_per_set
        {
            return vec![ModelChange::Snapshot(Box::new(self.clone()))];
        }
        let mut changes = Vec::new();
        for (id, (new, old)) in self.slots.iter().zip(&old.slots).enumerate() {
            if new == old {
                continue;
            }
            if new.differs_in_value_only(old) {
                changes.push(ModelChange::Value { id, value: new.value, value_text: new.value_text.clone() });
            } else {
                changes.push(ModelChange::Slot { id, slot: new.clone() });
            }
        }
        if self.active_row != old.active_row {
            changes.push(ModelChange::ActiveRow(self.active_row));
        }
        if self.selected_slot != old.selected_slot {
            changes.push(ModelChange::SelectedSlot(self.selected_slot));
        }
        if self.active_set != old.active_set {
            changes.push(ModelChange::ActiveSet(self.active_set));
        }
        if self.active_section != old.active_section {
            changes.push(ModelChange::ActiveSection(self.active_section));
        }
        for (index, (new, old)) in self.extras.iter().zip(&old.extras).enumerate() {
            if new != old {
                changes.push(ModelChange::Extra { index, extra: new.clone() });
            }
        }
        changes
    }
}

/// A change of the page model (provider → surfaces): a snapshot on attach and on a page
/// switch, the changes afterwards.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelChange {
    Snapshot(Box<PageModel>),
    /// A slot changed in more than its value.
    Slot { id: usize, slot: Slot },
    /// Only the value of a slot and its text changed.
    Value { id: usize, value: f32, value_text: String },
    ActiveRow(usize),
    SelectedSlot(Option<usize>),
    /// Since 1.2.
    ActiveSet(Option<usize>),
    ActiveSection(Option<usize>),
    Extra { index: usize, extra: Extra },
}

/// What a delta of a turn counts (since 1.2): the source scales it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnUnit {
    /// The travel of an absolute control (a touch strip, a fader): 1 is 1/127 of its length.
    #[default]
    Travel,
    /// One click of a relative encoder.
    Detent,
}

/// The input of a surface (surface → source, §3). Rows and slots count from 0. Deltas are
/// steps of the source: it applies its ranges and its acceleration, so that all surfaces
/// behave the same; `fine` asks for the source's fine resolution, `unit` says what a step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    SlotTurn { id: usize, delta: i32, fine: bool, unit: TurnUnit },
    SlotPress { id: usize },
    SlotRelease { id: usize },
    SlotTouch { id: usize, on: bool },
    RowSelect { row: usize },
    PageSelect { page: usize },
    PageNext,
    PagePrev,
    /// The previous page, on its last control set (paging back through the sets, since 1.4); a
    /// source without control sets handles it as `PagePrev`.
    PagePrevLast,
    SelectedTurn { delta: i32, fine: bool, unit: TurnUnit },
    SelectedReset,
    /// The next / previous value of the selected slot's control, for a control with several
    /// values on one knob (e.g. the handle of an envelope). Since 1.1; a provider without such
    /// controls ignores them.
    ValueNext,
    ValuePrev,
    ExtraTurn { index: usize, delta: i32, fine: bool },
    /// The surface shows (or no longer shows) the source (since 1.2): a host reports its pages
    /// only while one surface shows them.
    Shown { on: bool },
    /// Select a control set, the previous / next one, a section (since 1.2).
    SetSelect { set: usize },
    SetStep { delta: i32 },
    SectionSelect { section: usize },
    /// Assign a slot's control to slot `slot` of the source's remote control page (since 1.3,
    /// the host's remote pages); a source without remote pages ignores it.
    SlotAssign { id: usize, slot: i32 },
}

/// The color depth of a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    Rgb,
    Palette,
    Mono,
}

/// What a surface can do (surface → provider, on attach, §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// The grid size (columns, rows), at least 6x6.
    pub grid: (usize, usize),
    pub encoders: usize,
    pub touch_encoders: bool,
    pub main_encoder: bool,
    pub color_depth: ColorDepth,
    /// The cells the surface shows (columns, rows; since 1.2).
    pub slot_area: (usize, usize),
    /// The encoders which turn the cells of the active row (since 1.2).
    pub row_encoders: usize,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            grid: (GRID_SIZE, GRID_SIZE),
            encoders: GRID_SIZE,
            touch_encoders: false,
            main_encoder: false,
            color_depth: ColorDepth::Palette,
            slot_area: (GRID_SIZE, GRID_SIZE),
            row_encoders: GRID_SIZE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> PageModel {
        let mut model = PageModel { pages: vec!["A".into(), "B".into()], ..PageModel::default() };
        model.slots[0] = Slot { kind: SlotKind::Fader, name: "Vol".into(), value: 0.5, value_text: "0 dB".into(), enabled: true, ..Slot::default() };
        model
    }

    #[test]
    fn diff_and_apply() {
        let old = model();
        let mut new = old.clone();
        assert!(new.diff(&old).is_empty());

        new.slots[0].value = 0.75;
        new.slots[0].value_text = "+3 dB".into();
        new.slots[1].kind = SlotKind::Pad;
        new.active_row = 2;
        new.selected_slot = Some(13);
        let changes = new.diff(&old);
        assert_eq!(changes.len(), 4, "{changes:?}");
        assert!(matches!(&changes[0], ModelChange::Value { id: 0, value_text, .. } if value_text == "+3 dB"));
        assert!(matches!(&changes[1], ModelChange::Slot { id: 1, .. }));
        assert_eq!(changes[2], ModelChange::ActiveRow(2));
        assert_eq!(changes[3], ModelChange::SelectedSlot(Some(13)));
        let mut mirror = old.clone();
        for change in &changes {
            mirror.apply(change);
        }
        assert_eq!(mirror, new);

        // A page switch is a snapshot
        let mut other = new.clone();
        other.page = 1;
        let changes = other.diff(&new);
        assert!(matches!(changes.as_slice(), [ModelChange::Snapshot(m)] if m.page == 1));
        mirror.apply(&changes[0]);
        assert_eq!(mirror.page_name(), "B");
    }

    #[test]
    fn colors_and_slots() {
        assert_eq!(Rgb::from_hex("FF5000"), Rgb::new(255, 0x50, 0));
        assert_eq!(Rgb::from_hex("x"), Rgb::BLACK);
        assert_eq!(Rgb::from(crate::daw::color::ColorEx::ORANGE), Rgb::new(255, 80, 0));
        let model = model();
        assert_eq!(model.control_set(3), 1);
        assert_eq!(model.slot_at(0, 0).unwrap().name, "Vol");
        assert!(model.slot_at(6, 0).is_none());
    }

    #[test]
    fn sizes_sets_and_sections() {
        let mut wide = PageModel { columns: 8, rows: 6, slots: vec![Slot::default(); 48], ..PageModel::default() };
        assert_eq!(wide.slot_id(1, 7), Some(15));
        assert_eq!(wide.slot_id(0, 8), None);
        assert_eq!(wide.slot_id(6, 0), None);
        assert!(!wide.has_sets());
        let old = wide.clone();
        wide.sets = vec![ControlSet { name: "Osc".into(), section: Some(0), color: Rgb::WHITE }];
        // New sets: a snapshot
        assert!(matches!(wide.diff(&old).as_slice(), [ModelChange::Snapshot(_)]));
        let old = wide.clone();
        wide.active_set = Some(0);
        wide.active_section = Some(0);
        wide.slots[3].set = Some(0);
        let changes = wide.diff(&old);
        assert_eq!(changes.len(), 3, "{changes:?}");
        assert!(matches!(changes[0], ModelChange::Slot { id: 3, .. }));
        let mut mirror = old.clone();
        for change in &changes {
            mirror.apply(change);
        }
        assert_eq!(mirror, wide);
        // Another size: a snapshot
        let old = wide.clone();
        wide.columns = 4;
        assert!(matches!(wide.diff(&old).as_slice(), [ModelChange::Snapshot(_)]));
    }
}
