// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Rust port of DrivenByMoss4Reaper.
//!
//! The library is loaded by the C++ Reaper extension instead of a Java VM. The extension calls the
//! functions in [`ffi`] (which mirror the static methods of the former `Controller` Java class) and
//! hands over a table of callbacks which replaces the former JNI `native` methods.

pub mod app;
pub mod backend;
pub mod config;
pub mod controller;
pub mod daw;
pub mod ffi;
pub mod framework;
pub mod host;
pub mod midi;
pub mod parser;
pub mod scale;

/// Defines a C-like enum with an ordinal, a list of all values and offset helpers (mirrors the
/// Java enum features used by the framework, e.g. `ButtonID.get (ButtonID.PAD1, i)`).
#[macro_export]
macro_rules! id_enum {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum $name { $($variant),* }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            pub fn ordinal(self) -> usize {
                self as usize
            }

            pub fn from_ordinal(ordinal: usize) -> Option<$name> {
                Self::ALL.get(ordinal).copied()
            }

            /// Get the value which is `offset` positions after `base`.
            pub fn get(base: $name, offset: usize) -> $name {
                Self::from_ordinal(base.ordinal() + offset).expect("ID offset out of range")
            }

            pub fn name(self) -> &'static str {
                match self { $($name::$variant => stringify!($variant)),* }
            }
        }
    };
}
