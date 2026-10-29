// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The generic paged bank which is the base of most banks of the model (former `AbstractBank`,
//! `AbstractItemBank`, `AbstractBankImpl` and `AbstractPagedBankImpl`).
//!
//! A bank gives access to a "page" (window) of `page_size` items. Items are addressed either by
//! their `index` in the current page ([`PagedBank::item`]) or by their absolute `position`
//! ([`PagedBank::unpaged_item`]). Indices outside of the existing items return an "empty"
//! placeholder item which reports `does_exist() == false` and whose commands do nothing.

use super::context::DawContext;
use crate::backend::Processor;
use super::events::ModelEvent;

/// Common functions of all items which are managed by a bank (former `IItem`).
pub trait BankItem {
    /// True if the item exists in the DAW.
    fn does_exist(&self) -> bool;
    /// True if the item is selected.
    fn is_selected(&self) -> bool;
    /// The index of the item in the page of its bank (-1 for empty items).
    fn index(&self) -> i32;
    /// The absolute position of the item (-1 for empty items).
    fn position(&self) -> i32;
    /// Select the item in the DAW. Default does nothing.
    fn select(&mut self) {}
}

type Factory<T> = Box<dyn Fn(usize) -> T>;
type EmptyFactory<T> = Box<dyn Fn() -> T>;

/// A bank which stores all items and gives paged access to them.
pub struct PagedBank<T> {
    pub(crate) ctx: DawContext,
    pub(crate) items: Vec<T>,
    empty: T,
    empty_factory: EmptyFactory<T>,
    factory: Factory<T>,
    pub(crate) page_size: usize,
    pub(crate) item_count: usize,
    pub(crate) bank_offset: usize,
    page_event: Option<ModelEvent>,
    /// The name of the bank for the host's `bank` command, if the host can page it.
    host_bank: Option<&'static str>,
    /// True if the host pages the bank (it reported the bank offset, protocol v2): the host only
    /// reports the items of its window and is told each change of the bank offset.
    host_paged: bool,
}

impl<T: BankItem> PagedBank<T> {
    /// Create a bank. `factory` creates the item for a position, `empty_factory` creates the
    /// placeholder for non-existing items.
    pub fn new(
        ctx: DawContext,
        page_size: usize,
        factory: impl Fn(usize) -> T + 'static,
        empty_factory: impl Fn() -> T + 'static,
    ) -> Self {
        PagedBank {
            ctx,
            items: Vec::new(),
            empty: empty_factory(),
            empty_factory: Box::new(empty_factory),
            factory: Box::new(factory),
            page_size,
            item_count: 0,
            bank_offset: 0,
            page_event: None,
            host_bank: None,
            host_paged: false,
        }
    }

    /// Set the name with which the host can page this bank (`bank <name> <offset>`).
    pub(crate) fn set_host_bank(&mut self, name: &'static str) {
        self.host_bank = Some(name);
    }

    /// The host reported the offset of its window: from now on the host pages the bank. Tells
    /// the host the current offset.
    pub(crate) fn enable_host_paging(&mut self) {
        if self.host_paged || self.host_bank.is_none() {
            return;
        }
        self.host_paged = true;
        self.send_bank_offset();
    }

    pub fn is_host_paged(&self) -> bool {
        self.host_paged
    }

    fn send_bank_offset(&self) {
        if let (true, Some(name)) = (self.host_paged, self.host_bank) {
            self.ctx.send_int(Processor::Bank, name, self.bank_offset as i32);
        }
    }

    /// Set the event which is posted when the page changes (former page observers).
    pub(crate) fn set_page_event(&mut self, event: ModelEvent) {
        self.page_event = Some(event);
    }

    /// The number of items in a page.
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The number of all items.
    pub fn item_count(&self) -> usize {
        self.item_count
    }

    /// Set the number of all items. Creates the items if necessary.
    pub fn set_item_count(&mut self, item_count: usize) {
        self.item_count = item_count;
        self.ensure_items(item_count);
        if let Some(pages) = self.item_count.checked_div(self.page_size) {
            let offset = self.bank_offset.min(pages * self.page_size);
            self.set_bank_offset(offset as i64);
        }
    }

    fn ensure_items(&mut self, count: usize) {
        while self.items.len() < count {
            let item = (self.factory)(self.items.len());
            self.items.push(item);
        }
    }

    /// The position of the first item of the page.
    pub fn scroll_position(&self) -> i32 {
        self.bank_offset as i32
    }

    /// Get the item at the given index of the current page. Returns the empty item if there is
    /// no item at that position.
    pub fn item(&self, index: usize) -> &T {
        let id = self.bank_offset + index;
        if id < self.item_count && id < self.items.len() {
            &self.items[id]
        } else {
            &self.empty
        }
    }

    /// Get the item at the given index of the current page for modification. Returns a (fresh)
    /// empty item if there is no item at that position; all commands on it do nothing.
    pub fn item_mut(&mut self, index: usize) -> &mut T {
        let id = self.bank_offset + index;
        if id < self.item_count && id < self.items.len() {
            &mut self.items[id]
        } else {
            self.empty = (self.empty_factory)();
            &mut self.empty
        }
    }

    /// Get the item at the given absolute position. Returns the empty item if it was not yet
    /// created.
    pub fn unpaged_item(&self, position: usize) -> &T {
        self.items.get(position).unwrap_or(&self.empty)
    }

    /// Get the item at the given absolute position. The item is created if necessary.
    pub fn unpaged_item_mut(&mut self, position: usize) -> &mut T {
        self.ensure_items(position + 1);
        &mut self.items[position]
    }

    /// All items which were created so far (might be more than the item count).
    pub fn all_items(&self) -> &[T] {
        &self.items
    }

    pub(crate) fn all_items_mut(&mut self) -> &mut [T] {
        &mut self.items
    }

    /// Iterate over the items of the current page (including empty placeholders).
    pub fn page_items(&self) -> impl Iterator<Item = &T> {
        (0..self.page_size).map(move |i| self.item(i))
    }

    /// True if there is at least one existing item on the current page.
    pub fn has_existing_items(&self) -> bool {
        self.page_items().any(|item| item.does_exist())
    }

    /// Get the selected item of the current page.
    pub fn selected_item(&self) -> Option<&T> {
        self.page_items().find(|item| item.is_selected())
    }

    /// The index (in the page) of the selected item or None.
    pub fn selected_index(&self) -> Option<usize> {
        (0..self.page_size).find(|i| self.item(*i).is_selected())
    }

    /// Get the selected item of the current page for modification.
    pub fn selected_item_mut(&mut self) -> Option<&mut T> {
        let index = self.selected_index()?;
        Some(self.item_mut(index))
    }

    /// Get all selected items of the current page.
    pub fn selected_items(&self) -> Vec<&T> {
        self.page_items().filter(|item| item.is_selected()).collect()
    }

    /// The position of the last existing item on the page, -1 if none.
    pub fn position_of_last_item(&self) -> i32 {
        for i in (0..self.page_size).rev() {
            let item = self.item(i);
            if item.does_exist() && item.position() >= 0 {
                return item.position();
            }
        }
        -1
    }

    /// Can the bank be scrolled backwards (selected item or page)?
    pub fn can_scroll_backwards(&self) -> bool {
        let sel_index = self.selected_item().map(|i| i.index()).unwrap_or(-1);
        sel_index > 0 || self.can_scroll_page_backwards()
    }

    /// Can the bank be scrolled forwards (selected item or page)?
    pub fn can_scroll_forwards(&self) -> bool {
        let sel_index = self.selected_item().map(|i| i.index()).unwrap_or(-1);
        (sel_index >= 0 && (sel_index as usize) + 1 < self.page_size && self.item(sel_index as usize + 1).does_exist())
            || self.can_scroll_page_forwards()
    }

    pub fn can_scroll_page_backwards(&self) -> bool {
        self.bank_offset >= self.page_size
    }

    pub fn can_scroll_page_forwards(&self) -> bool {
        self.bank_offset + self.page_size < self.item_count
    }

    /// Scroll the page by one item backwards.
    pub fn scroll_backwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 - 1);
    }

    /// Scroll the page by one item forwards.
    pub fn scroll_forwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 + 1);
    }

    /// Scroll to the given position. If `adjust_page` is true, the page start is aligned to the
    /// page size.
    pub fn scroll_to(&mut self, position: usize, adjust_page: bool) {
        if position >= self.item_count {
            return;
        }
        let offset = if adjust_page && self.page_size > 0 { position / self.page_size * self.page_size } else { position };
        self.set_bank_offset(offset as i64);
    }

    pub fn scroll_page_backwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 - self.page_size as i64);
    }

    pub fn scroll_page_forwards(&mut self) {
        self.set_bank_offset(self.bank_offset as i64 + self.page_size as i64);
    }

    /// Set the position of the first item of the page (clamped).
    pub fn set_bank_offset(&mut self, bank_offset: i64) {
        let max = (self.item_count as i64 - 1).max(0);
        let old_offset = self.bank_offset;
        self.bank_offset = bank_offset.clamp(0, max) as usize;
        if self.bank_offset != old_offset {
            self.send_bank_offset();
        }
        if let Some(event) = &self.page_event {
            self.ctx.post_event(event.clone());
        }
    }

    /// Select the next item. Switches to the next page if necessary.
    pub fn select_next_item(&mut self) {
        let index = self.selected_item().map(|i| i.index() + 1).unwrap_or(0);
        if index as usize == self.page_size {
            self.select_next_page();
        } else {
            self.item_mut(index.max(0) as usize).select();
        }
    }

    /// Select the previous item. Switches to the previous page if necessary.
    pub fn select_previous_item(&mut self) {
        let index = self.selected_item().map(|i| i.index() - 1).unwrap_or(0);
        if index == -1 {
            self.select_previous_page();
        } else {
            self.item_mut(index as usize).select();
        }
    }

    /// Scroll to the previous page and select the last item.
    pub fn select_previous_page(&mut self) {
        if !self.can_scroll_page_backwards() {
            return;
        }
        self.scroll_page_backwards();
        let last = self.page_size.saturating_sub(1);
        self.item_mut(last).select();
    }

    /// Scroll to the next page and select the first item.
    pub fn select_next_page(&mut self) {
        if !self.can_scroll_page_forwards() {
            return;
        }
        self.scroll_page_forwards();
        self.item_mut(0).select();
    }

    /// Scroll to the page of the item at the given position and select the item.
    pub fn select_item_at_position(&mut self, position: usize) {
        if position >= self.item_count || self.page_size == 0 {
            return;
        }
        let ps = self.page_size;
        self.scroll_to(position / ps * ps, false);
        self.item_mut(position % ps).select();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::backend::{RecordingBackend, Sender};
    use crate::framework::value_changer::ValueChanger;
    use crate::host::{Host, MemoryLog};
    use std::rc::Rc;

    pub(crate) fn test_ctx() -> (DawContext, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        let host = Host::new(Rc::new(MemoryLog::default()));
        let ctx = DawContext::new(host, Sender::new(backend.clone()), ValueChanger::twos_complement(128, 1));
        (ctx, backend)
    }

    struct Item {
        position: i32,
        page_size: i32,
        exists: bool,
        selected: bool,
    }

    impl BankItem for Item {
        fn does_exist(&self) -> bool {
            self.exists
        }
        fn is_selected(&self) -> bool {
            self.exists && self.selected
        }
        fn index(&self) -> i32 {
            if self.position < 0 {
                -1
            } else {
                self.position % self.page_size
            }
        }
        fn position(&self) -> i32 {
            self.position
        }
        fn select(&mut self) {
            self.selected = true;
        }
    }

    #[test]
    fn paging() {
        let (ctx, _) = test_ctx();
        let mut bank = PagedBank::new(
            ctx,
            4,
            |pos| Item { position: pos as i32, page_size: 4, exists: true, selected: false },
            || Item { position: -1, page_size: 4, exists: false, selected: false },
        );
        assert!(!bank.item(0).does_exist());
        bank.set_item_count(10);
        assert_eq!(bank.item(3).position(), 3);
        assert!(!bank.can_scroll_page_backwards());
        assert!(bank.can_scroll_page_forwards());
        bank.select_next_page();
        assert_eq!(bank.scroll_position(), 4);
        assert_eq!(bank.selected_item().unwrap().position(), 4);
        bank.scroll_page_forwards();
        assert_eq!(bank.scroll_position(), 8);
        assert!(bank.item(1).does_exist());
        assert!(!bank.item(2).does_exist());
        assert!(!bank.can_scroll_page_forwards());
        bank.item_mut(3).select();
        assert!(!bank.item(3).is_selected());
        bank.scroll_to(5, true);
        assert_eq!(bank.scroll_position(), 4);
        bank.set_item_count(3);
        assert_eq!(bank.scroll_position(), 0);
        bank.select_item_at_position(2);
        assert!(bank.item(2).is_selected());
    }
}
