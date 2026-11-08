// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The macro which expands the command table (TYPED_PROTOCOL_PLAN §2.1).
//!
//! One entry per command:
//!
//! ```text
//! /// doc
//! Name = 0x8301 { field: Type, … } @ <processor> [ <segments> ] => <value>, feature = <F>;
//! ```
//!
//! - `<processor>`: a fixed processor (`Transport`), `(field)` for a field which selects the
//!   processor and is not written into the text (an owner such as `device`/`primary`), or
//!   `[field]` for a field which selects the processor and is also written as segments
//!   ([`super::Chan`]: `track` + `3/…`, `master` + nothing).
//! - `<segments>`: string literals and fields ([`super::legacy::Field`]), joined with `/`.
//! - `=> field`: the field holding the value ([`super::legacy::Value`]); `=> (= Type)`: a
//!   constant value ([`super::legacy::ConstValue`]); nothing: `process_no_arg`.
//! - `feature = F`: the [`HostFeature`](crate::daw::HostFeature) the host needs to understand the
//!   command (documentation; the call sites check it).
//!
//! It expands into `Cmd`, `CmdId`, `Cmd::id`, `Cmd::legacy`, `Cmd::from_legacy` and the metadata
//! table `COMMANDS`.

macro_rules! commands {
    ($(
        $(#[$meta:meta])*
        $name:ident = $code:literal { $($field:ident : $fty:ty),* $(,)? }
            @ $proc:tt [ $($seg:tt)* ] $(=> $val:tt)? $(, feature = $feat:ident)? ;
    )*) => {
        /// A command of the core to the host. The legacy text form is [`Cmd::legacy`].
        #[derive(Debug, Clone, PartialEq)]
        pub enum Cmd<'a> {
            $( $(#[$meta])* $name { $($field: $fty),* }, )*
        }

        /// The ID of a command (stable; the high bit marks the direction core → host).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum CmdId {
            $( $name = $code, )*
        }

        impl CmdId {
            /// All command IDs in the order of the table.
            pub const ALL: &'static [CmdId] = &[$(CmdId::$name),*];

            /// The metadata of the command.
            pub fn info(self) -> &'static CmdInfo {
                COMMANDS.iter().find(|info| info.id == self).expect("every ID is in the table")
            }

            /// Sample commands of this ID for the round-trip test: the k-th sample of every field.
            #[cfg(test)]
            pub(crate) fn sample(self, k: usize) -> Cmd<'static> {
                fn pick<T: $crate::protocol::legacy::Sample>(k: usize) -> T {
                    let mut samples = T::samples();
                    let n = samples.len();
                    samples.swap_remove(k % n)
                }
                let _ = k;
                match self {
                    $( CmdId::$name => Cmd::$name { $($field: pick(k)),* }, )*
                }
            }
        }

        /// The metadata of all commands, in the order of the table.
        pub const COMMANDS: &[CmdInfo] = &[$(
            CmdInfo {
                id: CmdId::$name,
                name: stringify!($name),
                processor: cmd_proc!(info $proc),
                template: cmd_template!($($seg)*),
                fields: &[$((stringify!($field), stringify!($fty))),*],
                value: cmd_val!(info $($val)?),
                feature: cmd_feature!($($feat)?),
            },
        )*];

        impl<'a> Cmd<'a> {
            pub fn id(&self) -> CmdId {
                match self {
                    $( Cmd::$name { .. } => CmdId::$name, )*
                }
            }

            /// The legacy form: the C callback, processor, command and value which the text
            /// protocol delivers (byte-identical to the strings built before the table).
            pub fn legacy(&self) -> Legacy<'_> {
                match self {
                    $( Cmd::$name { $($field),* } => {
                        #[allow(unused_mut)]
                        let mut w = PathWriter::new();
                        $( cmd_seg!(write w $seg); )*
                        let arg = cmd_val!(encode w $($val)?);
                        Legacy { processor: cmd_proc!(encode $proc), command: w.finish(), arg }
                    } )*
                }
            }

            /// The command of a legacy form (the first entry of the table which matches), None if
            /// no entry matches.
            #[allow(clippy::redundant_closure_call)]
            ///
            /// The entries with a fixed processor are tried first, so that an owner field (any
            /// processor, e.g. `<dev> pinned`) does not shadow them (`clip pinned`).
            pub fn from_legacy(processor: Processor, command: &'a str, arg: &'a Arg<'a>) -> Option<Cmd<'a>> {
                for fixed in [true, false] {
                $(
                    if cmd_proc!(fixed $proc) != fixed {
                        // Tried in the other pass
                    } else if let Some(cmd) = (|| -> Option<Cmd<'a>> {
                        cmd_proc!(decode processor $proc);
                        #[allow(unused_mut)]
                        let mut r = PathReader::new(command);
                        $( cmd_seg!(read processor r $seg); )*
                        cmd_proc!(check processor $proc);
                        cmd_val!(decode r arg $($val)?);
                        if !r.is_empty() {
                            return None;
                        }
                        Some(Cmd::$name { $($field),* })
                    })() {
                        return Some(cmd);
                    }
                )*
                }
                None
            }
        }
    };
}

/// The processor of an entry: fixed, `(field)` (not in the text) or `[field]` (in the text).
macro_rules! cmd_proc {
    (info ($f:ident)) => { ProcSpec::Field(stringify!($f)) };
    (info [$f:ident]) => { ProcSpec::Field(stringify!($f)) };
    (info $p:ident) => { ProcSpec::Fixed(Processor::$p) };
    (encode ($f:ident)) => { ProcField::processor($f) };
    (encode [$f:ident]) => { ProcField::processor($f) };
    (encode $p:ident) => { Processor::$p };
    (decode $pr:ident ($f:ident)) => { let $f = ProcField::from_processor($pr)?; };
    (decode $pr:ident [$f:ident]) => {};
    (decode $pr:ident $p:ident) => {
        if $pr != Processor::$p {
            return None;
        }
    };
    (check $pr:ident [$f:ident]) => {
        if ProcField::processor(&$f) != $pr {
            return None;
        }
    };
    (check $pr:ident $other:tt) => {};
    (fixed ($f:ident)) => { false };
    (fixed [$f:ident]) => { false };
    (fixed $p:ident) => { true };
}

/// A segment of the template: a literal or a field.
macro_rules! cmd_seg {
    (write $w:ident $l:literal) => { $w.seg($l) };
    (write $w:ident $f:ident) => { Field::write($f, &mut $w) };
    (read $pr:ident $r:ident $l:literal) => {
        if !$r.literal($l) {
            return None;
        }
    };
    (read $pr:ident $r:ident $f:ident) => { let $f = Field::read($pr, &mut $r)?; };
}

/// The value of an entry: none, a field or a constant.
macro_rules! cmd_val {
    (info) => { None };
    (info (= $t:ty)) => { Some(concat!("= ", stringify!($t))) };
    (info $v:ident) => { Some(stringify!($v)) };
    (encode $w:ident) => { Arg::None };
    (encode $w:ident (= $t:ty)) => { <$t as ConstValue>::encode() };
    (encode $w:ident $v:ident) => { Value::encode($v, &mut $w) };
    (decode $r:ident $a:ident) => {
        if *$a != Arg::None {
            return None;
        }
    };
    (decode $r:ident $a:ident (= $t:ty)) => {
        if !<$t as ConstValue>::matches($a) {
            return None;
        }
    };
    (decode $r:ident $a:ident $v:ident) => { let $v = Value::decode(&mut $r, $a)?; };
}

/// The template text of an entry, e.g. `{ch}/send/{send}/volume`.
macro_rules! cmd_template {
    (@one $l:literal) => { $l };
    (@one $f:ident) => { concat!("{", stringify!($f), "}") };
    () => { "" };
    ($s:tt) => { cmd_template!(@one $s) };
    ($s:tt $($rest:tt)+) => { concat!(cmd_template!(@one $s), "/", cmd_template!($($rest)+)) };
}

macro_rules! cmd_feature {
    () => { None };
    ($f:ident) => { Some(HostFeature::$f) };
}
