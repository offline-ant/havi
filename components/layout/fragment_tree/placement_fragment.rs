/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use app_units::Au;
use malloc_size_of_derive::MallocSizeOf;
use style::computed_values::position::T as Position;
use style::logical_geometry::WritingMode;
use style::values::specified::align::AlignFlags;

use crate::geom::{LogicalVec2, PhysicalRect};

#[derive(Clone, Debug, MallocSizeOf)]
pub struct OutOfFlowPlacementFragment {
    pub id: u32,
    pub static_position_rect: PhysicalRect<Au>,
    pub resolved_alignment: LogicalVec2<AlignFlags>,
    pub original_parent_writing_mode: WritingMode,
    #[ignore_malloc_size_of = "stylo position enum"]
    pub position: Position,
}
