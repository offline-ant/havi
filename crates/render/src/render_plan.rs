use std::collections::HashMap;

use havi_types::fragment_tree::BoxFragment;
use havi_types::Fragment;
use style::computed_values::transform_style::T as ComputedTransformStyle;

use crate::frame_tree::{FrameId, FrameTree};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NodeRenderSemantics {
    pub has_perspective: bool,
    pub has_true_3d_transform: bool,
    pub preserve_3d: bool,
}

impl NodeRenderSemantics {
    pub(crate) fn requires_compositor(self) -> bool {
        self.has_true_3d_transform || self.preserve_3d
    }

    pub(crate) fn participation(self) -> RenderParticipation {
        if !self.requires_compositor() {
            return RenderParticipation::Direct2d;
        }
        RenderParticipation::Compositor {
            group: if self.preserve_3d {
                CompositorGroupMode::Preserve3d
            } else {
                CompositorGroupMode::Flat
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompositorGroupMode {
    Flat,
    Preserve3d,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenderParticipation {
    Direct2d,
    Compositor { group: CompositorGroupMode },
}

impl Default for RenderParticipation {
    fn default() -> Self {
        Self::Direct2d
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RenderPlan {
    frame_participation: Vec<RenderParticipation>,
    owner_semantics: HashMap<usize, NodeRenderSemantics>,
}

impl RenderPlan {
    pub(crate) fn build<'a>(
        frame_tree: &FrameTree<'a>,
        owner_semantics: HashMap<usize, NodeRenderSemantics>,
    ) -> Self {
        let mut frame_participation = vec![RenderParticipation::Direct2d; frame_tree.frames.len()];
        for (frame_id, frame) in frame_tree.frames.iter().enumerate() {
            let participation = frame
                .owner_node_id
                .and_then(|node_id| owner_semantics.get(&node_id).copied())
                .map(NodeRenderSemantics::participation)
                .unwrap_or(RenderParticipation::Direct2d);
            frame_participation[frame_id] = participation;
        }
        Self {
            frame_participation,
            owner_semantics,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_frame_participation(
        frame_participation: Vec<RenderParticipation>,
    ) -> Self {
        Self {
            frame_participation,
            owner_semantics: HashMap::new(),
        }
    }

    pub(crate) fn frame_participation(&self, frame_id: FrameId) -> RenderParticipation {
        self.frame_participation
            .get(frame_id)
            .copied()
            .unwrap_or(RenderParticipation::Direct2d)
    }

    #[allow(dead_code)]
    pub(crate) fn owner_semantics(&self, node_id: usize) -> Option<NodeRenderSemantics> {
        self.owner_semantics.get(&node_id).copied()
    }
}

pub(crate) fn collect_owner_render_semantics(
    fragments: &[Fragment],
) -> HashMap<usize, NodeRenderSemantics> {
    let mut semantics = HashMap::new();
    for fragment in fragments {
        collect_fragment_render_semantics(fragment, &mut semantics);
    }
    semantics
}

fn collect_fragment_render_semantics(
    fragment: &Fragment,
    semantics: &mut HashMap<usize, NodeRenderSemantics>,
) {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            collect_box_render_semantics(bf, semantics);
            for child in &bf.children {
                collect_fragment_render_semantics(child, semantics);
            }
        }
        Fragment::Positioning(pf) => {
            for child in &pf.children {
                collect_fragment_render_semantics(child, semantics);
            }
        }
        Fragment::IFrame(iframe) => {
            for child in iframe.child_fragments.iter() {
                collect_fragment_render_semantics(child, semantics);
            }
        }
        Fragment::Text(_) | Fragment::Image(_) => {}
    }
}

fn collect_box_render_semantics(
    bf: &BoxFragment,
    semantics: &mut HashMap<usize, NodeRenderSemantics>,
) {
    let Some(node_id) = bf.base.tag.map(|tag| tag.node.0) else {
        return;
    };
    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    let style = &bf.base.style;
    let box_style = style.get_box();
    let preserve_3d = box_style.transform_style == ComputedTransformStyle::Preserve3d;
    let has_perspective = !matches!(box_style.perspective, style::values::generics::box_::Perspective::None);
    let has_transform = !box_style.transform.0.is_empty()
        || box_style.scale != style::values::generics::transform::GenericScale::None
        || box_style.rotate != style::values::generics::transform::GenericRotate::None
        || box_style.translate != style::values::generics::transform::GenericTranslate::None;
    let has_true_3d_transform = has_transform
        && crate::transform::has_true_3d_transform(style, bw, bh);

    semantics.insert(
        node_id,
        NodeRenderSemantics {
            has_perspective,
            has_true_3d_transform,
            preserve_3d,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_units::Au;
    use havi_types::fragment_tree::{BaseFragment, BaseFragmentInfo, Baselines};
    use havi_types::geom::{PhysicalRect, PhysicalSides};
    use havi_types::OpaqueNode;
    use makepad_widgets::dvec2;
    use style::properties::ComputedValues;
    use style::properties::generated::style_structs::Font;
    use style::values::computed::length::NonNegativeLength;
    use style::values::specified::TransformStyle;

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> PhysicalRect<Au> {
        use style_traits::CSSPixel;
        PhysicalRect::new(
            euclid::Point2D::<Au, CSSPixel>::new(Au::from_f32_px(x), Au::from_f32_px(y)),
            euclid::Size2D::<Au, CSSPixel>::new(Au::from_f32_px(w), Au::from_f32_px(h)),
        )
    }

    fn base_box(node_id: usize, style: servo_arc::Arc<ComputedValues>) -> Fragment {
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(node_id)),
                style,
                make_rect(0.0, 0.0, 100.0, 50.0),
            ),
            children: Vec::new(),
            padding: sides,
            border: sides,
            margin: sides,
            baselines: Baselines::default(),
            block_level_info: None,
            background_images: Vec::new(),
        })
    }

    fn initial_style() -> servo_arc::Arc<ComputedValues> {
        ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
    }

    #[test]
    fn preserve_3d_boxes_require_preserve3d_compositor_group() {
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style)
            .mutate_box()
            .set_transform_style(TransformStyle::Preserve3d);
        let fragments = [base_box(11, style.to_arc())];
        let semantics = collect_owner_render_semantics(&fragments);
        let node = semantics.get(&11).copied().unwrap();
        assert_eq!(
            node.participation(),
            RenderParticipation::Compositor {
                group: CompositorGroupMode::Preserve3d,
            }
        );
    }

    #[test]
    fn plain_boxes_stay_on_direct_2d_path() {
        let fragments = [base_box(12, initial_style())];
        let semantics = collect_owner_render_semantics(&fragments);
        let node = semantics.get(&12).copied().unwrap();
        assert_eq!(node.participation(), RenderParticipation::Direct2d);
    }

    #[test]
    fn perspective_only_boxes_stay_on_direct_2d_path() {
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style)
            .mutate_box()
            .set_perspective(style::values::generics::box_::Perspective::Length(NonNegativeLength::new(600.0)));
        let fragments = [base_box(13, style.to_arc())];
        let semantics = collect_owner_render_semantics(&fragments);
        let node = semantics.get(&13).copied().unwrap();
        assert_eq!(node.participation(), RenderParticipation::Direct2d);
    }

    #[test]
    fn perspective_with_2d_translate_stays_on_direct_2d_path() {
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        let box_style = servo_arc::Arc::make_mut(&mut style).mutate_box();
        box_style.set_perspective(style::values::generics::box_::Perspective::Length(NonNegativeLength::new(600.0)));
        box_style.set_translate(style::values::generics::transform::GenericTranslate::Translate(
            style::values::computed::LengthPercentage::new_length(style::values::computed::length::Length::new(10.0)),
            style::values::computed::LengthPercentage::zero_percent(),
            style::values::computed::length::Length::new(0.0),
        ));
        let fragments = [base_box(14, style.to_arc())];
        let semantics = collect_owner_render_semantics(&fragments);
        let node = semantics.get(&14).copied().unwrap();
        assert_eq!(node.participation(), RenderParticipation::Direct2d);
    }

    #[test]
    fn render_plan_defaults_unowned_frames_to_direct_2d() {
        let frame_tree = FrameTree::new();
        let plan = RenderPlan::build(&frame_tree, HashMap::new());
        assert_eq!(plan.frame_participation(frame_tree.root_id()), RenderParticipation::Direct2d);
        let _ = dvec2(0.0, 0.0);
    }
}
