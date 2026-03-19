#![cfg_attr(not(test), allow(dead_code))]

//! Hit testing and scroll container lookup using the frame and clip scene model.

use havi_types::{Fragment, OpaqueNode};
use makepad_widgets::*;

use crate::frame_tree::FramePaintCommand;

pub(crate) fn hit_test(
    frame_tree: &crate::frame_tree::FrameTree<'_>,
    clip_tree: &crate::clip_tree::ClipTree,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    hit_test_frame_reverse(frame_tree, clip_tree, frame_tree.root, point_world)
}

pub(crate) fn find_scroll_container(
    frame_tree: &crate::frame_tree::FrameTree<'_>,
    clip_tree: &crate::clip_tree::ClipTree,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    find_scroll_container_in_frame_reverse(frame_tree, clip_tree, frame_tree.root, point_world)
}

fn hit_test_frame_reverse(
    frame_tree: &crate::frame_tree::FrameTree<'_>,
    clip_tree: &crate::clip_tree::ClipTree,
    frame_id: crate::frame_tree::FrameId,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    let frame = frame_tree.frame(frame_id);
    let point_local = map_world_to_frame_local(frame, point_world);

    for command in frame.paint_list.iter().rev() {
        match *command {
            FramePaintCommand::ChildFrame(child_id) => {
                if let Some(hit) = hit_test_frame_reverse(frame_tree, clip_tree, child_id, point_world) {
                    return Some(hit);
                }
            }
            FramePaintCommand::Item(item_index) => {
                let item = &frame.items[item_index];
                if !clip_chain_contains_point(frame_tree, clip_tree, item.clip_id, point_world) {
                    continue;
                }
                if hit_test_item_local(item, point_local) {
                    if let Some(tag) = item.source.fragment().tag() {
                        return Some(tag.node);
                    }
                }
            }
        }
    }

    None
}

fn find_scroll_container_in_frame_reverse(
    frame_tree: &crate::frame_tree::FrameTree<'_>,
    clip_tree: &crate::clip_tree::ClipTree,
    frame_id: crate::frame_tree::FrameId,
    point_world: DVec2,
) -> Option<OpaqueNode> {
    let frame = frame_tree.frame(frame_id);

    for command in frame.paint_list.iter().rev() {
        if let FramePaintCommand::ChildFrame(child_id) = *command {
            if let Some(hit) = find_scroll_container_in_frame_reverse(frame_tree, clip_tree, child_id, point_world) {
                return Some(hit);
            }
        }
    }

    if frame.kind == crate::frame_tree::FrameKind::ScrollFrame {
        if !clip_chain_contains_point(frame_tree, clip_tree, frame.clip_id, point_world) {
            return None;
        }
        if let Some(node_id) = frame.owner_node_id {
            return Some(OpaqueNode(node_id));
        }
    }

    None
}

fn map_world_to_frame_local(
    frame: &crate::frame_tree::RenderFrame<'_>,
    point_world: DVec2,
) -> DVec2 {
    let mapped = frame
        .matrix
        .world_inverse
        .transform_vec4(vec4f(point_world.x as f32, point_world.y as f32, 0.0, 1.0));
    if mapped.w.abs() > 1e-6 {
        dvec2((mapped.x / mapped.w) as f64, (mapped.y / mapped.w) as f64)
    } else {
        dvec2(mapped.x as f64, mapped.y as f64)
    }
}

fn clip_chain_contains_point(
    frame_tree: &crate::frame_tree::FrameTree<'_>,
    clip_tree: &crate::clip_tree::ClipTree,
    clip_id: crate::clip_tree::ClipId,
    point_world: DVec2,
) -> bool {
    if clip_id == crate::clip_tree::ClipId::INVALID {
        return true;
    }

    let mut current = clip_id;
    while current != crate::clip_tree::ClipId::INVALID {
        let node = clip_tree.get(current);
        let frame = frame_tree.frame(node.parent_frame_id);
        let point_local = map_world_to_frame_local(frame, point_world);
        if !point_in_rect(point_local, node.rect) {
            return false;
        }
        current = node.parent_clip_id;
    }
    true
}

fn hit_test_item_local(
    item: &crate::frame_tree::FramePaintItem<'_>,
    point_local: DVec2,
) -> bool {
    let rect = match item.source.fragment() {
        Fragment::Box(bf) | Fragment::Float(bf) => bf.border_rect(),
        Fragment::Text(tf) => tf.base.rect,
        Fragment::Image(img) => img.base.rect,
        Fragment::IFrame(iframe) => iframe.base.rect,
        Fragment::Positioning(_) => return false,
    };

    point_in_rect(
        point_local,
        Rect {
            pos: dvec2(
                item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                item.local_origin.y + rect.origin.y.to_f32_px() as f64,
            ),
            size: dvec2(
                rect.size.width.to_f32_px() as f64,
                rect.size.height.to_f32_px() as f64,
            ),
        },
    )
}

fn point_in_rect(point: DVec2, rect: Rect) -> bool {
    point.x >= rect.pos.x
        && point.x < rect.pos.x + rect.size.x
        && point.y >= rect.pos.y
        && point.y < rect.pos.y + rect.size.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip_tree::{ClipId, ClipTree};
    use crate::frame_tree::{FrameKey, FrameKind, FrameTree};
    use havi_types::fragment_tree::{BaseFragment, BaseFragmentInfo, Baselines, BoxFragment};
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

    fn make_box(node_id: usize, x: f32, y: f32, w: f32, h: f32) -> Fragment {
        use app_units::Au;
        let sides = PhysicalSides::new(Au(0), Au(0), Au(0), Au(0));
        Fragment::Box(BoxFragment {
            base: BaseFragment::new(
                BaseFragmentInfo::new(OpaqueNode(node_id)),
                initial_style(),
                make_rect(x, y, w, h),
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

    fn translation(tx: f32, ty: f32) -> Mat4f {
        Mat4f {
            v: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                tx, ty, 0.0, 1.0,
            ],
        }
    }

    #[test]
    fn scene_hit_test_hits_topmost_child_frame() {
        let mut frames = FrameTree::new();
        let clips = ClipTree::new();
        let a = make_box(1, 0.0, 0.0, 100.0, 100.0);
        let b = make_box(2, 0.0, 0.0, 100.0, 100.0);
        frames.push_item(frames.root, crate::paint_items::PaintSource::Direct(&a), crate::layout_stacking_context::StackingContextSection::Foreground, dvec2(0.0, 0.0), ClipId::INVALID);
        let child = frames.push_child_frame(frames.root, FrameKey::NodeReferenceFrame(2), FrameKind::ReferenceFrame, Some(2), translation(10.0, 0.0));
        frames.append_child_frame(frames.root, child);
        frames.push_item(child, crate::paint_items::PaintSource::Direct(&b), crate::layout_stacking_context::StackingContextSection::Foreground, dvec2(0.0, 0.0), ClipId::INVALID);

        assert_eq!(hit_test(&frames, &clips, dvec2(20.0, 20.0)), Some(OpaqueNode(2)));
        assert_eq!(hit_test(&frames, &clips, dvec2(5.0, 5.0)), Some(OpaqueNode(1)));
    }

    #[test]
    fn scene_hit_test_respects_clip_chain() {
        let mut frames = FrameTree::new();
        let mut clips = ClipTree::new();
        let fragment = make_box(3, 0.0, 0.0, 100.0, 100.0);
        let clip_id = clips.push_rect(frames.root, ClipId::INVALID, Rect { pos: dvec2(10.0, 10.0), size: dvec2(20.0, 20.0) });
        frames.push_item(frames.root, crate::paint_items::PaintSource::Direct(&fragment), crate::layout_stacking_context::StackingContextSection::Foreground, dvec2(0.0, 0.0), clip_id);

        assert_eq!(hit_test(&frames, &clips, dvec2(15.0, 15.0)), Some(OpaqueNode(3)));
        assert_eq!(hit_test(&frames, &clips, dvec2(5.0, 5.0)), None);
    }

    #[test]
    fn scene_find_scroll_container_returns_innermost_scroll_frame() {
        let mut frames = FrameTree::new();
        let mut clips = ClipTree::new();
        let outer_fragment = make_box(10, 0.0, 0.0, 200.0, 200.0);
        frames.push_item(frames.root, crate::paint_items::PaintSource::Direct(&outer_fragment), crate::layout_stacking_context::StackingContextSection::Foreground, dvec2(0.0, 0.0), ClipId::INVALID);
        let outer_clip = clips.push_rect(frames.root, ClipId::INVALID, Rect { pos: dvec2(0.0, 0.0), size: dvec2(200.0, 200.0) });
        let outer_scroll = frames.push_child_frame(frames.root, FrameKey::NodeScrollFrame(10), FrameKind::ScrollFrame, Some(10), Mat4f::identity());
        frames.set_clip(outer_scroll, outer_clip);
        frames.append_child_frame(frames.root, outer_scroll);
        frames.push_item(outer_scroll, crate::paint_items::PaintSource::Direct(&outer_fragment), crate::layout_stacking_context::StackingContextSection::Foreground, dvec2(0.0, 0.0), outer_clip);

        let inner_fragment = make_box(20, 20.0, 20.0, 50.0, 50.0);
        let inner_clip = clips.push_rect(outer_scroll, outer_clip, Rect { pos: dvec2(20.0, 20.0), size: dvec2(50.0, 50.0) });
        let inner_scroll = frames.push_child_frame(outer_scroll, FrameKey::NodeScrollFrame(20), FrameKind::ScrollFrame, Some(20), Mat4f::identity());
        frames.set_clip(inner_scroll, inner_clip);
        frames.append_child_frame(outer_scroll, inner_scroll);
        frames.push_item(inner_scroll, crate::paint_items::PaintSource::Direct(&inner_fragment), crate::layout_stacking_context::StackingContextSection::Foreground, dvec2(20.0, 20.0), inner_clip);

        assert_eq!(find_scroll_container(&frames, &clips, dvec2(30.0, 30.0)), Some(OpaqueNode(20)));
        assert_eq!(find_scroll_container(&frames, &clips, dvec2(5.0, 5.0)), Some(OpaqueNode(10)));
        assert_eq!(find_scroll_container(&frames, &clips, dvec2(250.0, 250.0)), None);
    }
}
