// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The parameter pages which the host provides (`/hostpage/…`, doc/HOST_PAGE_PLAN.md §3.4, §3.5;
//! not part of Java): the cells of the page the host shows, its control sets and sections, and
//! the host's selection. The parser sets it; a page surface shows it through the host source
//! (`framework::page_surface::host`). The host owns all of it: the core never changes it.

/// The largest number of cells (8 x 8).
pub const MAX_CELLS: usize = 64;
/// The largest number of pages, control sets and sections.
pub const MAX_ITEMS: usize = 256;

/// A cell of the page.
#[derive(Debug, Clone, PartialEq)]
pub struct HostCell {
    /// 0 empty, 1 fader, 2 list, 3 toggle, 4 trigger, 5 relative, 6 action.
    pub kind: i32,
    pub name: String,
    /// RGB, 0..1; None: none.
    pub color: Option<[f64; 3]>,
    /// The normalized value (0..1).
    pub value: f64,
    pub text: String,
    /// The number of items of a list.
    pub steps: i32,
    pub on: bool,
    pub enabled: bool,
    /// The control set (-1: none).
    pub set: i32,
}

impl Default for HostCell {
    fn default() -> Self {
        HostCell { kind: 0, name: String::new(), color: None, value: 0.0, text: String::new(), steps: 0, on: false, enabled: false, set: -1 }
    }
}

/// A control set (zynthian: a page of 4 controllers).
#[derive(Debug, Clone, PartialEq)]
pub struct HostSet {
    pub name: String,
    /// The section (-1: none).
    pub section: i32,
    pub color: Option<[f64; 3]>,
}

impl Default for HostSet {
    fn default() -> Self {
        HostSet { name: String::new(), section: -1, color: None }
    }
}

/// A section (zynthian: the chain's controllers, a processor).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HostSection {
    pub name: String,
    pub color: Option<[f64; 3]>,
}

/// The pages of the host.
#[derive(Debug, Clone, PartialEq)]
pub struct HostPage {
    pub exists: bool,
    pub title: String,
    /// Changed by the host whenever the cells hold other parameters.
    pub epoch: i32,
    pub columns: usize,
    pub rows: usize,
    pub pages: Vec<String>,
    /// The page the host shows.
    pub page: i32,
    pub cells: Vec<HostCell>,
    pub sets: Vec<HostSet>,
    pub set: i32,
    pub sections: Vec<HostSection>,
    pub section: i32,
    /// The active row.
    pub row: i32,
    /// The selected cell (-1: none).
    pub selected_cell: i32,
    /// Counts the changes (every mutable access), so that a source rebuilds only after one.
    generation: u64,
}

impl Default for HostPage {
    fn default() -> Self {
        HostPage {
            exists: false,
            title: String::new(),
            epoch: 0,
            columns: 8,
            rows: 6,
            pages: Vec::new(),
            page: 0,
            cells: Vec::new(),
            sets: Vec::new(),
            set: -1,
            sections: Vec::new(),
            section: -1,
            row: 0,
            selected_cell: -1,
            generation: 0,
        }
    }
}

/// The item of a list, which grows to hold it (up to `max`); None beyond.
fn item<T: Default>(items: &mut Vec<T>, index: usize, max: usize) -> Option<&mut T> {
    if index >= max {
        return None;
    }
    if items.len() <= index {
        items.resize_with(index + 1, T::default);
    }
    items.get_mut(index)
}

/// Resize a list to a reported count.
fn resize<T: Default>(items: &mut Vec<T>, count: i32, max: usize) {
    items.resize_with((count.max(0) as usize).min(max), T::default);
}

impl HostPage {
    /// The number of changes so far.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Mark a change (the parser calls it for every report).
    pub fn touch(&mut self) -> &mut Self {
        self.generation += 1;
        self
    }

    pub fn set_columns(&mut self, columns: i32) {
        self.columns = columns.clamp(1, 8) as usize;
    }

    pub fn set_rows(&mut self, rows: i32) {
        self.rows = rows.clamp(1, 8) as usize;
    }

    pub fn set_page_count(&mut self, count: i32) {
        resize(&mut self.pages, count, MAX_ITEMS);
    }

    pub fn page_name_mut(&mut self, page: usize) -> Option<&mut String> {
        item(&mut self.pages, page, MAX_ITEMS)
    }

    pub fn cell_mut(&mut self, cell: usize) -> Option<&mut HostCell> {
        item(&mut self.cells, cell, MAX_CELLS)
    }

    pub fn set_set_count(&mut self, count: i32) {
        resize(&mut self.sets, count, MAX_ITEMS);
    }

    pub fn set_mut(&mut self, set: usize) -> Option<&mut HostSet> {
        item(&mut self.sets, set, MAX_ITEMS)
    }

    pub fn set_section_count(&mut self, count: i32) {
        resize(&mut self.sections, count, MAX_ITEMS);
    }

    pub fn section_mut(&mut self, section: usize) -> Option<&mut HostSection> {
        item(&mut self.sections, section, MAX_ITEMS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_and_clamps() {
        let mut page = HostPage::default();
        let generation = page.generation();
        page.touch().set_columns(12);
        assert_eq!((page.columns, page.generation()), (8, generation + 1));
        page.set_rows(0);
        assert_eq!(page.rows, 1);
        page.cell_mut(10).unwrap().name = "Cutoff".into();
        assert_eq!(page.cells.len(), 11);
        assert_eq!(page.cells[3].set, -1);
        assert!(page.cell_mut(MAX_CELLS).is_none());
        page.set_page_count(3);
        *page.page_name_mut(1).unwrap() = "Helm".into();
        page.set_page_count(2);
        assert_eq!(page.pages, ["", "Helm"]);
        page.set_set_count(-1);
        assert!(page.sets.is_empty());
    }
}
