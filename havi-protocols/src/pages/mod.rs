/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Protocol page handlers.
//!
//! Each module exports an async `handle_request()` function that returns
//! a `PageResponse`. The shell embedder calls these and converts the
//! result to its rendering layer's response type.

pub mod havi;
pub mod hppr;
pub mod hppr_browse;
pub mod hppr_editor;
pub mod hppr_sandbox;
pub mod hppr_setup;
