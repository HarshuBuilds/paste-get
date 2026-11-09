// SPDX-License-Identifier: GPL-3.0-or-later

//! paste-get desktop daemon. `main.rs` is a thin command-line front end over this library,
//! which also lets integration tests run whole daemons in one process.

pub mod admission;
pub mod autostart;
pub mod clipboard;
pub mod config;
pub mod control;
pub mod daemon;
pub mod discovery;
pub mod hotkeys;
pub mod i18n;
pub mod notify;
pub mod recent;
