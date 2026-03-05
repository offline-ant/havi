/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! In-memory fixture state for hppr-join deterministic testing.
//!
//! Scope is process-local and resets on restart.

use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinFixtureState {
    None,
    Pending,
    Approved,
}

impl JoinFixtureState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pending => "pending",
            Self::Approved => "approved",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            _ => None,
        }
    }
}

impl Default for JoinFixtureState {
    fn default() -> Self {
        Self::None
    }
}

fn global_join_fixture() -> &'static RwLock<JoinFixtureState> {
    static JOIN_FIXTURE: OnceLock<RwLock<JoinFixtureState>> = OnceLock::new();
    JOIN_FIXTURE.get_or_init(|| RwLock::new(JoinFixtureState::None))
}

pub fn get_join_fixture_state() -> JoinFixtureState {
    global_join_fixture()
        .read()
        .map(|g| *g)
        .unwrap_or(JoinFixtureState::None)
}

pub fn set_join_fixture_state(state: JoinFixtureState) {
    if let Ok(mut g) = global_join_fixture().write() {
        *g = state;
    }
}
