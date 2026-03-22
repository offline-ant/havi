use std::collections::HashMap;

use havi_fragment_semantics::fragment_tree::BoxFragment;
use havi_fragment_semantics::Fragment;
use makepad_compositor::{MpBackfaceVisibility, MpTransformStyle};
use style::computed_values::backface_visibility::T as ComputedBackfaceVisibility;
use style::computed_values::mix_blend_mode::T as ComputedMixBlendMode;
use style::computed_values::overflow_x::T as ComputedOverflow;
use style::computed_values::transform_style::T as ComputedTransformStyle;
use style::properties::ComputedValues;
use style::values::computed::basic_shape::ClipPath;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NodeRenderSemantics {
    pub has_transform: bool,
    pub has_perspective: bool,
    pub has_true_3d_transform: bool,
    pub transform_style: MpTransformStyle,
    pub flattens_descendants: bool,
    pub backface_visibility: MpBackfaceVisibility,
    pub opacity: f32,
    pub needs_filter: bool,
    pub needs_blend: bool,
    pub needs_mask: bool,
    pub needs_isolation: bool,
}

impl Default for NodeRenderSemantics {
    fn default() -> Self {
        Self {
            has_transform: false,
            has_perspective: false,
            has_true_3d_transform: false,
            transform_style: MpTransformStyle::Flat,
            flattens_descendants: true,
            backface_visibility: MpBackfaceVisibility::Visible,
            opacity: 1.0,
            needs_filter: false,
            needs_blend: false,
            needs_mask: false,
            needs_isolation: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RenderPlan {
    owner_semantics: HashMap<usize, NodeRenderSemantics>,
}

impl RenderPlan {
    pub(crate) fn build(owner_semantics: HashMap<usize, NodeRenderSemantics>) -> Self {
        Self { owner_semantics }
    }

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

pub(crate) fn compute_used_transform_style(style: &ComputedValues) -> MpTransformStyle {
    if style.get_box().transform_style != ComputedTransformStyle::Preserve3d {
        return MpTransformStyle::Flat;
    }
    if grouping_properties_flatten(style) {
        return MpTransformStyle::Flat;
    }
    MpTransformStyle::Preserve3D
}

pub(crate) fn grouping_properties_flatten(style: &ComputedValues) -> bool {
    let effects = style.get_effects();
    let overflow = style.get_box();
    effects.opacity != 1.0
        || !effects.filter.0.is_empty()
        || effects.mix_blend_mode != ComputedMixBlendMode::Normal
        || style.get_svg().clip_path != ClipPath::None
        || !matches!(overflow.overflow_x, ComputedOverflow::Visible)
        || !matches!(overflow.overflow_y, ComputedOverflow::Visible)
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
        Fragment::AbsoluteOrFixedPositioned { .. } => {}
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
    let pseudo_key = match bf.base.style.pseudo() {
        Some(style::selector_parser::PseudoElement::Before) => 1,
        Some(style::selector_parser::PseudoElement::After) => 2,
        Some(style::selector_parser::PseudoElement::Marker) => 3,
        Some(style::selector_parser::PseudoElement::ServoAnonymousBox) => 4,
        Some(style::selector_parser::PseudoElement::ServoAnonymousTable) => 5,
        Some(style::selector_parser::PseudoElement::ServoAnonymousTableCell) => 6,
        Some(style::selector_parser::PseudoElement::ServoAnonymousTableRow) => 7,
        Some(_) => 15,
        None => 0,
    };
    let node_id = (node_id << 8) ^ pseudo_key;
    let border_rect = bf.border_rect();
    let bw = border_rect.size.width.to_f32_px();
    let bh = border_rect.size.height.to_f32_px();
    let style = &bf.base.style;
    let box_style = style.get_box();
    let has_perspective = !matches!(
        box_style.perspective,
        style::values::generics::box_::Perspective::None
    );
    let has_transform = !box_style.transform.0.is_empty()
        || box_style.scale != style::values::generics::transform::GenericScale::None
        || box_style.rotate != style::values::generics::transform::GenericRotate::None
        || box_style.translate != style::values::generics::transform::GenericTranslate::None;
    let has_true_3d_transform = has_transform
        && crate::transform::has_true_3d_transform(style, bw, bh);
    let transform_style = compute_used_transform_style(style);
    let needs_filter = !style.get_effects().filter.0.is_empty();
    let needs_blend = style.get_effects().mix_blend_mode != ComputedMixBlendMode::Normal;
    let needs_mask = style.get_svg().clip_path != ClipPath::None;
    let opacity = style.get_effects().opacity;

    semantics.insert(
        node_id,
        NodeRenderSemantics {
            has_transform,
            has_perspective,
            has_true_3d_transform,
            transform_style,
            flattens_descendants: !matches!(transform_style, MpTransformStyle::Preserve3D),
            backface_visibility: match box_style.backface_visibility {
                ComputedBackfaceVisibility::Hidden => MpBackfaceVisibility::Hidden,
                ComputedBackfaceVisibility::Visible => MpBackfaceVisibility::Visible,
            },
            opacity,
            needs_filter,
            needs_blend,
            needs_mask,
            needs_isolation: opacity != 1.0 || needs_filter || needs_blend || needs_mask,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use havi_fragment_semantics::fragment_tree::{BaseFragment, BaseFragmentInfo, Baselines};
    use havi_fragment_semantics::OpaqueNode;
    use havi_types::geom::{PhysicalRect, PhysicalSides};
    use style::properties::ComputedValues;
    use style::properties::generated::style_structs::Font;
    use style::values::specified::TransformStyle;

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> PhysicalRect<app_units::Au> {
        use style_traits::CSSPixel;
        PhysicalRect::new(
            euclid::Point2D::<app_units::Au, CSSPixel>::new(
                app_units::Au::from_f32_px(x),
                app_units::Au::from_f32_px(y),
            ),
            euclid::Size2D::<app_units::Au, CSSPixel>::new(
                app_units::Au::from_f32_px(w),
                app_units::Au::from_f32_px(h),
            ),
        )
    }

    fn base_box(node_id: usize, style: servo_arc::Arc<ComputedValues>) -> Fragment {
        let sides = PhysicalSides::new(app_units::Au(0), app_units::Au(0), app_units::Au(0), app_units::Au(0));
        Fragment::Box(havi_fragment_semantics::fragment_tree::BoxFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(node_id)),
                style,
                make_rect(0.0, 0.0, 100.0, 50.0),
            ),
            children: Vec::new(),
            cumulative_containing_block_rect: PhysicalRect::zero(),
            scrollable_overflow: None,
            resolved_sticky_insets: None,
            padding: sides,
            border: sides,
            margin: sides,
            baselines: Baselines::default(),
            block_level_info: None,
            background_images: Vec::new(),
        })
    }

    #[test]
    fn preserve_3d_survives_without_grouping_properties() {
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style)
            .mutate_box()
            .set_transform_style(TransformStyle::Preserve3d);
        let fragments = [base_box(11, style.to_arc())];
        let semantics = collect_owner_render_semantics(&fragments);
        let node = semantics.get(&(11 << 8)).copied().unwrap();
        assert_eq!(node.transform_style, MpTransformStyle::Preserve3D);
        assert!(!node.flattens_descendants);
    }

    #[test]
    fn opacity_flattens_used_transform_style() {
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        let style_mut = servo_arc::Arc::make_mut(&mut style);
        style_mut.mutate_box().set_transform_style(TransformStyle::Preserve3d);
        style_mut.mutate_effects().set_opacity(0.5);
        let fragments = [base_box(12, style.to_arc())];
        let semantics = collect_owner_render_semantics(&fragments);
        let node = semantics.get(&(12 << 8)).copied().unwrap();
        assert_eq!(node.transform_style, MpTransformStyle::Flat);
        assert!(node.flattens_descendants);
        assert!(node.needs_isolation);
    }
}
