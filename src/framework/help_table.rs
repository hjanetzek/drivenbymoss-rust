// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The help texts of a controller as one declarative table (doc/CONTROLLER_HELP_PLAN.md §9): the
//! [`help!`](crate::help) macro turns sections of rows into a [`HelpTable`], which
//! [`super::help::collect`] asks before the commands' own `help` (the escape hatch).
//!
//! ```text
//! help! {
//!     global {
//!         Play                  => "Start playback" long "All combinations ...",
//!         Shift + Play          => "Restart from the beginning",
//!         Select + Shift + Play => |ctx| format!("Punch out: {}", ...),
//!         Scene1 .. Scene8      => |ctx, key| scene_row(ctx, key),
//!         Footswitch1 | Footswitch2 => none,
//!     }
//!     view Play | Piano { ArrowUp => "The notes an octave up", grid => |ctx| grid_play(ctx) }
//!     mode HostStrips { PageLeft => "...", Select + PageLeft => "...", strips => "..." }
//! }
//! ```
//!
//! - **Keys:** a `ButtonId` or `ContinuousId` name (checked at compile time: an unknown name stops
//!   the build), `grid` (the pads), `strips` (the active mode's knobs or strips), `A | B` (the same
//!   text for several), `First .. Last` (a row of buttons: one box for all of them, see
//!   [`HelpKey::Row`]). Modifiers come first: `Shift + Select + Play`; a modifier is held when its
//!   button is (Shift and Select as the surface sees them).
//! - **Texts:** a string literal (with `long "..."`), `none` (explicitly no help: the lookup stops),
//!   `command` (the command's own `help`: the escape hatch for state only the command has),
//!   or a closure `|ctx| ...` / `|ctx, key| ...` returning a `&str`, a `String`, a [`Help`], an
//!   `Option` of these (`None`: this row doesn't apply now, the lookup goes on) or [`NoHelp`]. A
//!   closure row can have `, long "..."` after it.
//! - **Resolution:** the rows with held modifiers first (more modifiers first, then the mode's,
//!   the view's, the global rows, each in their order), then the plain rows (mode, view, global).
//!   A text without a long text gets the long text of the key's first row which has one.
//! - **Scope** (for the colours of the boxes): `modifier` for a row with modifiers, else its
//!   section's: `global`, `view`, `mode`.

use super::ctx::Ctx;
use super::help::Help;
use super::ids::{ButtonId, ContinuousId, Modes, Views};

/// What a help text is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HelpKey {
    Button(ButtonId),
    Continuous(ContinuousId),
    /// The pads.
    Grid,
    /// The knobs or strips of the active mode.
    Controls,
    /// A row of buttons, the first and the last (by their order in `ButtonId`): one box for all.
    Row(ButtonId, ButtonId),
}

const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn button_named(name: &str) -> Option<ButtonId> {
    let mut i = 0;
    while i < ButtonId::NAMES.len() {
        if same(ButtonId::NAMES[i], name) {
            return Some(ButtonId::ALL[i]);
        }
        i += 1;
    }
    None
}

impl HelpKey {
    /// The key of a `ButtonId` or `ContinuousId` name (or `grid`, `strips`); panics for an unknown
    /// name, which stops the build when evaluated in the table (a `static`).
    pub const fn named(name: &str) -> HelpKey {
        if same(name, "grid") {
            return HelpKey::Grid;
        }
        if same(name, "strips") {
            return HelpKey::Controls;
        }
        if let Some(button) = button_named(name) {
            return HelpKey::Button(button);
        }
        let mut i = 0;
        while i < ContinuousId::NAMES.len() {
            if same(ContinuousId::NAMES[i], name) {
                return HelpKey::Continuous(ContinuousId::ALL[i]);
            }
            i += 1;
        }
        panic!("help!: no ButtonId or ContinuousId of this name");
    }

    /// A row of buttons from `first` to `last`.
    pub const fn row(first: &str, last: &str) -> HelpKey {
        match (button_named(first), button_named(last)) {
            (Some(first), Some(last)) => {
                assert!((first as u16) < (last as u16), "help!: a row goes from the first to the last button");
                HelpKey::Row(first, last)
            }
            _ => panic!("help!: a row needs two ButtonId names"),
        }
    }

    /// The buttons of a row (one for a button key, none for the others).
    pub fn buttons(self) -> Vec<ButtonId> {
        match self {
            HelpKey::Button(button) => vec![button],
            HelpKey::Row(first, last) => (first.ordinal()..=last.ordinal()).filter_map(ButtonId::from_ordinal).collect(),
            _ => Vec::new(),
        }
    }

    /// The position of a button in a row (0 for the first), e.g. the scene's index.
    pub fn index_of(self, button: ButtonId) -> Option<usize> {
        self.buttons().iter().position(|b| *b == button)
    }
}

/// The result of a closure row.
pub enum Res {
    Help(Help),
    /// The row doesn't apply now: the lookup goes on.
    Skip,
    /// No help: the lookup stops.
    None,
}

/// Explicitly no help from a closure row (the lookup stops).
pub struct NoHelp;

/// What a closure row may return.
pub trait IntoRes {
    fn into_res(self) -> Res;
}

impl IntoRes for &str {
    fn into_res(self) -> Res {
        Res::Help(Help::new(self))
    }
}

impl IntoRes for String {
    fn into_res(self) -> Res {
        Res::Help(Help::new(self))
    }
}

impl IntoRes for Help {
    fn into_res(self) -> Res {
        Res::Help(self)
    }
}

impl IntoRes for NoHelp {
    fn into_res(self) -> Res {
        Res::None
    }
}

impl<T: IntoRes> IntoRes for Option<T> {
    fn into_res(self) -> Res {
        match self {
            Some(value) => value.into_res(),
            None => Res::Skip,
        }
    }
}

/// The text of a row.
pub enum Text {
    Static(&'static str),
    None,
    /// Ask the command's own `help` (the escape hatch, e.g. for state the command keeps).
    Command,
    Fn(fn(&Ctx, HelpKey) -> Res),
}

/// A row: the modifiers and the key (`chain`, the key last), further keys with the same text, the
/// text and the long text.
pub struct HelpRow {
    pub chain: &'static [HelpKey],
    pub alternatives: &'static [HelpKey],
    pub text: Text,
    pub long: &'static str,
}

impl HelpRow {
    fn key(&self) -> HelpKey {
        self.chain[self.chain.len() - 1]
    }

    fn modifiers(&self) -> &'static [HelpKey] {
        &self.chain[..self.chain.len() - 1]
    }

    fn is_for(&self, key: HelpKey) -> bool {
        self.key() == key || self.alternatives.contains(&key)
    }
}

/// Where the rows of a section apply.
pub enum Section {
    Global,
    View(&'static [Views]),
    Mode(&'static [Modes]),
}

pub struct HelpSection {
    pub section: Section,
    pub rows: &'static [HelpRow],
}

/// The scope of a help text (the colour of its box).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Global,
    View,
    Mode,
    Modifier,
}

impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::View => "view",
            Scope::Mode => "mode",
            Scope::Modifier => "modifier",
        }
    }
}

/// The result of a lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// The table has no row for the key: ask the command, the view, the mode.
    NoRow,
    /// No help now.
    None,
    Help(Help, Scope),
    /// A `command` row: ask the command, with the row's scope.
    Command(Scope),
}

/// The help table of a controller.
pub struct HelpTable {
    pub sections: &'static [HelpSection],
}

fn is_held(ctx: &Ctx, key: HelpKey) -> bool {
    match key {
        HelpKey::Button(ButtonId::Shift) => ctx.surface.is_shift_pressed(),
        HelpKey::Button(ButtonId::Select) => ctx.surface.is_select_pressed(),
        HelpKey::Button(button) => ctx.surface.is_pressed(button),
        _ => false,
    }
}

impl HelpTable {
    /// The sections which apply in the state: the active mode's, the active view's, the global
    /// ones (in this order).
    fn sections_for(&self, view: Option<Views>, mode: Option<Modes>) -> Vec<(&HelpSection, Scope)> {
        let mut result = Vec::new();
        for section in self.sections {
            if let Section::Mode(modes) = section.section {
                if mode.is_some_and(|m| modes.contains(&m)) {
                    result.push((section, Scope::Mode));
                }
            }
        }
        for section in self.sections {
            if let Section::View(views) = section.section {
                if view.is_some_and(|v| views.contains(&v)) {
                    result.push((section, Scope::View));
                }
            }
        }
        for section in self.sections {
            if let Section::Global = section.section {
                result.push((section, Scope::Global));
            }
        }
        result
    }

    /// The help of a key in the current state.
    pub fn lookup(&self, ctx: &Ctx, key: HelpKey) -> Lookup {
        let sections = self.sections_for(ctx.views.active_id(), ctx.modes.active_id());
        let mut candidates: Vec<(&HelpRow, Scope)> = Vec::new();
        let max_modifiers = sections.iter().flat_map(|(s, _)| s.rows.iter()).map(|r| r.modifiers().len()).max().unwrap_or(0);
        // The rows with held modifiers, more modifiers first
        for count in (1..=max_modifiers).rev() {
            for (section, _) in &sections {
                for row in section.rows.iter().filter(|r| r.modifiers().len() == count && r.is_for(key)) {
                    if row.modifiers().iter().all(|m| is_held(ctx, *m)) {
                        candidates.push((row, Scope::Modifier));
                    }
                }
            }
        }
        for (section, scope) in &sections {
            for row in section.rows.iter().filter(|r| r.modifiers().is_empty() && r.is_for(key)) {
                candidates.push((row, *scope));
            }
        }
        if candidates.is_empty() {
            return Lookup::NoRow;
        }
        for (row, scope) in candidates {
            let help = match &row.text {
                Text::Static(text) => Help::new(*text),
                Text::None => return Lookup::None,
                Text::Command => return Lookup::Command(scope),
                Text::Fn(f) => match f(ctx, key) {
                    Res::Help(help) => help,
                    Res::Skip => continue,
                    Res::None => return Lookup::None,
                },
            };
            let mut help = help;
            if help.long.is_empty() {
                let long = if row.long.is_empty() {
                    sections.iter().flat_map(|(s, _)| s.rows.iter()).find(|r| r.is_for(key) && !r.long.is_empty()).map(|r| r.long).unwrap_or("")
                } else {
                    row.long
                };
                help.long = long.to_string();
            }
            return Lookup::Help(help, scope);
        }
        Lookup::None
    }

    /// The long text of the key's first row which has one (in the current state's sections).
    pub fn long_of(&self, ctx: &Ctx, key: HelpKey) -> &'static str {
        let sections = self.sections_for(ctx.views.active_id(), ctx.modes.active_id());
        sections.iter().flat_map(|(s, _)| s.rows.iter()).find(|r| r.is_for(key) && !r.long.is_empty()).map(|r| r.long).unwrap_or("")
    }

    /// True if a row of the global section, the view's or the mode's is for the key (without
    /// modifiers; for the coverage tests).
    pub fn has_entry(&self, view: Option<Views>, mode: Option<Modes>, key: HelpKey) -> bool {
        self.sections_for(view, mode).iter().any(|(section, _)| section.rows.iter().any(|r| r.modifiers().is_empty() && r.is_for(key)))
    }

    /// The rows of buttons the table describes as one box.
    pub fn button_rows(&self) -> Vec<HelpKey> {
        let mut rows = Vec::new();
        for section in self.sections {
            for row in section.rows {
                for key in std::iter::once(row.key()).chain(row.alternatives.iter().copied()) {
                    if matches!(key, HelpKey::Row(..)) && !rows.contains(&key) {
                        rows.push(key);
                    }
                }
            }
        }
        rows
    }
}

/// The chain of a row of [`help!`](crate::help) whose key is a row of buttons ending at `end` (a
/// helper of the macro).
pub const fn row_end<const N: usize>(mut keys: [HelpKey; N], end: Option<&str>) -> [HelpKey; N] {
    if let Some(end) = end {
        let name = match keys[N - 1] {
            HelpKey::Button(button) => ButtonId::NAMES[button as usize],
            _ => panic!("help!: a row goes from a button to a button"),
        };
        keys[N - 1] = HelpKey::row(name, end);
    }
    keys
}

/// The key of a row of [`help!`](crate::help) (a helper of the macro).
#[macro_export]
#[doc(hidden)]
macro_rules! help_key {
    ($name:ident) => {
        $crate::framework::help_table::HelpKey::named(stringify!($name))
    };
}

/// The rows of a section of [`help!`](crate::help) (a helper of the macro: a tt muncher which
/// collects the rows).
#[macro_export]
#[doc(hidden)]
macro_rules! help_rows {
    // The end
    (@rows [$($done:expr,)*]) => {
        &[$($done,)*]
    };
    // A key: modifiers, the key (or a row of keys), alternatives; then the text
    (@rows [$($done:expr,)*] $first:ident $(+ $more:ident)* $(.. $last:ident)? $(| $alt:ident)* => $($rest:tt)*) => {
        $crate::help_rows!(@text [$($done,)*]
            ($crate::framework::help_table::row_end([$crate::help_key!($first), $($crate::help_key!($more),)*], $crate::help_rows!(@end $($last)?)))
            [$($crate::help_key!($alt),)*] $($rest)*)
    };
    (@end) => {
        None
    };
    (@end $last:ident) => {
        Some(stringify!($last))
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt none, long $long:expr $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::framework::help_table::Text::None, $long),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt command, long $long:expr $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::framework::help_table::Text::Command, $long),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt command $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::framework::help_table::Text::Command, ""),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt none $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::framework::help_table::Text::None, ""),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt $text:literal long $long:expr $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::framework::help_table::Text::Static($text), $long),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt $text:literal $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::framework::help_table::Text::Static($text), ""),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt |$ctx:ident, $key:ident| $body:expr, long $long:expr $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::help_rows!(@fn $ctx $key $body), $long),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt |$ctx:ident, $key:ident| $body:expr $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::help_rows!(@fn $ctx $key $body), ""),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt |$ctx:ident| $body:expr, long $long:expr $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::help_rows!(@fn $ctx _key $body), $long),] $($($rest)*)?)
    };
    (@text [$($done:expr,)*] $chain:tt $alts:tt |$ctx:ident| $body:expr $(, $($rest:tt)*)?) => {
        $crate::help_rows!(@rows [$($done,)* $crate::help_rows!(@row $chain $alts $crate::help_rows!(@fn $ctx _key $body), ""),] $($($rest)*)?)
    };
    (@row ($chain:expr) [$($alt:expr,)*] $text:expr, $long:expr) => {
        $crate::framework::help_table::HelpRow { chain: &$chain, alternatives: &[$($alt,)*], text: $text, long: $long }
    };
    (@fn $ctx:ident $key:ident $body:expr) => {{
        #[allow(unused_variables)]
        fn text($ctx: &$crate::framework::ctx::Ctx, $key: $crate::framework::help_table::HelpKey) -> $crate::framework::help_table::Res {
            $crate::framework::help_table::IntoRes::into_res($body)
        }
        $crate::framework::help_table::Text::Fn(text)
    }};
}

/// The help table of a controller, see [`crate::framework::help_table`]. Expands to a
/// `pub static TABLE: HelpTable`.
#[macro_export]
macro_rules! help {
    ($($kind:ident $($name:ident)|* { $($rows:tt)* })*) => {
        pub static TABLE: $crate::framework::help_table::HelpTable = $crate::framework::help_table::HelpTable {
            sections: &[$($crate::framework::help_table::HelpSection {
                section: $crate::help!(@section $kind $($name)|*),
                rows: $crate::help_rows!(@rows [] $($rows)*),
            },)*],
        };
    };
    (@section global) => {
        $crate::framework::help_table::Section::Global
    };
    (@section view $($name:ident)|+) => {
        $crate::framework::help_table::Section::View(&[$($crate::framework::ids::Views::$name),+])
    };
    (@section mode $($name:ident)|+) => {
        $crate::framework::help_table::Section::Mode(&[$($crate::framework::ids::Modes::$name),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    mod table {
        use crate::framework::help::Help;
        crate::help! {
            global {
                Play => "Start" long "All of Play",
                Shift + Play => "Restart",
                Select + Shift + Play => |ctx| Some("Punch out"),
                Scene1 .. Scene8 => |ctx, key| format!("Scenes {:?}", key.buttons().len()),
                Footswitch1 | Footswitch2 => none,
                Record => |ctx| None::<String>,
                MasterKnob => |ctx| Help::new("Turn"),
            }
            view Play | Piano { grid => "Play notes", Play => "Play in the view" }
            mode Volume { strips => "Volume", Shift + Record => "Shifted record" }
        }
    }

    #[test]
    fn keys() {
        assert_eq!(HelpKey::named("Play"), HelpKey::Button(ButtonId::Play));
        assert_eq!(HelpKey::named("MasterKnob"), HelpKey::Continuous(ContinuousId::MasterKnob));
        assert_eq!(HelpKey::named("grid"), HelpKey::Grid);
        let row = HelpKey::row("Scene1", "Scene8");
        assert_eq!(row.buttons().len(), 8);
        assert_eq!(row.index_of(ButtonId::Scene3), Some(2));
    }

    #[test]
    fn table_structure() {
        let table = &table::TABLE;
        assert_eq!(table.sections.len(), 3);
        assert_eq!(table.button_rows(), vec![HelpKey::row("Scene1", "Scene8")]);
        assert!(table.has_entry(None, None, HelpKey::Button(ButtonId::Footswitch2)));
        assert!(table.has_entry(Some(Views::Piano), None, HelpKey::Grid));
        assert!(!table.has_entry(Some(Views::Session), None, HelpKey::Grid));
        assert!(table.has_entry(None, Some(Modes::Volume), HelpKey::Controls));
        let rows = table.sections[0].rows;
        assert_eq!(rows[2].chain.len(), 3);
        assert!(matches!(rows[2].text, Text::Fn(_)));
        assert_eq!(rows[0].long, "All of Play");
    }
}
