// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cache of the controls of a preset page (former `PageCache`): the modes write the values,
//! value texts, names, colors and visibilities of their controls and the group labels into it,
//! and [`PageCache::flush`] sends only what changed since the last flush, wrapped in a pause of
//! the device's repaint.

use super::definition::ctrl_cc;
use super::surface;
use crate::daw::color::ColorEx;
use crate::framework::ctx::Ctx;

/// The number of the group labels which can be cached (former `NUM_GROUPS`).
const NUM_GROUPS: usize = 500;

/// The state of a control: the wanted and the sent values.
#[derive(Debug, Clone, Default, PartialEq)]
struct Control {
    value: i32,
    value_label: Option<String>,
    label: Option<String>,
    color: Option<ColorEx>,
    exists: Option<bool>,
}

impl Control {
    fn reset() -> Self {
        Control { value: -1, ..Control::default() }
    }
}

/// The cache of a preset page (former `PageCache`).
pub struct PageCache {
    page: usize,
    num_rows: usize,
    num_cols: usize,
    group_offset: i32,
    /// The wanted state of the controls (row by row).
    wanted: Vec<Control>,
    /// The state which was sent (former `current*Cache`).
    current: Vec<Control>,
    groups: Vec<Option<String>>,
    current_groups: Vec<Option<String>>,
    is_dirty: bool,
}

impl PageCache {
    pub fn new(page: usize, num_rows: usize, num_cols: usize, group_offset: i32) -> Self {
        let size = num_rows * num_cols;
        // Java starts with 0 values and no texts (reset sets the values to -1)
        PageCache {
            page,
            num_rows,
            num_cols,
            group_offset,
            wanted: vec![Control::default(); size],
            current: vec![Control::default(); size],
            groups: vec![None; NUM_GROUPS],
            current_groups: vec![None; NUM_GROUPS],
            is_dirty: false,
        }
    }

    fn index(&self, row: usize, column: usize) -> usize {
        row * self.num_cols + column
    }

    /// Former `updateValue`: the value and its text (`None` keeps the text).
    pub fn update_value(&mut self, row: usize, column: usize, value: i32, label: Option<&str>) {
        let index = self.index(row, column);
        let control = &mut self.wanted[index];
        if value != control.value {
            self.is_dirty = true;
            control.value = value;
        }
        if let Some(label) = label {
            if control.value_label.as_deref() != Some(label) {
                self.is_dirty = true;
                control.value_label = Some(label.to_string());
            }
        }
    }

    /// Former `updateColor`.
    pub fn update_color(&mut self, row: usize, column: usize, color: ColorEx) {
        self.update_element(row, column, None, Some(color), None);
    }

    /// Former `updateElement`: the name, color and visibility of a control (`None` keeps it).
    pub fn update_element(&mut self, row: usize, column: usize, label: Option<&str>, color: Option<ColorEx>, exists: Option<bool>) {
        let index = self.index(row, column);
        let control = &mut self.wanted[index];
        if let Some(label) = label {
            if control.label.as_deref() != Some(label) {
                self.is_dirty = true;
                control.label = Some(label.to_string());
            }
        }
        if let Some(color) = color {
            if control.color != Some(color) {
                self.is_dirty = true;
                control.color = Some(color);
            }
        }
        if let Some(exists) = exists {
            if control.exists != Some(exists) {
                self.is_dirty = true;
                control.exists = Some(exists);
            }
        }
    }

    /// Former `updateGroupLabel`.
    pub fn update_group_label(&mut self, group_id: i32, label: &str) {
        let Some(group) = usize::try_from(group_id - self.group_offset).ok().filter(|g| *g < NUM_GROUPS) else {
            return;
        };
        if self.groups[group].as_deref() != Some(label) {
            self.is_dirty = true;
            self.groups[group] = Some(label.to_string());
        }
    }

    /// Send all changes to the device (former `flush`).
    pub fn flush(&mut self, ctx: &mut Ctx) {
        if !self.is_dirty {
            return;
        }

        surface::set_repaint_enabled(ctx, false);

        // Flush the values and their texts
        for row in 0..self.num_rows {
            for column in 0..self.num_cols {
                let index = self.index(row, column);
                let wanted = &self.wanted[index];
                let current = &mut self.current[index];
                if wanted.value != current.value {
                    current.value = wanted.value;
                    surface::update_value(ctx, ctrl_cc(row, column), current.value);
                }
                if let Some(label) = &wanted.value_label {
                    if current.value_label.as_ref() != Some(label) {
                        current.value_label = Some(label.clone());
                        surface::update_value_label(ctx, self.control_id(row, column), label);
                    }
                }
            }
        }

        // Flush the names, colors and visibilities of the controls
        for row in 0..self.num_rows {
            for column in 0..self.num_cols {
                let index = self.index(row, column);
                let wanted = &self.wanted[index];
                let current = &mut self.current[index];
                let mut label = None;
                let mut color = None;
                let mut exists = None;
                if wanted.label.is_some() && wanted.label != current.label {
                    current.label = wanted.label.clone();
                    label = wanted.label.clone();
                }
                if wanted.color.is_some() && wanted.color != current.color {
                    current.color = wanted.color;
                    color = wanted.color;
                }
                if wanted.exists.is_some() && wanted.exists != current.exists {
                    current.exists = wanted.exists;
                    exists = wanted.exists;
                }
                if label.is_some() || color.is_some() || exists.is_some() {
                    surface::update_label(ctx, self.control_id(row, column), label.as_deref(), color, exists);
                }
            }
        }

        // Flush the group labels
        for group in 0..NUM_GROUPS {
            if let Some(label) = &self.groups[group] {
                if self.current_groups[group].as_ref() != Some(label) {
                    self.current_groups[group] = Some(label.clone());
                    surface::update_group_label(ctx, self.group_offset + group as i32, label);
                }
            }
        }

        self.is_dirty = false;
        surface::set_repaint_enabled(ctx, true);
    }

    /// The ID of a control on the device (former `getControlID`): counted from 1 over all pages.
    pub fn control_id(&self, row: usize, column: usize) -> i32 {
        (self.page * self.page_size() + self.num_cols * row + column + 1) as i32
    }

    /// The index of a control on its page (former `getIndex`); -1 for the ID 0.
    pub fn index_of(&self, control_id: i32) -> i32 {
        (control_id - 1) % self.page_size() as i32
    }

    /// Former `getPageSize`.
    pub fn page_size(&self) -> usize {
        self.num_rows * self.num_cols
    }

    /// Forget everything, so that the next flush sends all (former `reset`).
    pub fn reset(&mut self) {
        for control in self.wanted.iter_mut().chain(self.current.iter_mut()) {
            *control = Control::reset();
        }
        self.groups.fill(None);
        self.current_groups.fill(None);
    }
}

#[cfg(test)]
mod tests {
    use super::PageCache;

    #[test]
    fn ids() {
        let cache = PageCache::new(2, 6, 6, 500);
        assert_eq!(cache.control_id(0, 0), 73);
        assert_eq!(cache.control_id(5, 5), 108);
        assert_eq!(cache.index_of(73), 0);
        assert_eq!(cache.index_of(108), 35);
        assert_eq!(cache.index_of(0), -1);
        let mini = PageCache::new(1, 2, 8, 8);
        assert_eq!(mini.control_id(1, 7), 32);
        assert_eq!(mini.index_of(17), 0);
    }
}
