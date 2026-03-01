//! CSS 2.1 Appendix E.2 painting order.

use havi_types::Fragment;
use style::values::specified::box_::PositionProperty;

/// Return child indices in CSS 2.1 Appendix E.2 painting order.
///
/// Steps:
/// 1-2. Parent backgrounds/borders (handled by caller before children).
/// 3.   Positioned descendants with negative z-index.
/// 4.   Block-level non-positioned, non-float descendants.
/// 5.   Float descendants.
/// 6-7. Inline content (text, positioning wrappers).
/// 8-9. Positioned descendants with z-index >= 0 (sorted by z-index, then tree order).
pub fn paint_order(children: &[Fragment]) -> Vec<usize> {
    let mut negative_z: Vec<(i32, usize)> = Vec::new();
    let mut block_bg: Vec<usize> = Vec::new();
    let mut floats: Vec<usize> = Vec::new();
    let mut inline_content: Vec<usize> = Vec::new();
    let mut non_negative_z: Vec<(i32, usize)> = Vec::new();

    for (i, child) in children.iter().enumerate() {
        match child {
            Fragment::Float(_) => floats.push(i),
            Fragment::Text(_) | Fragment::Image(_) | Fragment::IFrame(_) => inline_content.push(i),
            Fragment::Box(bf) => {
                let pos = bf.base.style.get_box().position;
                let is_positioned = matches!(
                    pos,
                    PositionProperty::Relative
                        | PositionProperty::Absolute
                        | PositionProperty::Fixed
                        | PositionProperty::Sticky
                );
                if is_positioned {
                    let z = bf.base.style.get_position().z_index.integer_or(0);
                    if z < 0 {
                        negative_z.push((z, i));
                    } else {
                        non_negative_z.push((z, i));
                    }
                } else {
                    block_bg.push(i);
                }
            }
            Fragment::Positioning(_) => inline_content.push(i),
        }
    }

    negative_z.sort_by_key(|(z, i)| (*z, *i));
    non_negative_z.sort_by_key(|(z, i)| (*z, *i));

    let mut result = Vec::with_capacity(children.len());
    for (_, i) in &negative_z { result.push(*i); }
    result.extend_from_slice(&block_bg);
    result.extend_from_slice(&floats);
    result.extend_from_slice(&inline_content);
    for (_, i) in &non_negative_z { result.push(*i); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use havi_types::fragment_tree::{BaseFragment, BaseFragmentInfo, BoxFragment};
    use havi_types::geom::{PhysicalRect, PhysicalSides};
    use havi_types::TextFragment;
    use style::properties::ComputedValues;
    use style::properties::generated::style_structs::Font;

    fn initial_style() -> servo_arc::Arc<ComputedValues> {
        ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
    }

    fn rect_zero() -> PhysicalRect<app_units::Au> {
        use app_units::Au;
        use style_traits::CSSPixel;
        PhysicalRect::new(
            euclid::Point2D::<Au, CSSPixel>::new(Au(0), Au(0)),
            euclid::Size2D::<Au, CSSPixel>::new(Au::from_f32_px(100.0), Au::from_f32_px(100.0)),
        )
    }

    fn make_box(style: servo_arc::Arc<ComputedValues>) -> Fragment {
        use app_units::Au;
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(BaseFragmentInfo::anonymous(), style, rect_zero()),
            children: Vec::new(),
            padding: sides, border: sides, margin: sides,
            baselines: havi_types::fragment_tree::Baselines::default(),
            block_level_info: None,
        })
    }

    fn make_float(style: servo_arc::Arc<ComputedValues>) -> Fragment {
        use app_units::Au;
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Float(BoxFragment {
            base: BaseFragment::new(BaseFragmentInfo::anonymous(), style, rect_zero()),
            children: Vec::new(),
            padding: sides, border: sides, margin: sides,
            baselines: havi_types::fragment_tree::Baselines::default(),
            block_level_info: None,
        })
    }

    fn make_text() -> Fragment {
        use app_units::Au;
        Fragment::Text(TextFragment {
            base: BaseFragment::new(BaseFragmentInfo::anonymous(), initial_style(), rect_zero()),
            text: "hello".to_string(),
            font_size_px: 16.0,
            glyphs: Vec::new(),
            font_handle: None,
            baseline_ascent: Au(0),
            underline_offset: Au(0), underline_size: Au(0),
            strikeout_offset: Au(0), strikeout_size: Au(0),
        })
    }

    fn positioned_style(z: i32) -> servo_arc::Arc<ComputedValues> {
        use style::values::generics::position::ZIndex;
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style)
            .mutate_box()
            .set_position(PositionProperty::Relative);
        servo_arc::Arc::make_mut(&mut style)
            .mutate_position()
            .set_z_index(ZIndex::Integer(z));
        style.to_arc()
    }

    #[test]
    fn negative_z_before_blocks() {
        let children = vec![make_box(initial_style()), make_box(positioned_style(-1))];
        assert_eq!(paint_order(&children), vec![1, 0]);
    }

    #[test]
    fn floats_between_blocks_and_inline() {
        let children = vec![make_text(), make_float(initial_style()), make_box(initial_style())];
        assert_eq!(paint_order(&children), vec![2, 1, 0]);
    }

    #[test]
    fn positive_z_sorted() {
        let children = vec![
            make_box(positioned_style(2)), make_box(initial_style()), make_box(positioned_style(1)),
        ];
        assert_eq!(paint_order(&children), vec![1, 2, 0]);
    }

    #[test]
    fn full_stacking() {
        let children = vec![
            make_box(positioned_style(-5)), make_box(initial_style()),
            make_float(initial_style()), make_text(),
            make_box(positioned_style(0)), make_box(positioned_style(3)),
        ];
        assert_eq!(paint_order(&children), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn equal_z_preserves_tree_order() {
        let children = vec![make_box(positioned_style(1)), make_box(positioned_style(1))];
        assert_eq!(paint_order(&children), vec![0, 1]);
    }

    #[test]
    fn iframe_paints_as_inline_content() {
        use std::sync::Arc;
        use havi_types::IFrameFragment;
        let iframe = Fragment::IFrame(IFrameFragment {
            base: BaseFragment::new(BaseFragmentInfo::anonymous(), initial_style(), rect_zero()),
            child_fragments: Arc::new(Vec::new()),
            child_content_height: 0.0,
        });
        // block box, then iframe — iframe is inline content (step 6-7), block is step 4
        let children = vec![make_box(initial_style()), iframe];
        assert_eq!(paint_order(&children), vec![0, 1]);
    }
}
