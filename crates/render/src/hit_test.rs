#![cfg_attr(not(test), allow(dead_code))]

//! Temporary semantic-scene hit testing during the phase-1 rewrite.

use havi_fragment_semantics::OpaqueNode;
use makepad_widgets::DVec2;

use crate::scene::RenderScene;

pub(crate) fn hit_test(_scene: &RenderScene<'_>, _point_world: DVec2) -> Option<OpaqueNode> {
    None
}

pub(crate) fn find_scroll_container(
    _scene: &RenderScene<'_>,
    _point_world: DVec2,
) -> Option<OpaqueNode> {
    None
}
