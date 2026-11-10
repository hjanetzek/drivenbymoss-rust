// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The project: switching, saving, the audio engine and the project parameters (former
//! `ProjectModule`).

use super::super::address::{Dir, PageArg, ParamBank, ParamBankKeys, ParamIn, ParamOp, ProjectIn};
use super::{flush_parameter_data, is_trigger, to_integer, toggle_or_set, OscResult, OscShared, Value};
use crate::daw::events::ParameterBankId;
use crate::daw::model::Model;
use crate::daw::parameter::Parameter;
use crate::daw::parameter_bank::ParameterBank;
use crate::framework::ctx::Ctx;
use crate::framework::osc::writer::{Key, KeyGroup, OscWriter};
use crate::framework::parameterprovider::parameter_bank_mut;

/// Former `ProjectModule`.
pub struct ProjectModule {
    /// The size of a page of the parameter banks, for the empty pages if there is no bank.
    page_size: usize,
    name: Key<str>,
    engine: Key<bool>,
    params: ParamBankKeys,
}

impl ProjectModule {
    pub fn new(w: &mut OscWriter, page_size: usize) -> Self {
        ProjectModule { page_size, name: w.key("/project/name"), engine: w.key("/project/engine"), params: ParamBankKeys::new(w, "/project/") }
    }

    pub fn execute(&mut self, ctx: &mut Ctx, command: ProjectIn, value: Value) -> OscResult {
        match command {
            ProjectIn::Switch { dir: Dir::Next } => ctx.model.project_mut().next(),
            ProjectIn::Switch { dir: Dir::Previous } => ctx.model.project_mut().previous(),
            ProjectIn::Engine {} => toggle_or_set(ctx.model.application_mut(), value, |a| a.toggle_engine_active(), |a, on| a.set_engine_active(on)),
            ProjectIn::Save {} => ctx.model.project_mut().save(),
        }
        Ok(())
    }

    pub fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        w.put(self.name, ctx.model.project().name(), dump);
        w.put(self.engine, ctx.model.application().is_engine_active(), dump);
        flush_parameter_bank(w, &mut self.params, ctx.model.project().parameter_bank(), self.page_size, &ctx.model, dump);
    }
}

/// Send the parameters and pages of a parameter bank (the common part of the flush methods of
/// the project, track and device modules): `<prefix>param/<n>/...` and `<prefix>page/...`.
/// Without a bank (e.g. no project parameters on the host) `page_size` empty parameters and
/// pages are sent.
pub fn flush_parameter_bank(w: &mut OscWriter, keys: &mut ParamBankKeys, bank: Option<&ParameterBank>, page_size: usize, model: &Model, dump: bool) {
    let empty = Parameter::empty(model.ctx());
    let size = bank.map(|b| b.page_size()).unwrap_or(page_size);
    for i in 0..size {
        let parameter = bank.map(|b| b.item(i)).unwrap_or(&empty);
        let k = keys.params.at(w, i);
        flush_parameter_data(w, k, parameter, dump);
    }

    // The pages
    let page_bank = bank.map(|b| b.page_bank());
    let selected = page_bank.as_ref().map(|p| p.selected_item_index());
    let page_size = page_bank.as_ref().map(|p| p.page_size()).unwrap_or(page_size);
    for i in 0..page_size {
        let page_name = page_bank.as_ref().map(|p| p.item(i)).unwrap_or_default();
        let k = keys.pages.at(w, i);
        w.put(k.exists, !page_name.trim().is_empty(), dump);
        w.put(k.page, &page_name, dump);
        w.put(k.name, &page_name, dump);
        w.put(k.selected, selected == Some(i), dump);
    }
    let selected_name = page_bank.as_ref().map(|p| p.selected_item()).unwrap_or_default();
    w.put(keys.selected_page_name, &selected_name, dump);
}

/// Execute a parameter command (former `parseFXParamValue` of the modules).
pub fn execute_param_op(parameter: &mut Parameter, op: ParamOp, value: Value) -> OscResult {
    match op {
        ParamOp::Value => parameter.set_value(to_integer(value)?),
        ParamOp::Indicate => parameter.set_indication(is_trigger(value)),
        ParamOp::Reset => parameter.reset_value(),
        ParamOp::Touched => parameter.touch_value(is_trigger(value)),
    }
    Ok(())
}

fn bank(ctx: &mut Ctx, bank: ParamBank) -> Option<&mut ParameterBank> {
    parameter_bank_mut(&mut ctx.model, bank.id())
}

/// The commands of the parameter banks (former `parseParamValue` and `parsePageValue` of the
/// project and track modules and the parameter part of `parseDeviceValue`).
pub fn execute_param(ctx: &mut Ctx, command: ParamIn, value: Value) -> OscResult {
    match command {
        ParamIn::Value { bank: id, param, op } => {
            if let Some(bank) = bank(ctx, id) {
                execute_param_op(bank.item_mut(param), op, value)?;
            }
        }
        ParamIn::SelectItem { bank: id, dir } => {
            if is_trigger(value) {
                if let Some(bank) = bank(ctx, id) {
                    match dir {
                        Dir::Next => bank.select_next_item(),
                        Dir::Previous => bank.select_previous_item(),
                    }
                }
            }
        }
        ParamIn::ScrollPages { bank: id, dir } => {
            if let Some(bank) = bank(ctx, id) {
                match dir {
                    Dir::Next => bank.page_bank_scroll_forwards(),
                    Dir::Previous => bank.page_bank_scroll_backwards(),
                }
            }
        }
        ParamIn::PageBank { bank: id, dir } => {
            if let Some(bank) = bank(ctx, id) {
                match dir {
                    Dir::Next => bank.page_bank_select_next_page(),
                    Dir::Previous => bank.page_bank_select_previous_page(),
                }
            }
        }
        ParamIn::SelectPage { bank: id, page } => {
            let index = match page {
                PageArg::FromValue => to_integer(value)? - 1,
                PageArg::Index(index) => index,
            };
            select_page(ctx, id.id(), index);
        }
    }
    Ok(())
}

/// Select a parameter page by its index in the page bank page (former
/// `IParameterPageBank.selectPage`); a negative index does nothing.
pub fn select_page(ctx: &mut Ctx, bank_id: ParameterBankId, index: i32) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    if let Some(bank) = parameter_bank_mut(&mut ctx.model, bank_id) {
        bank.select_page(index);
    }
}
