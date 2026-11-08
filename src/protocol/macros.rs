// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The macro which expands the command table (TYPED_PROTOCOL_PLAN §2.1).
//!
//! One entry per command:
//!
//! ```text
//! /// doc
//! Name = 0x8301 { field: Type, … } @ <processor> [ <segments> ] => value: Type, feature = <F>;
//! ```
//!
//! - `<processor>`: a fixed processor (`Transport`), `(field)` for a field which selects the
//!   processor and is not written into the text (an owner such as `device`/`primary`), or
//!   `[field]` for a field which selects the processor and is also written as segments
//!   ([`super::Chan`]: `track` + `3/…`, `master` + nothing).
//! - `<segments>`: string literals and fields ([`super::legacy::Field`]), joined with `/`.
//! - `=> value: Type`: the value ([`super::legacy::Value`], [`super::bin::BinValue`]);
//!   `= Type`: a constant value ([`super::legacy::ConstValue`]); nothing: `process_no_arg`. The
//!   fields in the braces are the indices of the binary form ([`super::bin::BinIdx`]).
//! - `feature = F`: the [`HostFeature`](crate::daw::HostFeature) the host needs to understand the
//!   command (documentation; the call sites check it).
//!
//! It expands into `Cmd`, `CmdId`, `Cmd::id`, `Cmd::legacy`, `Cmd::from_legacy` and the metadata
//! table `COMMANDS`.

macro_rules! commands {
    ($(
        $(#[$meta:meta])*
        $name:ident = $code:literal { $($field:ident : $fty:ty),* $(,)? }
            @ $proc:tt [ $($seg:tt)* ] $(=> $val:ident : $vty:ty)? $(= $cty:ty)? $(, feature = $feat:ident)? ;
    )*) => {
        /// A command of the core to the host. The legacy text form is [`Cmd::legacy`].
        #[derive(Debug, Clone, PartialEq)]
        pub enum Cmd<'a> {
            $( $(#[$meta])* $name { $($field: $fty,)* $($val: $vty)? }, )*
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

            /// The ID of a number, None if it is not in the table.
            pub fn from_u16(id: u16) -> Option<CmdId> {
                match id {
                    $( $code => Some(CmdId::$name), )*
                    _ => None,
                }
            }

            /// The index slots of the binary form, in the order of `idx`.
            #[allow(clippy::extra_unused_lifetimes)]
            pub fn index_slots<'a>(self) -> Vec<IdxSlot> {
                match self {
                    $( CmdId::$name => {
                        #[allow(unused_mut)]
                        let mut slots = Vec::new();
                        $( <$fty as BinIdx>::describe(stringify!($field), &mut slots); )*
                        slots
                    } )*
                }
            }

            /// The value type of the binary form ([`super::bin::ty`]).
            #[allow(clippy::extra_unused_lifetimes)]
            pub fn value_type<'a>(self) -> u8 {
                match self {
                    $( CmdId::$name => {
                        #[allow(unused_mut, unused_assignments)]
                        let mut t = super::bin::ty::NONE;
                        $( t = <$vty as BinValue<'a>>::TYPE; )?
                        t
                    } )*
                }
            }

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
                    $( CmdId::$name => Cmd::$name { $($field: pick(k),)* $($val: pick(k))? }, )*
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
                fields: &[$((stringify!($field), stringify!($fty)),)* $((stringify!($val), stringify!($vty)))?],
                value: cmd_val!(info $($val)? $(= $cty)?),
                feature: cmd_feature!($($feat)?),
            },
        )*];

        impl<'a> Cmd<'a> {
            pub fn id(&self) -> CmdId {
                match self {
                    $( Cmd::$name { .. } => CmdId::$name, )*
                }
            }

            /// Append the record of the command (the binary form).
            pub fn encode(&self, w: &mut Writer) {
                match self {
                    $( Cmd::$name { $($field,)* $($val)? } => w.record($code, |_idx, _val| {
                        $( BinIdx::put($field, _idx); )*
                        $( BinValue::encode($val, _val); )?
                        $( <$cty as BinConst>::encode(_val); )?
                    }), )*
                }
            }

            /// The command of a record.
            pub fn decode_record(record: &Record<'a>) -> Result<Cmd<'a>, BinError> {
                let id = CmdId::from_u16(record.id).ok_or(BinError::UnknownId(record.id))?;
                let bad = BinError::Value(record.id);
                #[allow(unused_mut)]
                let mut _idx = IdxIn::new(record.idx);
                match id {
                    $( CmdId::$name => {
                        $( let $field = BinIdx::get(&mut _idx).ok_or(bad)?; )*
                        $( let $val = BinValue::decode(record).ok_or(bad)?; )?
                        $( if !<$cty as BinConst>::matches(record) { return Err(bad); } )?
                        cmd_none!(record bad $($val)? $(= $cty)?);
                        Ok(Cmd::$name { $($field,)* $($val)? })
                    } )*
                }
            }

            /// The legacy form: the C callback, processor, command and value which the text
            /// protocol delivers (byte-identical to the strings built before the table).
            pub fn legacy(&self) -> Legacy<'_> {
                match self {
                    $( Cmd::$name { $($field,)* $($val)? } => {
                        #[allow(unused_mut)]
                        let mut w = PathWriter::new();
                        $( cmd_seg!(write w $seg); )*
                        let arg = cmd_val!(encode w $($val)? $(= $cty)?);
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
                        cmd_val!(decode r arg $($val)? $(= $cty)?);
                        if !r.is_empty() {
                            return None;
                        }
                        Some(Cmd::$name { $($field,)* $($val)? })
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
    (info = $t:ty) => { Some(concat!("= ", stringify!($t))) };
    (info $v:ident) => { Some(stringify!($v)) };
    (encode $w:ident) => { Arg::None };
    (encode $w:ident = $t:ty) => { <$t as ConstValue>::encode() };
    (encode $w:ident $v:ident) => { Value::encode($v, &mut $w) };
    (decode $r:ident $a:ident) => {
        if *$a != Arg::None {
            return None;
        }
    };
    (decode $r:ident $a:ident = $t:ty) => {
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

/// A command without a value: the record has none.
macro_rules! cmd_none {
    ($r:ident $bad:ident) => {
        if !super::bin::no_value($r) {
            return Err($bad);
        }
    };
    ($r:ident $bad:ident $($rest:tt)+) => {};
}

macro_rules! cmd_feature {
    () => { None };
    ($f:ident) => { Some(HostFeature::$f) };
}

/// The macro which expands the address table of the reports ([`super::msg`]).
///
/// One entry per message:
///
/// ```text
/// /// doc
/// Name = 0x0301 { field: Type, … } [ <segments> ] => value: Type = <rule>;
/// ```
///
/// - `<segments>`: string literals, `("lit")` for an optional literal (accepted with and without,
///   written with it) and fields ([`super::text::Seg`]), joined with `/`. The text form is the
///   address `/<segments>`.
/// - `=> value: Type = rule`: the value and its parser in [`super::text::rules`], e.g. `= f64`,
///   `= int_or(-1)`; nothing: the value is ignored. The fields in the braces are the indices of
///   the binary form ([`super::bin::BinIdx`]).
///
/// It expands into `Msg`, `MsgId`, `Msg::id`, `Msg::to_text`, `MsgId::decode`, `MsgId::patterns`
/// and the metadata table `MESSAGES`.
macro_rules! messages {
    ($(
        $(#[$meta:meta])*
        $name:ident = $code:literal { $($field:ident : $fty:ty),* $(,)? }
            [ $($seg:tt)* ] $(=> $val:ident : $vty:ty = $rule:ident $(( $($arg:expr),* ))?)? ;
    )*) => {
        /// A report of the host to the core. The text form is [`Msg::to_text`].
        #[derive(Debug, Clone, PartialEq)]
        pub enum Msg<'a> {
            $( $(#[$meta])* $name { $($field: $fty,)* $($val: $vty)? }, )*
        }

        /// The ID of a report (stable; the high bit is clear: host → core).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum MsgId {
            $( $name = $code, )*
        }

        impl MsgId {
            /// All message IDs in the order of the table.
            pub const ALL: &'static [MsgId] = &[$(MsgId::$name),*];

            /// The metadata of the message.
            pub fn info(self) -> &'static MsgInfo {
                MESSAGES.iter().find(|info| info.id == self).expect("every ID is in the table")
            }

            /// The ID of a number, None if it is not in the table.
            pub fn from_u16(id: u16) -> Option<MsgId> {
                match id {
                    $( $code => Some(MsgId::$name), )*
                    _ => None,
                }
            }

            /// The index slots of the binary form, in the order of `idx`.
            #[allow(clippy::extra_unused_lifetimes)]
            pub fn index_slots<'a>(self) -> Vec<IdxSlot> {
                match self {
                    $( MsgId::$name => {
                        #[allow(unused_mut)]
                        let mut slots = Vec::new();
                        $( <$fty as BinIdx>::describe(stringify!($field), &mut slots); )*
                        slots
                    } )*
                }
            }

            /// The value type of the binary form ([`super::bin::ty`]).
            #[allow(clippy::extra_unused_lifetimes)]
            pub fn value_type<'a>(self) -> u8 {
                match self {
                    $( MsgId::$name => {
                        #[allow(unused_mut, unused_assignments)]
                        let mut t = super::bin::ty::NONE;
                        $( t = <$vty as BinValue<'a>>::TYPE; )?
                        t
                    } )*
                }
            }

            /// The segment patterns of the address (the alternatives, for the trie).
            #[allow(unused_variables, unused_mut, clippy::extra_unused_lifetimes)]
            pub fn patterns<'a>(self) -> Vec<Vec<Pat>> {
                match self {
                    $( MsgId::$name => {
                        $( let $field = PhantomData::<$fty>; )*
                        let mut acc: Vec<Vec<Pat>> = vec![vec![]];
                        $( msg_seg!(pat acc $seg); )*
                        acc
                    } )*
                }
            }

            /// Decode a report line of this ID: the path (without the leading `/`) and the value.
            #[allow(unused_mut, unused_variables)]
            pub fn decode<'a>(self, path: &'a str, value: &'a str, log: rules::Log) -> Result<Msg<'a>, Miss> {
                let mut r = PathReader::new(path);
                match self {
                    $( MsgId::$name => {
                        $( msg_seg!(read r $seg); )*
                        if !r.is_empty() {
                            return Err(Miss::Path);
                        }
                        $( let $val = rules::$rule(value, log $($(, $arg)*)?).ok_or(Miss::Value)?; )?
                        Ok(Msg::$name { $($field,)* $($val)? })
                    } )*
                }
            }

            /// Sample messages of this ID for the round-trip test: the k-th sample of every field.
            #[cfg(test)]
            pub(crate) fn sample(self, k: usize) -> Msg<'static> {
                fn pick<T: $crate::protocol::legacy::Sample>(k: usize) -> T {
                    let mut samples = T::samples();
                    let n = samples.len();
                    samples.swap_remove(k % n)
                }
                let _ = k;
                match self {
                    $( MsgId::$name => Msg::$name { $($field: pick(k),)* $($val: pick(k))? }, )*
                }
            }
        }

        /// The metadata of all messages, in the order of the table.
        pub const MESSAGES: &[MsgInfo] = &[$(
            MsgInfo {
                id: MsgId::$name,
                name: stringify!($name),
                template: concat!("/", msg_template!($($seg)*)),
                fields: &[$((stringify!($field), stringify!($fty)),)* $((stringify!($val), stringify!($vty)))?],
                value: msg_val!(info $($val)?),
                rule: msg_val!(info $($rule)?),
            },
        )*];

        impl<'a> Msg<'a> {
            pub fn id(&self) -> MsgId {
                match self {
                    $( Msg::$name { .. } => MsgId::$name, )*
                }
            }

            /// Append the record of the message (the binary form).
            pub fn encode(&self, w: &mut Writer) {
                match self {
                    $( Msg::$name { $($field,)* $($val)? } => w.record($code, |_idx, _val| {
                        $( BinIdx::put($field, _idx); )*
                        $( BinValue::encode($val, _val); )?
                    }), )*
                }
            }

            /// The message of a record.
            pub fn decode_record(record: &Record<'a>) -> Result<Msg<'a>, BinError> {
                let id = MsgId::from_u16(record.id).ok_or(BinError::UnknownId(record.id))?;
                let bad = BinError::Value(record.id);
                #[allow(unused_mut)]
                let mut _idx = IdxIn::new(record.idx);
                match id {
                    $( MsgId::$name => {
                        $( let $field = BinIdx::get(&mut _idx).ok_or(bad)?; )*
                        $( let $val = BinValue::decode(record).ok_or(bad)?; )?
                        msg_none!(record bad $($val)?);
                        Ok(Msg::$name { $($field,)* $($val)? })
                    } )*
                }
            }

            /// The report line `<address> <value>` (the value in the form which the rule of the
            /// entry parses back; doubles in their shortest exact form).
            pub fn to_text(&self) -> String {
                match self {
                    $( Msg::$name { $($field,)* $($val)? } => {
                        #[allow(unused_mut)]
                        let mut w = PathWriter::new();
                        $( msg_seg!(write w $seg); )*
                        let mut line = String::from("/");
                        line.push_str(&w.finish());
                        $(
                            line.push(' ');
                            TextValue::write_text($val, &mut line);
                        )?
                        line
                    } )*
                }
            }
        }
    };
}

/// A segment of an address template: a literal, an optional literal or a field.
macro_rules! msg_seg {
    (pat $acc:ident $l:literal) => { push_lit(&mut $acc, $l) };
    (pat $acc:ident ($l:literal)) => { push_opt_lit(&mut $acc, $l) };
    (pat $acc:ident $f:ident) => { push_field(&mut $acc, &$f) };
    (read $r:ident $l:literal) => {
        if !$r.literal($l) {
            return Err(Miss::Path);
        }
    };
    (read $r:ident ($l:literal)) => { $r.optional($l) };
    (read $r:ident $f:ident) => { let $f = Seg::read(&mut $r).ok_or(Miss::Path)?; };
    (write $w:ident $l:literal) => { $w.seg($l) };
    (write $w:ident ($l:literal)) => { $w.seg($l) };
    (write $w:ident $f:ident) => { Seg::write($f, &mut $w) };
}

/// A message without a value: the record has none.
macro_rules! msg_none {
    ($r:ident $bad:ident) => {
        if !super::bin::no_value($r) {
            return Err($bad);
        }
    };
    ($r:ident $bad:ident $v:ident) => {};
}

macro_rules! msg_val {
    (info) => { None };
    (info $v:ident) => { Some(stringify!($v)) };
}

/// The template text of an address, e.g. `track/{ch}/volume/str/{limit}`.
macro_rules! msg_template {
    (@one $l:literal) => { $l };
    (@one ($l:literal)) => { concat!("[", $l, "]") };
    (@one $f:ident) => { concat!("{", stringify!($f), "}") };
    () => { "" };
    ($s:tt) => { msg_template!(@one $s) };
    ($s:tt $($rest:tt)+) => { concat!(msg_template!(@one $s), "/", msg_template!($($rest)+)) };
}
