// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The help texts of the controls (not part of Java, doc/CONTROLLER_HELP_PLAN.md): what a button
//! (knob, the pads, the strips) does in the current state, for the help boxes of the simulators.
//!
//! - A command describes itself with [`TriggerCommand::help`] (and
//!   [`ContinuousCommand::help`](super::command::ContinuousCommand::help)), a command which hands
//!   the button to the active view or mode asks it ([`View::button_help`](super::feature::View::button_help),
//!   [`Mode::button_help`](super::feature::Mode::button_help)). A button without a command (a pure
//!   modifier such as the JAM's CLEAR) gets its text with [`Ctx::set_button_help`].
//! - The pads are one entry (`grid`, [`View::grid_help`](super::feature::View::grid_help) of the
//!   active view), the knobs or strips of the active mode another one
//!   ([`Mode::controls_help`](super::feature::Mode::controls_help), named by
//!   [`Ctx::help_controls_id`]).
//! - [`collect`] gathers them, [`HelpCache`] turns them into the changes since the last call (the
//!   C ABI `dbm_take_control_help`).

use std::collections::HashMap;
use std::rc::Rc;

use super::command::{ContinuousCommand, TriggerCommand};
use super::ctx::Ctx;
use super::help_table::{HelpKey, Lookup, Scope};
use super::surface::ButtonAction;
use crate::json::JsonWriter;
use crate::midi::{BindType, ButtonEvent};

/// The help of a control: a short text (at most 2 lines, about 60-80 characters: what pressing
/// it does now) and an optional long text (the full description, e.g. all combinations).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Help {
    pub short: String,
    pub long: String,
}

impl Help {
    pub fn new(short: impl Into<String>) -> Self {
        Help { short: short.into(), long: String::new() }
    }

    /// The help with a long text.
    pub fn with_long(mut self, long: impl Into<String>) -> Self {
        self.long = long.into();
        self
    }
}

/// `Some(Help::new(short))`.
pub fn help(short: impl Into<String>) -> Option<Help> {
    Some(Help::new(short))
}

/// `Some(Help::new(short).with_long(long))`.
pub fn help_long(short: impl Into<String>, long: impl Into<String>) -> Option<Help> {
    Some(Help::new(short).with_long(long))
}

/// Computes the help of a control which has no command describing it.
pub type HelpFn = Rc<dyn Fn(&Ctx) -> Option<Help>>;

/// A MIDI binding: type, channel, control.
pub type Binding = (BindType, i32, i32);

/// The help entry of one control (or of the grid, the mode's controls, a row of buttons).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpEntry {
    /// The `ButtonId` or `ContinuousId` name, `grid`, [`Ctx::help_controls_id`], `First..Last`
    /// for a row of buttons, `@view` / `@mode` for the names of the active view and mode.
    pub id: String,
    pub label: String,
    /// The MIDI binding: type, channel, control.
    pub binding: Option<(BindType, i32, i32)>,
    pub pressed: bool,
    /// The number of presses so far (a press between two collections still counts).
    pub presses: u32,
    /// What the text depends on (the colour of the box).
    pub scope: Scope,
    /// The buttons of a row: id and binding.
    pub members: Vec<(String, Option<Binding>)>,
    pub help: Option<Help>,
}

fn write_binding(writer: &mut JsonWriter, binding: Option<(BindType, i32, i32)>) {
    writer.key("bind");
    match binding {
        Some((bind_type, _, _)) => writer.string(bind_type_name(bind_type)),
        None => writer.null(),
    };
    let (channel, control) = binding.map(|(_, channel, control)| (channel, control)).unwrap_or((-1, -1));
    writer.key("channel").number(channel as i64);
    writer.key("control").number(control as i64);
}

impl HelpEntry {
    fn new(id: impl Into<String>, label: impl Into<String>, binding: Option<(BindType, i32, i32)>, help: Option<(Help, Scope)>) -> Self {
        let (help, scope) = match help {
            Some((help, scope)) => (Some(help), scope),
            None => (None, Scope::Global),
        };
        HelpEntry { id: id.into(), label: label.into(), binding, pressed: false, presses: 0, scope, members: Vec::new(), help }
    }

    /// One JSON object (a line of `dbm_take_control_help`).
    pub fn to_json(&self) -> String {
        let mut writer = JsonWriter::new();
        writer.begin_object();
        writer.key("id").string(&self.id);
        writer.key("label").string(&self.label);
        write_binding(&mut writer, self.binding);
        writer.key("pressed").boolean(self.pressed);
        writer.key("presses").number(self.presses as i64);
        writer.key("scope").string(self.scope.name());
        if !self.members.is_empty() {
            writer.key("members").begin_array();
            for (id, binding) in &self.members {
                writer.begin_object();
                writer.key("id").string(id);
                write_binding(&mut writer, *binding);
                writer.end_object();
            }
            writer.end_array();
        }
        let empty = Help::default();
        let help = self.help.as_ref().unwrap_or(&empty);
        writer.key("short").string(&help.short);
        writer.key("long").string(&help.long);
        writer.end_object();
        writer.finish()
    }
}

fn bind_type_name(bind_type: BindType) -> &'static str {
    match bind_type {
        BindType::Cc => "cc",
        BindType::Note => "note",
        BindType::PitchBend => "pitchbend",
        BindType::ProgramChange => "program",
    }
}

/// The id of the grid's entry.
pub const GRID_ID: &str = "grid";
/// The ids of the entries with the names of the active view and mode (in `short`).
pub const VIEW_ID: &str = "@view";
pub const MODE_ID: &str = "@mode";

/// The table's help of a key: Ok(the help) or Err(the scope for the escape hatch).
fn from_table(ctx: &Ctx, key: HelpKey, default_scope: Scope) -> Result<Option<(Help, Scope)>, Scope> {
    match ctx.help_table.map(|table| table.lookup(ctx, key)).unwrap_or(Lookup::NoRow) {
        Lookup::NoRow => Err(default_scope),
        Lookup::Command(scope) => Err(scope),
        Lookup::None => Ok(None),
        Lookup::Help(help, scope) => Ok(Some((help, scope))),
    }
}

fn with_scope(help: Option<Help>, scope: Scope) -> Option<(Help, Scope)> {
    help.map(|help| (help, scope))
}

fn button_binding(button: &super::surface::HwButton) -> Option<(BindType, i32, i32)> {
    button.binding.map(|(bind_type, channel, control, _)| (bind_type, channel, control))
}

/// The help entries of all buttons and continuous controls of the main surface, of the grid
/// (the active view), of the active mode's controls and of the rows of buttons the table
/// describes as one; the table ([`Ctx::help_table`]) first, then the commands', views' and modes'
/// own help (the escape hatch).
pub fn collect(ctx: &Ctx) -> Vec<HelpEntry> {
    let mut entries = Vec::new();
    // The rows of buttons with one text: one entry, else their buttons have their own
    let mut in_rows = std::collections::HashSet::new();
    for row in ctx.help_table.map(|table| table.button_rows()).unwrap_or_default() {
        let buttons: Vec<&super::surface::HwButton> = row.buttons().iter().filter_map(|id| ctx.surface.buttons.get(id)).collect();
        let (Some(first), Some(last)) = (buttons.first(), buttons.last()) else {
            continue;
        };
        let help = from_table(ctx, row, Scope::Global).ok().flatten();
        if help.is_some() {
            in_rows.extend(buttons.iter().map(|b| b.id));
        }
        let mut entry = HelpEntry::new(format!("{}..{}", first.id.name(), last.id.name()), format!("{} – {}", first.label, last.label), None, help);
        entry.pressed = buttons.iter().any(|b| ctx.surface.is_pressed(b.id));
        entry.presses = buttons.iter().map(|b| b.presses).fold(0, u32::wrapping_add);
        entry.members = buttons.iter().map(|b| (b.id.name().to_string(), button_binding(b))).collect();
        entries.push(entry);
    }
    for button in ctx.surface.buttons.values() {
        let help = if in_rows.contains(&button.id) {
            None
        } else {
            from_table(ctx, HelpKey::Button(button.id), Scope::Global).unwrap_or_else(|scope| {
                let help = match (&button.help, &button.action) {
                    (Some(help), _) => help(ctx),
                    (None, ButtonAction::Command(command)) => command.try_borrow().ok().and_then(|command| command.help(ctx)),
                    _ => None,
                };
                with_scope(help, scope).map(|(mut help, scope)| {
                    if help.long.is_empty() {
                        help.long = ctx.help_table.map(|t| t.long_of(ctx, HelpKey::Button(button.id))).unwrap_or("").to_string();
                    }
                    (help, scope)
                })
            })
        };
        let mut entry = HelpEntry::new(button.id.name(), button.label.clone(), button_binding(button), help);
        entry.pressed = ctx.surface.is_pressed(button.id);
        entry.presses = button.presses;
        entries.push(entry);
    }
    for control in ctx.surface.continuous.values() {
        let help = from_table(ctx, HelpKey::Continuous(control.id), Scope::Global).unwrap_or_else(|scope| {
            with_scope(control.command.as_ref().and_then(|command| command.try_borrow().ok().and_then(|command| command.help(ctx))), scope)
        });
        let mut entry = HelpEntry::new(control.id.name(), control.label.clone(), control.binding, help);
        entry.pressed = control.is_touched;
        entries.push(entry);
    }
    if ctx.surface.pad_grid.is_some() {
        let help = from_table(ctx, HelpKey::Grid, Scope::View).unwrap_or_else(|scope| {
            with_scope(ctx.active_view().and_then(|view| view.try_borrow().ok().and_then(|view| view.grid_help(ctx))), scope)
        });
        entries.push(HelpEntry::new(GRID_ID, "Pads", None, help));
    }
    let help = from_table(ctx, HelpKey::Controls, Scope::Mode).unwrap_or_else(|scope| {
        with_scope(ctx.active_mode().and_then(|mode| mode.try_borrow().ok().and_then(|mode| mode.controls_help(ctx))), scope)
    });
    entries.push(HelpEntry::new(ctx.help_controls_id, ctx.help_controls_id, None, help));
    // The names of the active view and mode (the legend of the simulator)
    let view = ctx.active_view().and_then(|view| view.try_borrow().ok().map(|view| view.name().to_string()));
    entries.push(HelpEntry::new(VIEW_ID, "View", None, view.map(|name| (Help::new(name), Scope::View))));
    let mode = ctx.active_mode().and_then(|mode| mode.try_borrow().ok().map(|mode| mode.name().to_string()));
    entries.push(HelpEntry::new(MODE_ID, "Mode", None, mode.map(|name| (Help::new(name), Scope::Mode))));
    entries
}

/// The last line of every entry, to hand out only the changes.
#[derive(Default)]
pub struct HelpCache {
    lines: HashMap<String, String>,
}

impl HelpCache {
    /// Forget everything: the next [`HelpCache::take_changes`] returns all entries with a help.
    pub fn reset(&mut self) {
        self.lines.clear();
    }

    /// The JSON lines of the entries which changed since the last call. An entry seen for the
    /// first time is only returned if it has a help.
    pub fn take_changes(&mut self, entries: &[HelpEntry]) -> String {
        let mut changes = Vec::new();
        for entry in entries {
            let line = entry.to_json();
            match self.lines.get(&entry.id) {
                Some(previous) if *previous == line => continue,
                None if entry.help.is_none() => {}
                _ => changes.push(line.clone()),
            }
            self.lines.insert(entry.id.clone(), line);
        }
        changes.join("\n")
    }
}

/// A trigger command with a help function, e.g. for a closure ([`super::command::FnTrigger`]).
pub struct WithHelp<C: TriggerCommand, H: Fn(&Ctx) -> Option<Help>> {
    inner: C,
    help: H,
}

impl<C: TriggerCommand, H: Fn(&Ctx) -> Option<Help>> WithHelp<C, H> {
    pub fn new(inner: C, help: H) -> Self {
        WithHelp { inner, help }
    }
}

impl<C: TriggerCommand, H: Fn(&Ctx) -> Option<Help>> TriggerCommand for WithHelp<C, H> {
    fn execute(&mut self, ctx: &mut Ctx, event: ButtonEvent, velocity: i32) {
        self.inner.execute(ctx, event, velocity);
    }

    fn help(&self, ctx: &Ctx) -> Option<Help> {
        (self.help)(ctx)
    }
}

/// A continuous command with a help function.
pub struct WithContinuousHelp<C: ContinuousCommand, H: Fn(&Ctx) -> Option<Help>> {
    inner: C,
    help: H,
}

impl<C: ContinuousCommand, H: Fn(&Ctx) -> Option<Help>> WithContinuousHelp<C, H> {
    pub fn new(inner: C, help: H) -> Self {
        WithContinuousHelp { inner, help }
    }
}

impl<C: ContinuousCommand, H: Fn(&Ctx) -> Option<Help>> ContinuousCommand for WithContinuousHelp<C, H> {
    fn execute(&mut self, ctx: &mut Ctx, value: i32) {
        self.inner.execute(ctx, value);
    }

    fn help(&self, ctx: &Ctx) -> Option<Help> {
        (self.help)(ctx)
    }
}

/// Join the lines of a long text.
pub fn lines(lines: &[&str]) -> String {
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, short: Option<&str>) -> HelpEntry {
        HelpEntry::new(id, id.to_uppercase(), Some((BindType::Cc, 0, 7)), short.map(|s| (Help::new(s), Scope::Global)))
    }

    #[test]
    fn json_line() {
        let line = HelpEntry { help: Some(Help::new("Play \"it\"").with_long("a\nb")), ..entry("Play", None) }.to_json();
        assert_eq!(line, r#"{"id":"Play","label":"PLAY","bind":"cc","channel":0,"control":7,"pressed":false,"presses":0,"scope":"global","short":"Play \"it\"","long":"a\nb"}"#);
        let line = HelpEntry { binding: None, ..entry("Shift", None) }.to_json();
        assert!(line.contains(r#""bind":null,"channel":-1,"control":-1"#), "{line}");
    }

    #[test]
    fn only_changes() {
        let mut cache = HelpCache::default();
        let first = cache.take_changes(&[entry("a", Some("A")), entry("b", None)]);
        assert_eq!(first.lines().count(), 1, "an entry without a help is not reported at first: {first}");
        assert_eq!(cache.take_changes(&[entry("a", Some("A")), entry("b", None)]), "");
        let changed = cache.take_changes(&[entry("a", None), entry("b", Some("B"))]);
        assert_eq!(changed.lines().count(), 2, "{changed}");
        assert!(changed.contains(r#""id":"a""#) && changed.contains(r#""short":"""#));
        cache.reset();
        assert_eq!(cache.take_changes(&[entry("a", None), entry("b", Some("B"))]).lines().count(), 1);
    }
}
