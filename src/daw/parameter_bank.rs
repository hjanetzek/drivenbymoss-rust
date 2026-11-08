// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The parameter bank of a device, track or project (former `IParameterBank`,
//! `ParameterBankImpl`) and its page bank (former `IParameterPageBank`, `ParameterPageBankImpl`).
//!
//! The parameter bank derefs to a [`PagedBank<Parameter>`] for the read access. Note that it
//! scrolls by pages: `select_next_item` moves to the next parameter page and `select_next_page`
//! moves by `page_size` pages (as in Java).

use std::collections::HashMap;
use std::ops::Deref;

use crate::backend::Processor;
use crate::protocol::Cmd;

use super::bank::PagedBank;
use super::context::DawContext;
use super::events::{ModelEvent, ParameterBankId};
use super::parameter::{Parameter, ParameterKind};

/// A bank of parameters.
pub struct ParameterBank {
    bank: PagedBank<Parameter>,
    id: ParameterBankId,
    processor: Processor,
    /// The page size of the page bank (number of page names per page).
    num_pages_in_page: usize,
    /// Remembers the selected parameter page per device name.
    selected_device_pages: HashMap<String, usize>,
    device_name: Option<String>,
    /// The names of the parameter pages reported by the host (Bitwig's remote control pages);
    /// empty if the host has no named pages (Reaper).
    page_names: Vec<String>,
    /// The page selected in the host as reported last (Bitwig), None without a report (Reaper).
    host_page: Option<usize>,
}

impl ParameterBank {
    /// Create a parameter bank for the given processor with `num_params` parameters per page.
    pub fn new(ctx: DawContext, id: ParameterBankId, processor: Processor, num_params: usize, has_device: bool) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let ps = num_params.max(1);
        let mut bank = PagedBank::new(
            ctx,
            num_params,
            move |position| {
                let mut p = Parameter::new(item_ctx.clone(), ParameterKind::Device(processor), (position % ps) as i32, 0.0);
                p.set_position(position as i32);
                p
            },
            move || Parameter::empty(&empty_ctx),
        );
        bank.set_page_event(ModelEvent::ParameterPageChanged { bank: id });
        ParameterBank {
            bank,
            id,
            processor,
            num_pages_in_page: num_params,
            selected_device_pages: HashMap::new(),
            device_name: if has_device { Some(String::new()) } else { None },
            page_names: Vec::new(),
            host_page: None,
        }
    }

    /// The ID of the bank which is used in events.
    /// The processor of the parameter commands.
    pub fn processor(&self) -> Processor {
        self.processor
    }

    pub fn id(&self) -> ParameterBankId {
        self.id
    }

    /// Get the parameter at the index of the current page for modification (returns an empty
    /// parameter if it does not exist).
    pub fn item_mut(&mut self, index: usize) -> &mut Parameter {
        self.bank.item_mut(index)
    }

    /// Get the parameter at the absolute position for modification (creates it if necessary).
    pub fn unpaged_item_mut(&mut self, position: usize) -> &mut Parameter {
        self.bank.unpaged_item_mut(position)
    }

    pub fn set_item_count(&mut self, count: usize) {
        self.bank.set_item_count(count);
    }

    /// The number of parameters without parameter mapping (same as `item_count`).
    pub fn unpaged_item_count(&self) -> usize {
        self.bank.item_count()
    }

    fn set_bank_offset(&mut self, offset: i64) {
        self.bank.set_bank_offset(offset);
        if let Some(name) = &self.device_name {
            self.selected_device_pages.insert(name.to_lowercase(), self.bank.bank_offset);
        }
    }

    /// Scroll to the given parameter position. The host is told the selected page of the cursor
    /// device and the first instrument (protocol v2, Bitwig then shows the same remote control
    /// page), of the project, and of the cursor track if the host reports its pages.
    pub fn scroll_to(&mut self, position: i64, adjust_page: bool) {
        if position >= 0 && (position as usize) < self.bank.item_count() {
            let ps = self.bank.page_size().max(1) as i64;
            self.set_bank_offset(if adjust_page { position / ps * ps } else { position });
            if matches!(self.id, ParameterBankId::CursorDevice | ParameterBankId::FirstInstrument)
                || self.processor == Processor::Project
                || (self.id == ParameterBankId::CursorTrack && self.host_page.is_some())
            {
                let page = self.bank.bank_offset as i64 / ps;
                self.bank.ctx.send(Cmd::DeviceParameterPage { dev: self.processor, page: page as i32 });
            }
        }
    }

    /// The page selected in the host (e.g. in Bitwig's remote controls panel): show it, as the
    /// Java version for Bitwig does.
    pub fn set_selected_page_from_host(&mut self, page: usize) {
        self.host_page = Some(page);
        let ps = self.bank.page_size().max(1);
        let position = page * ps;
        if position < self.bank.item_count() && position != self.bank.bank_offset {
            self.set_bank_offset(position as i64);
        }
    }

    /// Set the number of named parameter pages reported by the host.
    pub fn set_page_count(&mut self, count: usize) {
        self.page_names.resize(count, String::new());
    }

    /// Set the name of a parameter page reported by the host.
    pub fn set_page_name(&mut self, page: usize, name: &str) {
        if page >= self.page_names.len() {
            self.page_names.resize(page + 1, String::new());
        }
        self.page_names[page] = name.to_string();
    }

    /// Move to the previous parameter page.
    pub fn scroll_backwards(&mut self) {
        self.select_previous_item();
    }

    /// Move to the next parameter page.
    pub fn scroll_forwards(&mut self) {
        self.select_next_item();
    }

    /// Move to the previous parameter page.
    pub fn select_previous_item(&mut self) {
        let ps = self.bank.page_size() as i64;
        self.scroll_to(self.bank.bank_offset as i64 - ps, true);
    }

    /// Move to the next parameter page.
    pub fn select_next_item(&mut self) {
        let ps = self.bank.page_size() as i64;
        self.scroll_to(self.bank.bank_offset as i64 + ps, true);
    }

    /// Move `page_size` pages backwards.
    pub fn select_previous_page(&mut self) {
        let ps = self.bank.page_size() as i64;
        self.scroll_to(self.bank.bank_offset as i64 - ps * ps, true);
    }

    /// Move `page_size` pages forwards.
    pub fn select_next_page(&mut self) {
        let ps = self.bank.page_size() as i64;
        self.scroll_to(self.bank.bank_offset as i64 + ps * ps, true);
    }

    /// Notify that the value of a parameter was received (former `notifyValueObservers`).
    pub(crate) fn notify_value_observers(&self, param_index: usize) {
        let ps = self.bank.page_size().max(1);
        self.bank.ctx.post_event(ModelEvent::ParameterValueChanged {
            bank: self.id,
            page: (param_index / ps) as i32,
            index: (param_index % ps) as i32,
        });
    }

    /// Called when the name of the device changed: restores the last selected page of that
    /// device.
    pub(crate) fn device_name_changed(&mut self, name: &str) {
        if self.device_name.is_none() {
            return;
        }
        self.device_name = Some(name.to_string());
        // Bitwig remembers the page per device itself (remoteControls.selectedPageIndex ()): show
        // the page it reported last, not the one the core remembers
        if let Some(page) = self.host_page {
            let ps = self.bank.page_size().max(1);
            self.bank.set_bank_offset((page * ps) as i64);
            return;
        }
        let offset = self.selected_device_pages.get(&name.to_lowercase()).copied().unwrap_or(0);
        self.set_bank_offset(offset as i64);
    }

    /// Read access to the page bank.
    pub fn page_bank(&self) -> ParameterPageBank<'_> {
        ParameterPageBank { bank: self }
    }

    // Page bank modifications (former `IParameterPageBank` setters)

    /// Select the page with the given index of the current page bank page.
    pub fn select_page(&mut self, index: usize) {
        let pos = self.page_bank().scroll_position() + index;
        self.page_scroll_to(pos);
    }

    /// Select the page at the given absolute page position.
    pub fn page_scroll_to(&mut self, page: usize) {
        let ps = self.bank.page_size() as i64;
        self.scroll_to(page as i64 * ps, true);
    }

    /// Select the previous page of pages.
    pub fn page_bank_select_previous_page(&mut self) {
        self.select_previous_page();
    }

    /// Select the next page of pages.
    pub fn page_bank_select_next_page(&mut self) {
        self.select_next_page();
    }

    /// Select the previous page.
    pub fn page_bank_scroll_backwards(&mut self) {
        self.scroll_backwards();
    }

    /// Select the next page.
    pub fn page_bank_scroll_forwards(&mut self) {
        self.scroll_forwards();
    }
}

impl Deref for ParameterBank {
    type Target = PagedBank<Parameter>;
    fn deref(&self) -> &PagedBank<Parameter> {
        &self.bank
    }
}

/// Read-only view on the parameter pages of a parameter bank. The items of the page bank are the
/// page names.
pub struct ParameterPageBank<'a> {
    bank: &'a ParameterBank,
}

impl ParameterPageBank<'_> {
    /// The number of page names in a page of the page bank.
    pub fn page_size(&self) -> usize {
        self.bank.num_pages_in_page
    }

    /// The number of parameter pages.
    pub fn item_count(&self) -> usize {
        let item_count = self.bank.item_count();
        let ps = self.bank.page_size();
        if ps == 0 {
            return 0;
        }
        item_count.div_ceil(ps)
    }

    /// The name of the page at the index of the page bank page ("" if it does not exist).
    pub fn item(&self, index: usize) -> String {
        let pos = self.scroll_position() + index;
        if pos >= self.item_count() {
            return String::new();
        }
        match self.bank.page_names.get(pos) {
            Some(name) if !name.is_empty() => name.clone(),
            _ => format!("Page {}", pos + 1),
        }
    }

    /// The absolute position of the selected parameter page.
    pub fn selected_item_position(&self) -> usize {
        let ps = self.bank.page_size();
        if ps == 0 {
            return 0;
        }
        let scroll_position = self.bank.scroll_position() as usize;
        scroll_position.div_ceil(ps)
    }

    /// The index of the selected page in the page bank page.
    pub fn selected_item_index(&self) -> usize {
        let ps = self.page_size();
        if ps == 0 {
            0
        } else {
            self.selected_item_position() % ps
        }
    }

    /// The name of the selected page.
    pub fn selected_item(&self) -> String {
        self.item(self.selected_item_index())
    }

    /// The position of the first page in the page bank page.
    pub fn scroll_position(&self) -> usize {
        let ps = self.page_size();
        self.selected_item_position().checked_div(ps).map(|p| p * ps).unwrap_or(0)
    }

    pub fn can_scroll_backwards(&self) -> bool {
        self.can_scroll_page_backwards()
    }

    pub fn can_scroll_forwards(&self) -> bool {
        self.selected_item_position() < self.bank.item_count()
    }

    pub fn can_scroll_page_backwards(&self) -> bool {
        self.bank.scroll_position() > 0
    }

    /// ParameterPageBankImpl.canScrollPageForwards: a next page of pages exists (`end = sel / ps *
    /// ps + ps; end < itemCount`).
    pub fn can_scroll_page_forwards(&self) -> bool {
        let ps = self.page_size();
        if ps == 0 {
            return false;
        }
        let end = self.selected_item_position() / ps * ps + ps;
        end < self.item_count()
    }

    pub fn position_of_last_item(&self) -> i32 {
        (self.scroll_position() + self.page_size()).min(self.item_count()) as i32 - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daw::bank::tests::test_ctx;

    #[test]
    fn named_pages_and_host_selection() {
        let (ctx, backend) = test_ctx();
        let mut bank = ParameterBank::new(ctx, ParameterBankId::CursorDevice, Processor::Device, 8, true);
        bank.set_item_count(24);
        bank.set_page_count(3);
        bank.set_page_name(1, "Filter");
        assert_eq!(bank.page_bank().item(0), "Page 1");
        assert_eq!(bank.page_bank().item(1), "Filter");
        // The host selected the third page: no command back
        bank.set_selected_page_from_host(2);
        assert_eq!(bank.scroll_position(), 16);
        assert!(backend.take_calls().is_empty());
        // The core selects a page: the host is told
        bank.select_previous_item();
        assert_eq!(backend.take_calls(), vec![crate::backend::BackendCall::IntArg("device".into(), "parameterPage".into(), 1)]);
    }

    /// Bitwig: the page remembered by Bitwig after a device change, ParameterPageBankImpl
    /// paging by pages of pages, touch without holding back the host's values.
    #[test]
    fn host_pages_and_touch() {
        let (ctx, _) = test_ctx();
        ctx.set_host_paging();
        let mut bank = ParameterBank::new(ctx, ParameterBankId::CursorDevice, Processor::Device, 8, true);
        bank.set_item_count(8 * 12);
        bank.device_name_changed("A");
        bank.set_selected_page_from_host(9);
        bank.select_page(1);
        // Another device: Bitwig reports its page; the core does not jump to the page it
        // remembered for the name
        bank.device_name_changed("B");
        assert_eq!(bank.page_bank().selected_item_position(), 9);
        bank.device_name_changed("A");
        assert_eq!(bank.page_bank().selected_item_position(), 9);
        // 12 pages, 8 per page of pages: page 9 is on the second page of pages, no third one
        assert!(!bank.page_bank().can_scroll_page_forwards());
        bank.set_selected_page_from_host(3);
        assert!(bank.page_bank().can_scroll_page_forwards());
        let param = bank.item_mut(0);
        param.set_exists(true);
        param.touch_value(true);
        param.set_internal_value(0.5);
        assert_eq!(param.internal_value(), 0.5);
    }

    #[test]
    fn pages() {
        let (ctx, _) = test_ctx();
        let mut bank = ParameterBank::new(ctx, ParameterBankId::CursorDevice, Processor::Device, 8, true);
        bank.set_item_count(20);
        assert_eq!(bank.page_bank().item_count(), 3);
        assert_eq!(bank.page_bank().item(0), "Page 1");
        assert_eq!(bank.page_bank().item(3), "");
        bank.select_next_item();
        assert_eq!(bank.scroll_position(), 8);
        assert_eq!(bank.page_bank().selected_item_index(), 1);
        bank.select_page(2);
        assert_eq!(bank.scroll_position(), 16);
        assert_eq!(bank.item(3).position(), 19);
        assert!(!bank.item(4).does_exist());
        bank.device_name_changed("Other");
        assert_eq!(bank.scroll_position(), 0);
        bank.select_page(1);
        bank.device_name_changed("Device");
        bank.device_name_changed("other");
        assert_eq!(bank.scroll_position(), 8);
    }
}
