//! Hit testing and scroll container lookup.

use havi_types::{Fragment, OpaqueNode};
use makepad_widgets::*;
use style::values::computed::box_::Overflow;

use crate::paint_order::paint_order;
use crate::ScrollState;

/// Hit test: find the topmost fragment at `point`. Walks in reverse paint
/// order (front to back) and returns the first hit `OpaqueNode`.
pub fn hit_test(
    fragments: &[Fragment],
    origin: DVec2,
    point: DVec2,
) -> Option<OpaqueNode> {
    for fragment in fragments.iter().rev() {
        if let Some(node) = hit_test_fragment(fragment, origin, point) {
            return Some(node);
        }
    }
    None
}

fn hit_test_fragment(
    fragment: &Fragment,
    parent_origin: DVec2,
    point: DVec2,
) -> Option<OpaqueNode> {
    let rect = fragment.content_rect();
    let x = parent_origin.x + rect.origin.x.to_f32_px() as f64;
    let y = parent_origin.y + rect.origin.y.to_f32_px() as f64;

    let (hx, hy, hw, hh) = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            let br = bf.border_rect();
            (
                parent_origin.x + br.origin.x.to_f32_px() as f64,
                parent_origin.y + br.origin.y.to_f32_px() as f64,
                br.size.width.to_f32_px() as f64,
                br.size.height.to_f32_px() as f64,
            )
        }
        _ => (x, y, rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
    };

    // For iframes, recurse into child fragment tree (clipped to iframe rect).
    if let Fragment::IFrame(iframe) = fragment {
        let contains = point.x >= hx && point.x < hx + hw && point.y >= hy && point.y < hy + hh;
        if contains {
            let child_origin = dvec2(x, y);
            for child in iframe.child_fragments.iter().rev() {
                if let Some(node) = hit_test_fragment(child, child_origin, point) {
                    return Some(node);
                }
            }
        }
    }

    if let Some(children) = fragment.children() {
        let child_origin = dvec2(x, y);
        let ordered = paint_order(children);
        for &idx in ordered.iter().rev() {
            if let Some(node) = hit_test_fragment(&children[idx], child_origin, point) {
                return Some(node);
            }
        }
    }

    let contains = point.x >= hx && point.x < hx + hw && point.y >= hy && point.y < hy + hh;
    if contains {
        if let Some(tag) = fragment.tag() {
            return Some(tag.node);
        }
    }
    None
}

/// Find the innermost scroll container (overflow != visible) at `point`.
pub fn find_scroll_container(
    fragments: &[Fragment],
    origin: DVec2,
    point: DVec2,
    scroll_state: &ScrollState,
) -> Option<OpaqueNode> {
    for fragment in fragments.iter().rev() {
        if let Some(node) = find_scroll_container_in(fragment, origin, point, scroll_state) {
            return Some(node);
        }
    }
    None
}

fn find_scroll_container_in(
    fragment: &Fragment,
    parent_origin: DVec2,
    point: DVec2,
    scroll_state: &ScrollState,
) -> Option<OpaqueNode> {
    let rect = fragment.content_rect();
    let x = parent_origin.x + rect.origin.x.to_f32_px() as f64;
    let y = parent_origin.y + rect.origin.y.to_f32_px() as f64;

    let (hx, hy, hw, hh) = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            let br = bf.border_rect();
            (
                parent_origin.x + br.origin.x.to_f32_px() as f64,
                parent_origin.y + br.origin.y.to_f32_px() as f64,
                br.size.width.to_f32_px() as f64,
                br.size.height.to_f32_px() as f64,
            )
        }
        _ => return None,
    };

    if !(point.x >= hx && point.x < hx + hw && point.y >= hy && point.y < hy + hh) {
        return None;
    }

    if let Some(children) = fragment.children() {
        let mut child_origin = dvec2(x, y);
        if let Some(tag) = fragment.tag() {
            if let Some(scroll) = scroll_state.get(&tag.node.0) {
                child_origin.x -= scroll.x;
                child_origin.y -= scroll.y;
            }
        }
        for child in children.iter().rev() {
            if let Some(node) = find_scroll_container_in(child, child_origin, point, scroll_state) {
                return Some(node);
            }
        }
    }

    let is_scroll_container = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            let ov = bf.base.style.get_box();
            !matches!(ov.overflow_x, Overflow::Visible) || !matches!(ov.overflow_y, Overflow::Visible)
        }
        _ => false,
    };

    if is_scroll_container { fragment.tag().map(|t| t.node) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use havi_types::fragment_tree::{BaseFragment, BaseFragmentInfo, BoxFragment};
    use havi_types::geom::{PhysicalRect, PhysicalSides};
    use style::properties::ComputedValues;
    use style::properties::generated::style_structs::Font;

    fn initial_style() -> servo_arc::Arc<ComputedValues> {
        ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
    }

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> PhysicalRect<app_units::Au> {
        use app_units::Au;
        use style_traits::CSSPixel;
        PhysicalRect::new(
            euclid::Point2D::<Au, CSSPixel>::new(Au::from_f32_px(x), Au::from_f32_px(y)),
            euclid::Size2D::<Au, CSSPixel>::new(Au::from_f32_px(w), Au::from_f32_px(h)),
        )
    }

    fn make_tagged_box(node_id: usize, x: f32, y: f32, w: f32, h: f32, style: servo_arc::Arc<ComputedValues>) -> Fragment {
        use app_units::Au;
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(BaseFragmentInfo::new(OpaqueNode(node_id)), style, make_rect(x, y, w, h)),
            children: Vec::new(),
            padding: sides, border: sides, margin: sides,
            baselines: havi_types::fragment_tree::Baselines::default(),
            block_level_info: None,
        })
    }

    fn make_tagged_box_with_children(node_id: usize, x: f32, y: f32, w: f32, h: f32, style: servo_arc::Arc<ComputedValues>, children: Vec<Fragment>) -> Fragment {
        use app_units::Au;
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(BaseFragmentInfo::new(OpaqueNode(node_id)), style, make_rect(x, y, w, h)),
            children,
            padding: sides, border: sides, margin: sides,
            baselines: havi_types::fragment_tree::Baselines::default(),
            block_level_info: None,
        })
    }

    fn positioned_style(z: i32) -> servo_arc::Arc<ComputedValues> {
        use style::values::specified::box_::PositionProperty;
        use style::values::generics::position::ZIndex;
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style).mutate_box().set_position(PositionProperty::Relative);
        servo_arc::Arc::make_mut(&mut style).mutate_position().set_z_index(ZIndex::Integer(z));
        style.to_arc()
    }

    fn overflow_hidden_style() -> servo_arc::Arc<ComputedValues> {
        use style::values::specified::Overflow;
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style).mutate_box().set_overflow_x(Overflow::Hidden);
        servo_arc::Arc::make_mut(&mut style).mutate_box().set_overflow_y(Overflow::Hidden);
        style.to_arc()
    }

    fn overflow_auto_style() -> servo_arc::Arc<ComputedValues> {
        use style::values::specified::Overflow;
        let mut style = ComputedValues::initial_values_with_font_override(Font::initial_values());
        servo_arc::Arc::make_mut(&mut style).mutate_box().set_overflow_x(Overflow::Auto);
        servo_arc::Arc::make_mut(&mut style).mutate_box().set_overflow_y(Overflow::Auto);
        style.to_arc()
    }

    #[test]
    fn basic_point_in_box() {
        let fragments = vec![make_tagged_box(1, 10.0, 10.0, 50.0, 50.0, initial_style())];
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(25.0, 25.0)), Some(OpaqueNode(1)));
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(5.0, 5.0)), None);
    }

    #[test]
    fn overlapping_later_wins() {
        let fragments = vec![
            make_tagged_box(1, 0.0, 0.0, 50.0, 50.0, initial_style()),
            make_tagged_box(2, 25.0, 25.0, 50.0, 50.0, initial_style()),
        ];
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(30.0, 30.0)), Some(OpaqueNode(2)));
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(10.0, 10.0)), Some(OpaqueNode(1)));
    }

    #[test]
    fn z_index_higher_wins() {
        let children = vec![
            make_tagged_box(10, 0.0, 0.0, 80.0, 80.0, positioned_style(2)),
            make_tagged_box(20, 0.0, 0.0, 80.0, 80.0, positioned_style(1)),
        ];
        let fragments = vec![
            make_tagged_box_with_children(1, 0.0, 0.0, 100.0, 100.0, initial_style(), children),
        ];
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(10.0, 10.0)), Some(OpaqueNode(10)));
    }

    #[test]
    fn child_over_parent() {
        let children = vec![make_tagged_box(2, 10.0, 10.0, 30.0, 30.0, initial_style())];
        let fragments = vec![
            make_tagged_box_with_children(1, 0.0, 0.0, 100.0, 100.0, initial_style(), children),
        ];
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(20.0, 20.0)), Some(OpaqueNode(2)));
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(80.0, 80.0)), Some(OpaqueNode(1)));
    }

    #[test]
    fn negative_z_behind_parent() {
        let children = vec![make_tagged_box(2, 0.0, 0.0, 50.0, 50.0, positioned_style(-1))];
        let fragments = vec![
            make_tagged_box_with_children(1, 0.0, 0.0, 100.0, 100.0, initial_style(), children),
        ];
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(25.0, 25.0)), Some(OpaqueNode(2)));
    }

    #[test]
    fn scroll_container_basic() {
        let fragments = vec![
            make_tagged_box_with_children(1, 0.0, 0.0, 200.0, 200.0, overflow_auto_style(),
                vec![make_tagged_box(2, 0.0, 0.0, 50.0, 50.0, initial_style())]),
        ];
        assert_eq!(find_scroll_container(&fragments, dvec2(0.0, 0.0), dvec2(10.0, 10.0), &ScrollState::new()), Some(OpaqueNode(1)));
    }

    #[test]
    fn scroll_container_none_for_visible() {
        let fragments = vec![make_tagged_box(1, 0.0, 0.0, 100.0, 100.0, initial_style())];
        assert_eq!(find_scroll_container(&fragments, dvec2(0.0, 0.0), dvec2(10.0, 10.0), &ScrollState::new()), None);
    }

    #[test]
    fn scroll_container_nested_inner_wins() {
        let inner = make_tagged_box_with_children(2, 10.0, 10.0, 80.0, 80.0, overflow_hidden_style(), vec![]);
        let outer = make_tagged_box_with_children(1, 0.0, 0.0, 200.0, 200.0, overflow_auto_style(), vec![inner]);
        let fragments = vec![outer];
        let ss = ScrollState::new();
        assert_eq!(find_scroll_container(&fragments, dvec2(0.0, 0.0), dvec2(20.0, 20.0), &ss), Some(OpaqueNode(2)));
        assert_eq!(find_scroll_container(&fragments, dvec2(0.0, 0.0), dvec2(5.0, 5.0), &ss), Some(OpaqueNode(1)));
    }

    #[test]
    fn scroll_container_outside_returns_none() {
        let fragments = vec![make_tagged_box(1, 10.0, 10.0, 50.0, 50.0, overflow_auto_style())];
        assert_eq!(find_scroll_container(&fragments, dvec2(0.0, 0.0), dvec2(5.0, 5.0), &ScrollState::new()), None);
    }

    fn make_iframe(node_id: usize, x: f32, y: f32, w: f32, h: f32, child_fragments: Vec<Fragment>) -> Fragment {
        use std::sync::Arc;
        Fragment::IFrame(havi_types::IFrameFragment {
            base: BaseFragment::new(BaseFragmentInfo::new(OpaqueNode(node_id)), initial_style(), make_rect(x, y, w, h)),
            child_fragments: Arc::new(child_fragments),
            child_content_height: 0.0,
        })
    }

    #[test]
    fn iframe_hit_test_reaches_child() {
        // iframe at (10,10) 100x100, containing a box at (5,5) 30x30
        let child = make_tagged_box(42, 5.0, 5.0, 30.0, 30.0, initial_style());
        let iframe = make_iframe(1, 10.0, 10.0, 100.0, 100.0, vec![child]);
        let fragments = vec![iframe];
        // Point inside the child box (10+5+10, 10+5+10) = (25, 25)
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(25.0, 25.0)), Some(OpaqueNode(42)));
    }

    #[test]
    fn iframe_hit_test_falls_through_to_iframe_node() {
        // Point inside iframe but outside child — hits the iframe itself
        let child = make_tagged_box(42, 5.0, 5.0, 30.0, 30.0, initial_style());
        let iframe = make_iframe(1, 10.0, 10.0, 100.0, 100.0, vec![child]);
        let fragments = vec![iframe];
        // Point (90, 90) is inside iframe rect but outside child
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(90.0, 90.0)), Some(OpaqueNode(1)));
    }

    #[test]
    fn iframe_hit_test_outside_returns_none() {
        let child = make_tagged_box(42, 5.0, 5.0, 30.0, 30.0, initial_style());
        let iframe = make_iframe(1, 10.0, 10.0, 100.0, 100.0, vec![child]);
        let fragments = vec![iframe];
        // Point outside iframe entirely
        assert_eq!(hit_test(&fragments, dvec2(0.0, 0.0), dvec2(5.0, 5.0)), None);
    }
}
