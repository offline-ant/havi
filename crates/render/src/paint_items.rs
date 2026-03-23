use havi_fragment_semantics::Fragment;
use makepad_widgets::DVec2;

use crate::layout_stacking_context::StackingContextSection;

pub(crate) type PaintSource<'a> = &'a Fragment;

#[derive(Clone, Copy)]
pub(crate) struct RenderPaintItem<'a> {
    pub section: StackingContextSection,
    pub local_origin: DVec2,
    pub source: PaintSource<'a>,
}
