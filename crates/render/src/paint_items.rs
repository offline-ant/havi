use havi_types::fragment_tree::FragmentId;
use makepad_widgets::DVec2;

use crate::layout_stacking_context::StackingContextSection;

#[derive(Clone, Copy)]
pub(crate) struct RenderPaintItem {
    pub section: StackingContextSection,
    pub local_origin: DVec2,
    pub fragment_id: FragmentId,
}
