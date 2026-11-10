// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device and preset browser (former `BrowserModule`).
//!
//! Differences to Java: `/browser/insertClap` (insert a CLAP plug-in by its ID without the
//! browser) is not supported, the core has no command for it; it is logged.

use super::super::address::{BrowserColumnKeys, BrowserIn, BrowserItemKeys, Dir, FilterOp};
use super::{OscResult, OscShared, Value};
use crate::daw::browser::BrowserColumnItem;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::{Key, KeyList, OscWriter};

/// Former `BrowserModule`.
pub struct BrowserModule {
    is_active: Key<bool>,
    tab: Key<str>,
    columns: KeyList<BrowserColumnKeys>,
    results: KeyList<BrowserItemKeys>,
}

impl BrowserModule {
    pub fn new(w: &mut OscWriter) -> Self {
        BrowserModule {
            is_active: w.key("/browser/isActive"),
            tab: w.key("/browser/tab"),
            columns: KeyList::new("/browser/filter/".into()),
            results: KeyList::new("/browser/result/".into()),
        }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: BrowserIn, value: Value) -> OscResult {
        match command {
            BrowserIn::Preset {} => ctx.model.browse_cursor_device_presets(),
            BrowserIn::Tab { dir } => {
                if let (Some(browser), Some(dir)) = (ctx.model.browser_mut(), dir) {
                    match dir {
                        Dir::Next => browser.next_content_type(),
                        Dir::Previous => browser.previous_content_type(),
                    }
                }
            }
            BrowserIn::Device { before } => {
                let device_name = ctx.model.cursor_device().name().to_string();
                let device_position = ctx.model.cursor_device().position();
                if let Some(browser) = ctx.model.browser_mut() {
                    if before {
                        browser.insert_before_cursor_device(&device_name, device_position);
                    } else {
                        browser.insert_after_cursor_device(&device_name, device_position);
                    }
                }
            }
            BrowserIn::InsertClap {} => {
                if let Some(id) = value.text() {
                    ctx.host.println(&format!("Inserting a CLAP device by its ID is not supported: {id}"));
                }
            }
            BrowserIn::Stop { commit } => {
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.stop_browsing(commit);
                }
            }
            BrowserIn::Filter { column, op } => {
                if let (Some(browser), Some(op)) = (ctx.model.browser_mut(), op) {
                    match op {
                        FilterOp::Next => browser.select_next_filter_item(column),
                        FilterOp::Previous => browser.select_previous_filter_item(column),
                        FilterOp::Reset => browser.reset_filter_column(column),
                    }
                }
            }
            BrowserIn::Result { next } => {
                if let Some(browser) = ctx.model.browser_mut() {
                    if next {
                        browser.select_next_result();
                    } else {
                        browser.select_previous_result();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let browser = ctx.model.browser();
        let is_active = browser.is_some_and(|b| b.is_active());
        w.put(self.is_active, is_active, dump);
        let Some(browser) = browser.filter(|_| is_active) else {
            return;
        };
        w.put(self.tab, browser.selected_content_type(), dump);

        // Filter columns
        for i in 0..browser.filter_column_count() {
            let Some(column) = browser.filter_column(i) else {
                continue;
            };
            let k = self.columns.at(w, i);
            w.put(k.exists, column.exists, dump);
            w.put(k.name, &column.name, dump);
            w.put(k.wildcard, &column.wildcard, dump);
            for (j, item) in column.items.iter().enumerate() {
                let keys = k.items.at(w, j);
                flush_item(w, keys, item, dump);
            }
        }

        // Presets
        for (i, item) in browser.result_items().iter().enumerate() {
            let keys = self.results.at(w, i);
            flush_item(w, keys, item, dump);
        }
    }
}

fn flush_item(w: &mut OscWriter, k: &BrowserItemKeys, item: &BrowserColumnItem, dump: bool) {
    w.put(k.exists, item.exists, dump);
    w.put(k.name, &item.name, dump);
    w.put(k.hits, item.hit_count, dump);
    w.put(k.is_selected, item.is_selected, dump);
}
