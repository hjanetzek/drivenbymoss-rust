// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The typed messages of the surface link (doc/SURFACE_LINK.md §2-§4): the page model which a
//! provider publishes, its changes, the input of the surfaces and their capabilities. Nothing
//! here knows a concrete controller.

/// The number of slot columns and rows of a page.
pub const GRID_SIZE: usize = 6;
/// The number of slots of a page (row by row, the ID of a slot is its index).
pub const NUM_SLOTS: usize = GRID_SIZE * GRID_SIZE;

/// The version of the link protocol. Surfaces and providers with the same major version work
/// together; a different minor version only adds messages or fields which the other side
/// ignores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16) -> Self {
        Version { major, minor }
    }

    /// True if the two sides can talk to each other.
    pub fn is_compatible(self, other: Version) -> bool {
        self.major == other.major
    }
}

/// The version of this build.
/// 1.1: `Input::ValueNext` / `ValuePrev`.
pub const PROTOCOL_VERSION: Version = Version::new(1, 1);

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

/// The page model of a provider (§2).
#[derive(Debug, Clone, PartialEq)]
pub struct PageModel {
    /// The index of the page in `pages`.
    pub page: usize,
    /// The names of all pages (for a page selector).
    pub pages: Vec<String>,
    /// The 36 slots, row by row.
    pub slots: Vec<Slot>,
    /// The number of rows which form a control set (rows 1/2, 3/4 and 5/6 with 2).
    pub rows_per_set: usize,
    /// The row which the encoders of a surface drive (0..5).
    pub active_row: usize,
    /// The slot which the main encoder fine-tunes.
    pub selected_slot: Option<usize>,
    pub extras: Vec<Extra>,
}

impl Default for PageModel {
    fn default() -> Self {
        PageModel { page: 0, pages: Vec::new(), slots: vec![Slot::default(); NUM_SLOTS], rows_per_set: 2, active_row: 0, selected_slot: None, extras: Vec::new() }
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

    /// The slot at the row and column.
    pub fn slot_at(&self, row: usize, column: usize) -> Option<&Slot> {
        (row < GRID_SIZE && column < GRID_SIZE).then(|| self.slots.get(row * GRID_SIZE + column)).flatten()
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
            ModelChange::Extra { index, extra } => {
                if let Some(e) = self.extras.get_mut(*index) {
                    *e = extra.clone();
                }
            }
        }
    }

    /// The changes which turn `old` into `self`: a snapshot if the page, the page list, the
    /// number of slots or extras or the row grouping differ, otherwise one message per changed
    /// part.
    pub fn diff(&self, old: &PageModel) -> Vec<ModelChange> {
        if self.page != old.page
            || self.pages != old.pages
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
    Extra { index: usize, extra: Extra },
}

/// The input of a surface (surface → provider, §3). Rows and slots count from 0. Deltas are
/// steps of the provider: it applies its ranges and its acceleration, so that all surfaces
/// behave the same; `fine` asks for the provider's fine resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    SlotTurn { id: usize, delta: i32, fine: bool },
    SlotPress { id: usize },
    SlotRelease { id: usize },
    SlotTouch { id: usize, on: bool },
    RowSelect { row: usize },
    PageSelect { page: usize },
    PageNext,
    PagePrev,
    SelectedTurn { delta: i32, fine: bool },
    SelectedReset,
    /// The next / previous value of the selected slot's control, for a control with several
    /// values on one knob (e.g. the handle of an envelope). Since 1.1; a provider without such
    /// controls ignores them.
    ValueNext,
    ValuePrev,
    ExtraTurn { index: usize, delta: i32, fine: bool },
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
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities { grid: (GRID_SIZE, GRID_SIZE), encoders: GRID_SIZE, touch_encoders: false, main_encoder: false, color_depth: ColorDepth::Palette }
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
    fn colors_and_versions() {
        assert_eq!(Rgb::from_hex("FF5000"), Rgb::new(255, 0x50, 0));
        assert_eq!(Rgb::from_hex("x"), Rgb::BLACK);
        assert_eq!(Rgb::from(crate::daw::color::ColorEx::ORANGE), Rgb::new(255, 80, 0));
        assert!(PROTOCOL_VERSION.is_compatible(Version::new(1, 7)));
        assert!(!PROTOCOL_VERSION.is_compatible(Version::new(2, 0)));
        let model = model();
        assert_eq!(model.control_set(3), 1);
        assert_eq!(model.slot_at(0, 0).unwrap().name, "Vol");
        assert!(model.slot_at(6, 0).is_none());
    }
}
