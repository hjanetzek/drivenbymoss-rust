// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The access of a script to the context of the call which runs.
//!
//! A script never holds a reference to the context, the model or the surface. When Rust calls
//! the script (a mode method, a command, a light), the context of that call is pushed on the
//! [`CtxStack`] of the script's instance and popped when the call returns. Every function of
//! the script API resolves its target through the top of the stack when it runs:
//!
//! - The `ctx` value which the script gets is valid only during its call; its serial must be on
//!   the stack, otherwise the access is an error (not undefined behaviour). This is what
//!   `mlua::Lua::scope` would give for one value; the stack also serves the handles (a track, a
//!   clip, …), which are an index and resolved again on each access.
//! - Calls which get a shared context (`&Ctx`: lights, button colors, knob values) are read-only:
//!   a function which changes something raises an error there, as Rust does not allow it.
//! - A script call which calls into Rust which calls the script again (e.g. `ctx:set_active_mode`
//!   which activates a mode written in the script) pushes a new frame; the pointer of the inner
//!   frame is derived from the `&mut Ctx` which the outer function has borrowed, and the outer
//!   frame is not used until the inner call returned.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use crate::framework::ctx::Ctx;

/// An error of a script API function, raised as an error in the script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError(pub String);

impl ScriptError {
    pub fn new(message: impl Into<String>) -> Self {
        ScriptError(message.into())
    }
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ScriptError {}

pub type ScriptResult<T> = Result<T, ScriptError>;

#[derive(Clone, Copy)]
struct Frame {
    ctx: NonNull<Ctx>,
    mutable: bool,
    serial: u64,
}

/// The contexts of the calls into the script of one controller instance which are running.
#[derive(Default)]
pub struct CtxStack {
    frames: RefCell<Vec<Frame>>,
    next_serial: Cell<u64>,
}

/// Pops the frame when the call ends (also on an error).
struct PopGuard<'a>(&'a CtxStack);

impl Drop for PopGuard<'_> {
    fn drop(&mut self) {
        self.0.frames.borrow_mut().pop();
    }
}

impl CtxStack {
    fn push(&self, ctx: NonNull<Ctx>, mutable: bool) -> u64 {
        let serial = self.next_serial.get() + 1;
        self.next_serial.set(serial);
        self.frames.borrow_mut().push(Frame { ctx, mutable, serial });
        serial
    }

    /// Run `f` (the call into the script) with the context on the stack; `f` gets the serial of
    /// the frame (for the `ctx` value of the call).
    pub fn enter_mut<R>(&self, ctx: &mut Ctx, f: impl FnOnce(u64) -> R) -> R {
        let serial = self.push(NonNull::from(ctx), true);
        let _guard = PopGuard(self);
        f(serial)
    }

    /// As [`Self::enter_mut`] for a call with a shared context (read-only).
    pub fn enter<R>(&self, ctx: &Ctx, f: impl FnOnce(u64) -> R) -> R {
        let serial = self.push(NonNull::from(ctx), false);
        let _guard = PopGuard(self);
        f(serial)
    }

    /// The number of running calls.
    pub fn depth(&self) -> usize {
        self.frames.borrow().len()
    }

    /// True if the frame with the serial is running (the `ctx` of that call is valid).
    pub fn is_valid(&self, serial: u64) -> bool {
        self.frames.borrow().iter().any(|f| f.serial == serial)
    }

    /// True if the innermost call may change the context.
    pub fn is_mutable(&self) -> bool {
        self.frames.borrow().last().is_some_and(|f| f.mutable)
    }

    fn top(&self) -> ScriptResult<Frame> {
        self.frames.borrow().last().copied().ok_or_else(|| ScriptError::new("no controller call is running (a handle can only be used during a call)"))
    }

    /// Read the context of the innermost call.
    pub fn with<R>(&self, f: impl FnOnce(&Ctx) -> R) -> ScriptResult<R> {
        let frame = self.top()?;
        // SAFETY: the frame was pushed from a reference which is borrowed by `enter`/`enter_mut`
        // for as long as the frame is on the stack, and the innermost frame is the only one
        // used. The frame list is not borrowed while `f` runs, so `f` may enter again.
        let ctx = unsafe { frame.ctx.as_ref() };
        Ok(f(ctx))
    }

    /// Change the context of the innermost call; an error if that call is read-only.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut Ctx) -> R) -> ScriptResult<R> {
        let mut frame = self.top()?;
        if !frame.mutable {
            return Err(ScriptError::new("the context is read-only in this call (a light, a color or a value)"));
        }
        // SAFETY: as in `with`; a mutable frame was pushed from a `&mut Ctx` which is not used
        // by its owner while the frame is on the stack. A nested frame is derived from the
        // reference which `f` gets here, and this reference is not used while the nested call
        // runs.
        let ctx = unsafe { frame.ctx.as_mut() };
        Ok(f(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frame_is_an_error() {
        let stack = CtxStack::default();
        assert!(stack.with(|_| ()).is_err());
        assert!(stack.with_mut(|_| ()).is_err());
        assert_eq!(stack.depth(), 0);
        assert!(!stack.is_valid(1));
    }
}
