// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The device and preset browser (former `BrowserModule`).
//!
//! Differences to Java: `/browser/insertClap` (insert a CLAP plug-in by its ID without the
//! browser) is not supported, the core has no command for it; it is logged.

use super::{parse_int_or_throw, sub_command, OscError, OscModule, OscResult, OscShared, Path, Value, TAG_EXISTS, TAG_NAME, TAG_RESET};
use crate::daw::browser::BrowserColumnItem;
use crate::framework::ctx::Ctx;
use crate::framework::osc::OscWriter;

/// Former `BrowserModule`.
#[derive(Default)]
pub struct BrowserModule;

impl OscModule for BrowserModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["browser"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        if command != "browser" {
            return Err(OscError::UnknownCommand(command.to_string()));
        }
        let sub = sub_command(path)?;
        let device_name = ctx.model.cursor_device().name().to_string();
        let device_position = ctx.model.cursor_device().position();
        let is_active = ctx.model.browser().is_some_and(|b| b.is_active());
        match sub.as_str() {
            "preset" => ctx.model.browse_cursor_device_presets(),
            "tab" => {
                if !is_active {
                    return Ok(());
                }
                let direction = sub_command(path)?;
                if let Some(browser) = ctx.model.browser_mut() {
                    match direction.as_str() {
                        "+" => browser.next_content_type(),
                        "-" => browser.previous_content_type(),
                        _ => {}
                    }
                }
            }
            "device" => {
                let insert_location = path.pop_front();
                if let Some(browser) = ctx.model.browser_mut() {
                    match insert_location.as_deref() {
                        None | Some("after") => browser.insert_after_cursor_device(&device_name, device_position),
                        Some(_) => browser.insert_before_cursor_device(&device_name, device_position),
                    }
                }
            }
            "insertClap" => {
                if let Some(id) = value.text() {
                    ctx.host.println(&format!("Inserting a CLAP device by its ID is not supported: {id}"));
                }
            }
            "commit" | "cancel" => {
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.stop_browsing(sub == "commit");
                }
            }
            "filter" => {
                let column = parse_int_or_throw(&sub_command(path)?)?;
                if !(1..=6).contains(&column) || !is_active {
                    return Ok(());
                }
                let column = (column - 1) as usize;
                let direction = sub_command(path)?;
                if let Some(browser) = ctx.model.browser_mut() {
                    match direction.as_str() {
                        "+" => browser.select_next_filter_item(column),
                        "-" => browser.select_previous_filter_item(column),
                        TAG_RESET => browser.reset_filter_column(column),
                        _ => {}
                    }
                }
            }
            "result" => {
                if !is_active {
                    return Ok(());
                }
                let direction = path.pop_front().unwrap_or_else(|| "+".to_string());
                if let Some(browser) = ctx.model.browser_mut() {
                    if direction == "+" {
                        browser.select_next_result();
                    } else {
                        browser.select_previous_result();
                    }
                }
            }
            _ => return Err(OscError::UnknownCommand(command.to_string())),
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        let browser = ctx.model.browser();
        let is_active = browser.is_some_and(|b| b.is_active());
        w.send_bool("/browser/isActive", is_active, dump);
        let Some(browser) = browser.filter(|_| is_active) else {
            return;
        };
        w.send_str("/browser/tab", browser.selected_content_type(), dump);

        // Filter columns
        for i in 0..browser.filter_column_count() {
            let Some(column) = browser.filter_column(i) else {
                continue;
            };
            let address = format!("/browser/filter/{}/", i + 1);
            w.send_bool(&format!("{address}{TAG_EXISTS}"), column.exists, dump);
            w.send_str(&format!("{address}{TAG_NAME}"), &column.name, dump);
            w.send_str(&format!("{address}wildcard"), &column.wildcard, dump);
            for (j, item) in column.items.iter().enumerate() {
                flush_item(w, &format!("{address}item/{}/", j + 1), item, dump);
            }
        }

        // Presets
        for (i, item) in browser.result_items().iter().enumerate() {
            flush_item(w, &format!("/browser/result/{}/", i + 1), item, dump);
        }
    }
}

fn flush_item(w: &mut OscWriter, address: &str, item: &BrowserColumnItem, dump: bool) {
    w.send_bool(&format!("{address}{TAG_EXISTS}"), item.exists, dump);
    w.send_str(&format!("{address}{TAG_NAME}"), &item.name, dump);
    w.send_int(&format!("{address}hits"), item.hit_count, dump);
    w.send_bool(&format!("{address}isSelected"), item.is_selected, dump);
}
