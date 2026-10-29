// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Markers of the arranger (former `IMarker`/`MarkerImpl`, `IMarkerBank`/`MarkerBankImpl`).

use std::ops::{Deref, DerefMut};

use crate::backend::Processor;

use super::actions;
use super::bank::{BankItem, PagedBank};
use super::color::ColorEx;
use super::context::DawContext;
use super::util;

/// A marker.
pub struct Marker {
    ctx: DawContext,
    index: i32,
    position: i32,
    exists: bool,
    selected: bool,
    name: String,
    color: ColorEx,
}

impl Marker {
    pub fn new(ctx: DawContext, index: i32, position: i32) -> Self {
        Marker { ctx, index, position, exists: false, selected: false, name: String::new(), color: ColorEx::new(0.2, 0.2, 0.2) }
    }

    pub fn empty(ctx: &DawContext) -> Self {
        let mut m = Marker::new(ctx.muted(), -1, -1);
        m.color = ColorEx::BLACK;
        m
    }

    /// The name of the marker; "Marker N" if the name is blank.
    pub fn name(&self) -> String {
        if !self.exists {
            return String::new();
        }
        if self.name.trim().is_empty() {
            format!("Marker {}", self.index + 1)
        } else {
            self.name.clone()
        }
    }

    pub fn name_limited(&self, limit: usize) -> String {
        util::optimize_name(&self.name(), limit)
    }

    pub fn color(&self) -> ColorEx {
        self.color
    }

    fn send(&self, command: &str) {
        self.ctx.send(Processor::Marker, &format!("{}/{}", self.position, command));
    }

    /// Move the play cursor to the marker.
    pub fn launch(&mut self, _quantized: bool) {
        self.send("launch");
    }

    pub fn remove_marker(&mut self) {
        self.send("remove");
    }

    // Internal setters

    pub fn set_exists(&mut self, exists: bool) {
        self.exists = exists;
    }

    pub fn set_position(&mut self, position: i32) {
        if position >= 0 {
            self.position = position;
        }
    }

    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_color_state(&mut self, color: [f64; 3]) {
        self.color = ColorEx::from_array(color);
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl BankItem for Marker {
    fn does_exist(&self) -> bool {
        self.exists
    }

    fn is_selected(&self) -> bool {
        self.exists && self.selected
    }

    fn index(&self) -> i32 {
        self.index
    }

    fn position(&self) -> i32 {
        self.position
    }

    fn select(&mut self) {
        self.send("select");
    }
}

/// The bank of markers. Derefs (mutably) to [`PagedBank<Marker>`].
pub struct MarkerBank {
    bank: PagedBank<Marker>,
}

impl MarkerBank {
    pub fn new(ctx: DawContext, num_markers: usize) -> Self {
        let item_ctx = ctx.clone();
        let empty_ctx = ctx.clone();
        let bank = PagedBank::new(
            ctx,
            num_markers,
            move |position| {
                let index = if num_markers == 0 { 0 } else { position % num_markers };
                Marker::new(item_ctx.clone(), index as i32, position as i32)
            },
            move || Marker::empty(&empty_ctx),
        );
        MarkerBank { bank }
    }

    pub fn enable_observers(&self, enable: bool) {
        self.bank.ctx.enable_updates(Processor::Marker, enable);
    }

    /// Add a marker at the play position.
    pub fn add_marker(&self) {
        self.bank.ctx.send(Processor::Marker, "add");
    }

    /// Move the play cursor to the previous marker.
    pub fn select_previous_item(&mut self) {
        self.bank.ctx.invoke_action(actions::GO_TO_PREV_MARKER);
    }

    /// Move the play cursor to the next marker.
    pub fn select_next_item(&mut self) {
        self.bank.ctx.invoke_action(actions::GO_TO_NEXT_MARKER);
    }
}

impl Deref for MarkerBank {
    type Target = PagedBank<Marker>;
    fn deref(&self) -> &PagedBank<Marker> {
        &self.bank
    }
}

impl DerefMut for MarkerBank {
    fn deref_mut(&mut self) -> &mut PagedBank<Marker> {
        &mut self.bank
    }
}
