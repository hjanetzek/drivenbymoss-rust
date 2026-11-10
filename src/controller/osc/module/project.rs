// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The project: switching, saving, the audio engine and the project parameters (former
//! `ProjectModule`).

use super::{
    flush_parameter_data, flush_parameter_pages, index_of, is_trigger, parse_fx_param_value, parse_int, sub_command, to_integer, OscError, OscModule, OscResult,
    OscShared, Path, Value, TAG_PAGE,
};
use crate::daw::events::ParameterBankId;
use crate::framework::ctx::Ctx;
use crate::framework::parameterprovider::parameter_bank_mut;

/// Former `ProjectModule`.
pub struct ProjectModule {
    /// The size of a page of the parameter banks, for the empty pages if there is no bank.
    page_size: usize,
}

impl ProjectModule {
    pub fn new(page_size: usize) -> Self {
        ProjectModule { page_size }
    }
}

impl OscModule for ProjectModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["project"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, value: Value) -> OscResult {
        if command != "project" {
            return Err(OscError::UnknownCommand(command.to_string()));
        }
        let sub = sub_command(path)?;
        match sub.as_str() {
            "+" => ctx.model.project_mut().next(),
            "-" => ctx.model.project_mut().previous(),
            "engine" => {
                let application = ctx.model.application_mut();
                if value.is_null() {
                    application.toggle_engine_active();
                } else {
                    application.set_engine_active(is_trigger(value));
                }
            }
            "save" => ctx.model.project_mut().save(),
            "param" => parse_param_value(ctx, ParameterBankId::Project, path, value)?,
            "page" => parse_page_value(ctx, ParameterBankId::Project, path, value)?,
            _ => return Err(OscError::UnknownCommand(sub)),
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let w = &mut shared.writer;
        w.send_str("/project/name", ctx.model.project().name(), dump);
        w.send_bool("/project/engine", ctx.model.application().is_engine_active(), dump);
        flush_parameter_bank(w, "/project/", ctx.model.project().parameter_bank(), self.page_size, &ctx.model, dump);
    }
}

/// Send the parameters and pages of a parameter bank (the common part of the flush methods of
/// the project, track and device modules): `<address>param/<n>/...` and `<address>page/...`.
pub fn flush_parameter_bank(
    writer: &mut crate::framework::osc::OscWriter,
    address: &str,
    bank: Option<&crate::daw::parameter_bank::ParameterBank>,
    page_size: usize,
    model: &crate::daw::model::Model,
    dump: bool,
) {
    let empty = crate::daw::parameter::Parameter::empty(model.ctx());
    let size = bank.map(|b| b.page_size()).unwrap_or(page_size);
    for i in 0..size {
        let parameter = bank.map(|b| b.item(i)).unwrap_or(&empty);
        flush_parameter_data(writer, &format!("{address}param/{}/", i + 1), parameter, dump);
    }
    flush_parameter_pages(writer, address, bank, page_size, dump);
}

/// The parameter commands of a bank (former `parseParamValue` of the project and track modules):
/// `param/{1-8}/value|indicate|reset|touched`, `param/+`, `param/-`, `param/bank/page/{+,-}`.
pub fn parse_param_value(ctx: &mut Ctx, bank_id: ParameterBankId, path: &mut Path, value: Value) -> OscResult {
    let sub = sub_command(path)?;
    if let Some(number) = parse_int(&sub) {
        let index = index_of(number)?;
        return match parameter_bank_mut(&mut ctx.model, bank_id) {
            Some(bank) => parse_fx_param_value(bank.item_mut(index), path, value),
            None => Ok(()),
        };
    }
    match sub.as_str() {
        "+" => {
            if is_trigger(value) {
                if let Some(bank) = parameter_bank_mut(&mut ctx.model, bank_id) {
                    bank.select_next_item();
                }
            }
        }
        "-" => {
            if is_trigger(value) {
                if let Some(bank) = parameter_bank_mut(&mut ctx.model, bank_id) {
                    bank.select_previous_item();
                }
            }
        }
        "bank" => {
            let sub2 = sub_command(path)?;
            if sub2 != TAG_PAGE {
                return Err(OscError::UnknownCommand(sub2));
            }
            let direction = sub_command(path)?;
            if let Some(bank) = parameter_bank_mut(&mut ctx.model, bank_id) {
                if direction == "+" {
                    bank.page_bank_select_next_page();
                } else {
                    bank.page_bank_select_previous_page();
                }
            }
        }
        _ => return Err(OscError::UnknownCommand(sub)),
    }
    Ok(())
}

/// The page commands of a bank (former `parsePageValue` of the project and track modules):
/// `page/select|selected {1-8}`, `page/{1-8}`.
pub fn parse_page_value(ctx: &mut Ctx, bank_id: ParameterBankId, path: &mut Path, value: Value) -> OscResult {
    let sub = sub_command(path)?;
    let index = if sub == "select" || sub == "selected" {
        to_integer(value)? - 1
    } else {
        match parse_int(&sub) {
            Some(number) => number - 1,
            None => return Err(OscError::UnknownCommand(sub)),
        }
    };
    select_page(ctx, bank_id, index);
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
