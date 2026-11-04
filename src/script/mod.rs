// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Controllers written in a script language (doc/LUA_PLAN.md, doc/LUA_API.md).
//!
//! The parts in this module do not depend on the script engine:
//! - [`ctx_stack`]: the access of the scripts to the context. A script never holds a reference;
//!   every call resolves its target through the context of the call which runs.
//! - [`names`]: the names of the IDs (buttons, controls, modes, settings, …) as the scripts see
//!   them.
//! - [`builtin`]: the built-in modes, parameter providers and commands, created by name.
//! - [`errors`]: the error policy (a function which fails three times in a row is disabled).
//! - The discovery of the scripts: built into the library and
//!   `<ini folder>/DrivenByMoss/controllers/*.lua`.
//!
//! The engine is in [`lua`] (Lua 5.4 through mlua). No mlua type is visible outside of this
//! module.

pub mod builtin;
pub mod ctx_stack;
pub mod errors;
pub mod lua;
pub mod names;

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::controller::ControllerFactory;
use crate::framework::ctx::Ctx;

/// The scripts which are built into the library: the file name and the source.
pub const BUILTIN_SCRIPTS: &[(&str, &str)] = &[("launch_control_xl.lua", include_str!("../../controllers/launch_control_xl.lua"))];

/// The sub folder of the configuration folder with the user's controller scripts.
pub const USER_FOLDER: &str = "DrivenByMoss/controllers";

/// A controller script: built in or a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptSource {
    /// The file name, used in the error messages (e.g. `launch_control_xl.lua`).
    pub name: String,
    pub text: String,
    /// The folder of the script (for `require`), `None` for a built-in script.
    pub folder: Option<PathBuf>,
}

thread_local! {
    /// The configuration folder of the application on this thread (see [`set_config_folder`]).
    static CONFIG_FOLDER: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Set the configuration folder (the Reaper INI folder or the folder of the controller
/// configuration of a single-controller host). The scripts in its `DrivenByMoss/controllers`
/// sub folder are found by [`factories`] on this thread from now on. The library has no global
/// state, therefore the folder is kept per thread (the thread which creates the application).
pub fn set_config_folder(folder: &Path) {
    CONFIG_FOLDER.with(|f| *f.borrow_mut() = Some(folder.to_path_buf()));
}

/// The folder of the user's controller scripts, if a configuration folder is set.
pub fn user_folder() -> Option<PathBuf> {
    CONFIG_FOLDER.with(|f| f.borrow().as_ref().map(|folder| folder.join(USER_FOLDER)))
}

/// All controller scripts: the built-in ones, then the files of the user folder (sorted by
/// name). A user script with the same UUID as a built-in one is listed too; the first one wins
/// in `find_factory`.
pub fn sources() -> Vec<ScriptSource> {
    let mut sources: Vec<ScriptSource> =
        BUILTIN_SCRIPTS.iter().map(|(name, text)| ScriptSource { name: name.to_string(), text: text.to_string(), folder: None }).collect();
    if let Some(folder) = user_folder() {
        sources.extend(folder_sources(&folder));
    }
    sources
}

/// The `*.lua` files of a folder, sorted by name.
pub fn folder_sources(folder: &Path) -> Vec<ScriptSource> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "lua")).collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let name = path.file_name()?.to_string_lossy().to_string();
            Some(ScriptSource { name, text, folder: Some(folder.to_path_buf()) })
        })
        .collect()
}

/// The factories of all controller scripts which could be loaded. Scripts with errors are
/// skipped (and reported on standard error, there is no host yet).
pub fn factories() -> Vec<Box<dyn ControllerFactory>> {
    sources().into_iter().filter_map(lua::factory).collect()
}

/// Evaluate an expression in the Lua state of the controller instance of the context, for tests
/// and debugging: `code` is the body of a function which gets the table which the script
/// returned as `controller` (e.g. `return controller.state.template_id`). The result is
/// converted to a string, `None` if the context is not a script controller or on an error.
pub fn eval(ctx: &mut Ctx, code: &str) -> Option<String> {
    lua::eval(ctx, code)
}

/// The reference of the script API as Markdown (doc/LUA_API.md), generated from the bindings.
pub fn api_reference() -> String {
    lua::api_reference()
}
