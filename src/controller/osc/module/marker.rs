// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The cue markers (former `MarkerModule`).

use super::{index_of, parse_int, send_color, sub_command, OscError, OscModule, OscResult, OscShared, Path, Value, TAG_NAME};
use crate::daw::bank::BankItem;
use crate::framework::ctx::Ctx;

/// Former `MarkerModule`.
#[derive(Default)]
pub struct MarkerModule;

impl OscModule for MarkerModule {
    fn supported_commands(&self) -> &'static [&'static str] {
        &["marker"]
    }

    fn execute(&mut self, ctx: &mut Ctx, _shared: &mut OscShared, command: &str, path: &mut Path, _value: Value) -> OscResult {
        if command != "marker" {
            return Err(OscError::UnknownCommand(command.to_string()));
        }
        let sub = sub_command(path)?;
        if let Some(number) = parse_int(&sub) {
            let index = index_of(number)?;
            let sub2 = sub_command(path)?;
            if sub2 != "launch" {
                return Err(OscError::UnknownCommand(sub2));
            }
            ctx.model.marker_bank_mut().item_mut(index).launch(true);
            return Ok(());
        }
        if sub != "bank" {
            return Err(OscError::UnknownCommand(sub));
        }
        let sub2 = sub_command(path)?;
        let bank = ctx.model.marker_bank_mut();
        match sub2.as_str() {
            "+" => bank.select_next_page(),
            "-" => bank.select_previous_page(),
            _ => return Err(OscError::UnknownCommand(sub2)),
        }
        Ok(())
    }

    fn flush(&mut self, ctx: &Ctx, shared: &mut OscShared, dump: bool) {
        let bank = ctx.model.marker_bank();
        for i in 0..bank.page_size() {
            let address = format!("/marker/{}/", i + 1);
            let marker = bank.item(i);
            shared.writer.send_bool(&format!("{address}exists"), marker.does_exist(), dump);
            shared.writer.send_str(&format!("{address}{TAG_NAME}"), &marker.name(), dump);
            send_color(&mut shared.writer, &format!("{address}color"), marker.color(), dump);
        }
    }
}
