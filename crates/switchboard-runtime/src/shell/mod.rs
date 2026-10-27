//! The terminal hook (SB-183): plain `claude` and `codex` started in an organization's group
//! folders run in that group's shell homes with `SWITCHBOARD_CONTEXT=org:<id>`. Iteration 0.7.0,
//! dossier A part A. `map` is the folder map the hook reads on every change of folder.
pub(crate) mod homes;
pub mod install;
pub mod map;
pub mod scripts;
pub mod windows;
