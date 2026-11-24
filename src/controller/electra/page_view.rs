// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The page view of the Electra (`01 52`, electra-one `docs/surface-link.md` §6c; not part of
//! Java) as typed data, and its mapping to the 36 slots of the surface link (doc/SURFACE_LINK.md
//! §6.2).
//!
//! **One slot per cell, and a cell is a knob:** cell (row r, column c), both from 0, is knob
//! `c + 1` (even r) or `c + 7` (odd r) of the control set `r / 2 + 1`, as the panel has it. Its
//! slot carries the control and the value which that knob turns:
//! - the active control set: from the device's knob map (`knobs`);
//! - the other sets (and the knobs the map has none for): from the controls' `inputs`. A Custom
//!   control listens to all its inputs (each one a slot of its own value), every other control
//!   to its first input only, which turns its active value (an envelope's or XY pad's handle);
//! - a visible control wins over a hidden one; a hidden one is a disabled slot.
//!
//! A cell which is no control's knob, but which a control covers (an envelope spanning 2x2, the
//! Custom control's other cells), is an `Empty` slot with that control's name and color, not
//! enabled; a press on it selects the control's knob slot. Every other cell is empty.

use crate::framework::surface_link::{PageModel, Rgb, Slot, SlotKind, GRID_SIZE, NUM_SLOTS};
use crate::json::JsonValue;

/// The number of knobs of a control set.
pub const KNOBS: usize = 12;

/// A value of a control.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewValue {
    pub id: String,
    pub min: f64,
    pub max: f64,
    /// As shown: a list's item index.
    pub value: f64,
    pub text: String,
    /// The overlay items (value, label) of a list.
    pub items: Vec<(i64, String)>,
}

/// A control of the page.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewControl {
    pub id: i32,
    /// The type as a preset writes it (`fader`, `list`, `pad`, `adsr`, `custom`, …).
    pub kind: String,
    /// The mode (`momentary`, `toggle` for pads).
    pub mode: String,
    pub name: String,
    pub color: Rgb,
    pub visible: bool,
    /// 1..3.
    pub control_set: usize,
    /// The top left cell (row, column), from 0.
    pub cell: (usize, usize),
    /// The cells covered (columns, rows), at least 1.
    pub span: (usize, usize),
    /// The knobs (1..12) and the values they turn.
    pub inputs: Vec<(usize, String)>,
    pub active_value: String,
    pub values: Vec<ViewValue>,
}

impl ViewControl {
    pub fn value(&self, id: &str) -> Option<&ViewValue> {
        self.values.iter().find(|v| v.id == id)
    }

    pub fn is_custom(&self) -> bool {
        self.kind == "custom"
    }

    /// True if the control has several values, which one knob turns in turn (envelopes, XY
    /// pads), so that `14 56` switches between them.
    pub fn has_handles(&self) -> bool {
        !self.is_custom() && self.values.len() > 1
    }

    /// The knob a control listens to and the value it turns: all inputs of a Custom control,
    /// the first input (with the active value) of every other.
    fn knobs(&self) -> Vec<(usize, String)> {
        if self.is_custom() {
            self.inputs.clone()
        } else {
            self.inputs.first().map(|(pot, _)| (*pot, self.active_value.clone())).into_iter().collect()
        }
    }

    fn covers(&self, row: usize, column: usize) -> bool {
        (self.cell.0..self.cell.0 + self.span.1).contains(&row) && (self.cell.1..self.cell.1 + self.span.0).contains(&column)
    }
}

/// A page of the preset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewPage {
    pub id: usize,
    pub name: String,
    pub hidden: bool,
}

/// The page view.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageView {
    /// The current page, from 1.
    pub page_id: usize,
    pub page_name: String,
    /// The active control set (1..3) and row (0..5).
    pub control_set: usize,
    pub row: usize,
    /// 0 = none.
    pub selected_control: i32,
    /// All pages of the preset (empty if the device does not list them).
    pub pages: Vec<ViewPage>,
    pub controls: Vec<ViewControl>,
    /// The 12 knobs of the active control set: the control and the value each one turns.
    pub knobs: Vec<Option<(i32, String)>>,
}

fn number(json: &JsonValue, key: &str) -> f64 {
    match json.get(key) {
        Some(JsonValue::Number(n)) => *n,
        _ => 0.0,
    }
}

fn text(json: &JsonValue, key: &str) -> String {
    match json.get(key) {
        Some(JsonValue::String(s)) => s.clone(),
        Some(JsonValue::Number(_)) => json.get(key).map(JsonValue::as_text).unwrap_or_default(),
        _ => String::new(),
    }
}

fn index(json: &JsonValue, key: &str) -> usize {
    (number(json, key) as i64).max(0) as usize
}

impl PageView {
    /// Parse the JSON of `01 52`; `None` if it is not a page view.
    pub fn parse(json: &JsonValue) -> Option<PageView> {
        let page = json.get("page")?;
        let controls = json
            .get("controls")?
            .elements()
            .iter()
            .map(|c| {
                let cell = c.get("cell");
                let span = c.get("span");
                ViewControl {
                    id: number(c, "id") as i32,
                    kind: text(c, "type"),
                    mode: text(c, "mode"),
                    name: text(c, "name"),
                    color: Rgb::from_hex(&text(c, "color")),
                    visible: !matches!(c.get("visible"), Some(JsonValue::Bool(false))),
                    control_set: index(c, "controlSetId").max(1),
                    cell: cell.map_or((0, 0), |cell| (index(cell, "row").max(1) - 1, index(cell, "column").max(1) - 1)),
                    span: span.map_or((1, 1), |span| (index(span, "columns").max(1), index(span, "rows").max(1))),
                    inputs: c.get("inputs").map(JsonValue::elements).unwrap_or_default().iter().map(|i| (index(i, "potId"), text(i, "valueId"))).filter(|(pot, _)| (1..=KNOBS).contains(pot)).collect(),
                    active_value: text(c, "activeValueId"),
                    values: c
                        .get("values")
                        .map(JsonValue::elements)
                        .unwrap_or_default()
                        .iter()
                        .map(|v| ViewValue {
                            id: text(v, "id"),
                            min: number(v, "min"),
                            max: number(v, "max"),
                            value: number(v, "value"),
                            text: text(v, "text"),
                            items: v.get("items").map(JsonValue::elements).unwrap_or_default().iter().map(|i| (number(i, "value") as i64, text(i, "label"))).collect(),
                        })
                        .collect(),
                }
            })
            .collect();
        let mut knobs = vec![None; KNOBS];
        for knob in json.get("knobs").map(JsonValue::elements).unwrap_or_default() {
            let pot = index(knob, "potId");
            if let (1..=KNOBS, Some(JsonValue::Number(control)), Some(JsonValue::String(value))) = (pot, knob.get("controlId"), knob.get("valueId")) {
                knobs[pot - 1] = Some((*control as i32, value.clone()));
            }
        }
        Some(PageView {
            page_id: index(page, "id").max(1),
            page_name: text(page, "name"),
            control_set: index(json, "controlSetId").clamp(1, 3),
            row: index(json, "row").clamp(1, 6) - 1,
            selected_control: number(json, "selectedControlId") as i32,
            pages: json
                .get("pages")
                .map(JsonValue::elements)
                .unwrap_or_default()
                .iter()
                .map(|p| ViewPage { id: index(p, "id"), name: text(p, "name"), hidden: matches!(p.get("hidden"), Some(JsonValue::Bool(true))) })
                .collect(),
            controls,
            knobs,
        })
    }

    pub fn control(&self, id: i32) -> Option<&ViewControl> {
        self.controls.iter().find(|c| c.id == id)
    }

    /// A value change (`7E 63`); returns false if the page has no such value.
    pub fn set_value(&mut self, control_id: i32, value_id: &str, value: f64, text: Option<&str>) -> bool {
        let Some(value_entry) = self.controls.iter_mut().find(|c| c.id == control_id).and_then(|c| c.values.iter_mut().find(|v| v.id == value_id)) else {
            return false;
        };
        value_entry.value = value;
        if let Some(text) = text {
            value_entry.text = text.to_string();
        }
        true
    }

    /// The pages shown on the surfaces (not the hidden ones, but always the current one): their
    /// IDs and names. Without a page list, only the current page.
    pub fn shown_pages(&self) -> Vec<(usize, String)> {
        let pages: Vec<(usize, String)> = self.pages.iter().filter(|p| !p.hidden || p.id == self.page_id).map(|p| (p.id, p.name.clone())).collect();
        if pages.iter().any(|(id, _)| *id == self.page_id) {
            pages
        } else {
            vec![(self.page_id, self.page_name.clone())]
        }
    }

    /// What each cell is (see the module documentation).
    pub fn cells(&self) -> Vec<Cell> {
        // The knob map of all sets from the controls (visible ones first)
        let mut by_knob: Vec<Option<(usize, String)>> = vec![None; NUM_SLOTS];
        for visible in [true, false] {
            for (index, control) in self.controls.iter().enumerate() {
                if control.visible != visible || !(1..=3).contains(&control.control_set) {
                    continue;
                }
                for (pot, value_id) in control.knobs() {
                    let slot = knob_slot(control.control_set, pot);
                    if by_knob[slot].is_none() {
                        by_knob[slot] = Some((index, value_id));
                    }
                }
            }
        }
        (0..NUM_SLOTS)
            .map(|slot| {
                let (row, column) = (slot / GRID_SIZE, slot % GRID_SIZE);
                let (set, pot) = slot_knob(slot);
                let from_map = (set == self.control_set)
                    .then(|| self.knobs.get(pot - 1).cloned().flatten())
                    .flatten()
                    .and_then(|(control, value)| self.controls.iter().position(|c| c.id == control).map(|index| (index, value)));
                if let Some((index, value_id)) = from_map.or_else(|| by_knob[slot].clone()) {
                    return Cell::Knob { control: index, value_id };
                }
                // A cell a control covers (one of the same set first)
                let covering = self.controls.iter().enumerate().filter(|(_, c)| c.visible && c.covers(row, column));
                let covering: Vec<usize> = covering.map(|(index, _)| index).collect();
                match covering.iter().find(|i| self.controls[**i].control_set == set).or(covering.first()) {
                    Some(index) => Cell::Covered { control: *index },
                    None => Cell::Empty,
                }
            })
            .collect()
    }

    /// The page model of the surface link.
    pub fn to_model(&self, cells: &[Cell], active_row: usize, selected: Option<usize>) -> PageModel {
        let pages = self.shown_pages();
        let mut model = PageModel {
            page: pages.iter().position(|(id, _)| *id == self.page_id).unwrap_or(0),
            pages: pages.into_iter().map(|(_, name)| name).collect(),
            active_row,
            selected_slot: selected,
            ..PageModel::default()
        };
        for (slot, cell) in cells.iter().enumerate().take(NUM_SLOTS) {
            model.slots[slot] = match cell {
                Cell::Empty => Slot::default(),
                Cell::Covered { control } => {
                    let control = &self.controls[*control];
                    Slot { kind: SlotKind::Empty, name: control.name.clone(), color: control.color, enabled: false, ..Slot::default() }
                }
                Cell::Knob { control, value_id } => knob_slot_model(&self.controls[*control], value_id),
            };
        }
        model
    }
}

/// The slot of the knob `pot` (1..12) of the control set `set` (1..3).
pub fn knob_slot(set: usize, pot: usize) -> usize {
    let row = (set - 1) * 2 + usize::from(pot > GRID_SIZE);
    row * GRID_SIZE + (pot - 1) % GRID_SIZE
}

/// The control set (1..3) and the knob (1..12) of a slot.
pub fn slot_knob(slot: usize) -> (usize, usize) {
    let (row, column) = (slot / GRID_SIZE, slot % GRID_SIZE);
    (row / 2 + 1, column + 1 + if row % 2 == 1 { GRID_SIZE } else { 0 })
}

/// What a cell is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Empty,
    /// A knob: the control (index in `controls`) and the value it turns.
    Knob { control: usize, value_id: String },
    /// Covered by a control which has no knob here.
    Covered { control: usize },
}

/// "attack" → "Attack".
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The slot of a knob.
pub fn knob_slot_model(control: &ViewControl, value_id: &str) -> Slot {
    let several = if control.is_custom() { control.inputs.len() > 1 } else { control.values.len() > 1 };
    let name = if several { format!("{} · {}", control.name, capitalized(value_id)) } else { control.name.clone() };
    let value = control.value(value_id).cloned().unwrap_or_default();
    let range = value.max - value.min;
    let normalized = |v: f64| if range > 0.0 { ((v - value.min) / range).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let (kind, steps, value_01, on) = if control.kind == "list" || !value.items.is_empty() {
        let count = value.items.len().max(1);
        let value_01 = if count > 1 { (value.value / (count - 1) as f64).clamp(0.0, 1.0) as f32 } else { 0.0 };
        (SlotKind::List, count as u32, value_01, false)
    } else {
        match control.kind.as_str() {
            "pad" => (SlotKind::Pad, 0, normalized(value.value), value.value > value.min),
            "relative" => (SlotKind::Relative, 0, 0.0, false),
            "textBox" | "none" => (SlotKind::Empty, 0, 0.0, false),
            _ => (SlotKind::Fader, 0, normalized(value.value), false),
        }
    };
    Slot { kind, name, color: control.color, value: value_01, value_text: value.text, steps, on, enabled: control.visible, set: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The emulator's fixture `tests/fixtures/surface/surface-view.epr` as the page view says it
    /// (shortened, plus a hidden page and a pad in set 3; `tests/fixtures/electra_page_view.json`): an ADSR on 2x2 cells (1), a list (2), a fader (3), a Custom control on three
    /// knobs (4), a hidden fader (6), a relative control (7), one in control set 2 (8).
    pub const VIEW: &str = include_str!("../../../tests/fixtures/electra_page_view.json");

    fn view() -> PageView {
        PageView::parse(&JsonValue::parse(VIEW).unwrap()).unwrap()
    }

    fn model(view: &PageView) -> PageModel {
        view.to_model(&view.cells(), 0, None)
    }

    #[test]
    fn knob_cells() {
        assert_eq!(slot_knob(0), (1, 1));
        assert_eq!(slot_knob(6), (1, 7));
        assert_eq!(slot_knob(17), (2, 6));
        assert_eq!(slot_knob(23), (2, 12));
        assert_eq!(slot_knob(35), (3, 12));
        for slot in 0..NUM_SLOTS {
            let (set, pot) = slot_knob(slot);
            assert_eq!(knob_slot(set, pot), slot);
        }
    }

    #[test]
    fn parse() {
        let view = view();
        assert_eq!((view.page_id, view.page_name.as_str(), view.control_set, view.row, view.selected_control), (1, "VIEW", 1, 0, 0));
        assert_eq!(view.controls.len(), 8);
        let env = view.control(1).unwrap();
        assert_eq!((env.cell, env.span, env.active_value.as_str()), ((0, 0), (2, 2), "attack"));
        assert!(env.has_handles());
        assert!(!view.control(4).unwrap().has_handles());
        assert_eq!(view.control(2).unwrap().value("value").unwrap().items[1], (32, "Square".to_string()));
        assert_eq!(view.knobs[0], Some((1, "attack".to_string())));
        assert_eq!(view.knobs[1], None);
        // The hidden page is not shown
        assert_eq!(view.shown_pages(), [(1, "VIEW".to_string()), (2, "OTHER".to_string())]);
    }

    #[test]
    fn a_single_control() {
        let model = model(&view());
        let cutoff = &model.slots[6 + 2];
        assert_eq!((cutoff.kind, cutoff.name.as_str(), cutoff.value_text.as_str(), cutoff.enabled), (SlotKind::Fader, "CUTOFF", "Cut 64", true));
        assert_eq!(cutoff.color, Rgb::new(0x52, 0x9D, 0xEC));
        assert!((cutoff.value - 64.0 / 127.0).abs() < 1e-6);
        assert_eq!(model.pages, ["VIEW", "OTHER"]);
        assert_eq!(model.page, 0);
    }

    #[test]
    fn an_envelope_with_its_handle() {
        let mut view = view();
        let model = model(&view);
        assert_eq!((model.slots[0].kind, model.slots[0].name.as_str(), model.slots[0].value_text.as_str()), (SlotKind::Fader, "ENV · Attack", "10"));
        // The other cells of its 2x2: covered, in its color, disabled
        for slot in [1, 6, 7] {
            let s = &model.slots[slot];
            assert_eq!((s.kind, s.name.as_str(), s.enabled, s.color), (SlotKind::Empty, "ENV", false, Rgb::new(0x03, 0xA5, 0x98)), "{slot}");
        }
        // Another handle: the knob map names it
        view.knobs[0] = Some((1, "sustain".into()));
        view.controls[0].active_value = "sustain".into();
        let model = view.to_model(&view.cells(), 0, None);
        assert_eq!((model.slots[0].name.as_str(), model.slots[0].value_text.as_str()), ("ENV · Sustain", "30"));
    }

    #[test]
    fn a_list_with_items() {
        let mut view = view();
        let wave = &model(&view).slots[2];
        assert_eq!((wave.kind, wave.steps, wave.value_text.as_str()), (SlotKind::List, 4, "Square"));
        assert!((wave.value - 1.0 / 3.0).abs() < 1e-6);
        assert!(view.set_value(2, "value", 3.0, Some("Sine")));
        let wave = &model(&view).slots[2];
        assert_eq!((wave.value, wave.value_text.as_str()), (1.0, "Sine"));
        assert!(!view.set_value(2, "nothing", 3.0, None));
    }

    #[test]
    fn a_custom_control_on_several_knobs() {
        let model = model(&view());
        let names: Vec<&str> = (3..6).map(|s| model.slots[s].name.as_str()).collect();
        assert_eq!(names, ["GRID · A", "GRID · B", "GRID · C"]);
        assert_eq!(model.slots[5].value, 1.0);
        assert!(model.slots[3..6].iter().all(|s| s.kind == SlotKind::Fader && s.enabled));
    }

    #[test]
    fn hidden_and_relative_controls() {
        let model = model(&view());
        // Knob 11 has no control in the map, the hidden SPARE is its disabled slot
        let spare = &model.slots[6 + 4];
        assert_eq!((spare.kind, spare.name.as_str(), spare.enabled), (SlotKind::Fader, "SPARE", false));
        assert_eq!(model.slots[6 + 5].kind, SlotKind::Relative);
        // Nothing at knob 7 / 8 of set 1 (row 2, columns 1/2 are the envelope's)
        assert_eq!(model.slots[6 + 3].kind, SlotKind::Empty);
        assert_eq!(model.slots[6 + 3].name, "");
    }

    #[test]
    fn other_control_sets() {
        let mut view = view();
        let model = model(&view);
        let set2 = &model.slots[12];
        assert_eq!((set2.kind, set2.name.as_str()), (SlotKind::Fader, "SET2"));
        let pad = &model.slots[24 + 1];
        assert_eq!((pad.kind, pad.on, pad.name.as_str()), (SlotKind::Pad, true, "ON"));
        // With set 2 active, set 1 comes from the inputs: the same slots
        view.control_set = 2;
        view.knobs = vec![None; KNOBS];
        view.knobs[0] = Some((8, "value".into()));
        let other = view.to_model(&view.cells(), 2, None);
        assert_eq!(other.slots, model.slots);
    }
}
