use base::id::ScrollTreeNodeId;
use havi_types::fragment_tree::{
    FragmentArenaGeneration, FragmentId, FragmentKind, PaintChild,
};
use layout_api::{
    shared_committed_scroll_offsets_for_pipeline, shared_layout_fragment_tree_for_pipeline,
    SharedLayoutFragmentTree, SharedScrollState,
};
use makepad_widgets::{dvec2, DVec2, Rect};
use paint_api::scroll_tree::{
    AxesScrollSensitivity, ScrollTree, ScrollType, ScrollableNodeInfo, SpatialTreeNodeInfo,
};
use rustc_hash::{FxHashMap, FxHashSet};
use style::computed_values::overflow_x::T as ComputedOverflow;
use webrender_api::units::{LayoutPoint, LayoutRect, LayoutSize, LayoutVector2D};
use webrender_api::{ExternalScrollId, PipelineId};

#[derive(Clone)]
pub struct BrowserScrollCommit {
    pub scrolled_node: ExternalScrollId,
    pub offsets: FxHashMap<ExternalScrollId, LayoutVector2D>,
}

#[derive(Default)]
pub struct BrowserScrollController {
    webview_id: Option<base::id::WebViewId>,
    root_pipeline_id: Option<PipelineId>,
    sampled_offsets: FxHashMap<ExternalScrollId, LayoutVector2D>,
    structural_tree: ScrollTree,
    node_ids: FxHashMap<ExternalScrollId, ScrollTreeNodeId>,
    painted_node_order: Vec<ScrollTreeNodeId>,
    seeded_from_layout_snapshot: bool,
}

impl BrowserScrollController {
    pub fn attach_webview(
        &mut self,
        webview_id: base::id::WebViewId,
        root_pipeline_id: Option<PipelineId>,
    ) {
        if self.webview_id == Some(webview_id) && self.root_pipeline_id == root_pipeline_id {
            return;
        }

        self.webview_id = Some(webview_id);
        self.root_pipeline_id = root_pipeline_id;
        self.sampled_offsets.clear();
        self.structural_tree = ScrollTree::default();
        self.node_ids.clear();
        self.painted_node_order.clear();
        self.seeded_from_layout_snapshot = false;
    }

    pub fn root_pipeline_id(&self) -> Option<PipelineId> {
        self.root_pipeline_id
    }

    pub fn sync_from_layout(
        &mut self,
        shared_fragments: &SharedLayoutFragmentTree,
        scroll_snapshot: Option<&SharedScrollState>,
    ) {
        let Some(root_pipeline_id) = self.root_pipeline_id else {
            return;
        };
        let Some(root_generation) = shared_fragments.get::<FragmentArenaGeneration>() else {
            self.sampled_offsets.clear();
            self.structural_tree = ScrollTree::default();
            self.node_ids.clear();
            self.painted_node_order.clear();
            self.seeded_from_layout_snapshot = false;
            return;
        };

        let mut next_nodes = FxHashSet::default();
        let mut structural_tree = ScrollTree::default();
        let mut node_ids = FxHashMap::default();
        let mut paint_order = Vec::new();

        collect_pipeline_scroll_tree(
            root_generation.as_ref(),
            root_pipeline_id,
            dvec2(0.0, 0.0),
            None,
            &mut structural_tree,
            &mut node_ids,
            &mut next_nodes,
            &mut paint_order,
        );

        if !self.seeded_from_layout_snapshot {
            if let Some(scroll_snapshot) = scroll_snapshot {
                self.seed_from_layout_snapshot(root_pipeline_id, &next_nodes, scroll_snapshot);
            }
            self.seeded_from_layout_snapshot = true;
        }

        self.sampled_offsets.retain(|id, _| next_nodes.contains(id));
        for id in &next_nodes {
            self.sampled_offsets
                .entry(*id)
                .or_insert_with(LayoutVector2D::zero);
        }

        self.structural_tree = structural_tree;
        self.node_ids = node_ids;
        self.painted_node_order = paint_order;
    }

    pub fn render_scroll_state(&self) -> havi_render::ScrollState {
        self.sampled_offsets
            .iter()
            .map(|(&id, &offset)| (id, dvec2(offset.x as f64, offset.y as f64)))
            .collect()
    }

    pub fn hit_test_scroll_node(&self, point: DVec2) -> Option<ExternalScrollId> {
        for node_id in self.painted_node_order.iter().rev() {
            let Some(info) = self.scroll_info_for_node_id(*node_id) else {
                continue;
            };
            if !info.scroll_sensitivity.x.contains(ScrollType::InputEvents)
                && !info.scroll_sensitivity.y.contains(ScrollType::InputEvents)
            {
                continue;
            }

            let external_id = info.external_id;
            if self.current_visible_rect(external_id).contains(point) {
                return Some(external_id);
            }
        }
        None
    }

    pub fn apply_scroll_delta_at_point(
        &mut self,
        point: DVec2,
        delta: DVec2,
    ) -> Option<BrowserScrollCommit> {
        if delta.x == 0.0 && delta.y == 0.0 {
            return None;
        }

        let hit = self.hit_test_scroll_node(point)?;
        let target = self.scroll_node_or_ancestor_inner(hit, delta)?;
        self.apply_scroll_delta_to_node(target, delta)
    }

    pub fn apply_root_scroll_delta(&mut self, delta: DVec2) -> Option<BrowserScrollCommit> {
        if delta.x == 0.0 && delta.y == 0.0 {
            return None;
        }
        let root_id = ExternalScrollId(0, self.root_pipeline_id?);
        self.apply_scroll_delta_to_node(root_id, delta)
    }

    fn apply_scroll_delta_to_node(
        &mut self,
        target: ExternalScrollId,
        delta: DVec2,
    ) -> Option<BrowserScrollCommit> {
        let current = self
            .sampled_offsets
            .get(&target)
            .copied()
            .unwrap_or_else(LayoutVector2D::zero);
        let next = self.scroll_to_offset(
            target,
            LayoutVector2D::new(current.x + delta.x as f32, current.y + delta.y as f32),
            ScrollType::InputEvents,
        )?;
        if next == current {
            return None;
        }

        self.sampled_offsets.insert(target, next);
        Some(BrowserScrollCommit {
            scrolled_node: target,
            offsets: self.sampled_offsets.clone(),
        })
    }

    fn seed_from_layout_snapshot(
        &mut self,
        root_pipeline_id: PipelineId,
        nodes: &FxHashSet<ExternalScrollId>,
        root_scroll_snapshot: &SharedScrollState,
    ) {
        let mut pipeline_offsets = FxHashMap::default();

        for &id in nodes {
            let offsets = pipeline_offsets
                .entry(id.1)
                .or_insert_with(|| shared_committed_scroll_offsets_for_pipeline(id.1.into()).get());

            let offset = offsets.get(&id).copied().unwrap_or_else(|| {
                if id.0 == 0 && id.1 == root_pipeline_id {
                    LayoutVector2D::new(0.0, root_scroll_snapshot.get().scroll_y as f32)
                } else {
                    LayoutVector2D::zero()
                }
            });
            self.sampled_offsets.insert(id, offset);
        }
    }

    fn scroll_node_or_ancestor_inner(
        &self,
        start: ExternalScrollId,
        delta: DVec2,
    ) -> Option<ExternalScrollId> {
        let mut current = Some(start);
        while let Some(id) = current {
            let info = self.scroll_info(id)?;
            if self.node_accepts_input(info.scroll_sensitivity, delta) {
                let current_offset = self
                    .sampled_offsets
                    .get(&id)
                    .copied()
                    .unwrap_or_else(LayoutVector2D::zero);
                let next = self.scroll_to_offset(
                    id,
                    LayoutVector2D::new(current_offset.x + delta.x as f32, current_offset.y + delta.y as f32),
                    ScrollType::InputEvents,
                )?;
                if next != current_offset {
                    return Some(id);
                }
            }
            current = self.parent_scroll_node(id);
        }
        None
    }

    fn scroll_to_offset(
        &self,
        id: ExternalScrollId,
        new_offset: LayoutVector2D,
        context: ScrollType,
    ) -> Option<LayoutVector2D> {
        let info = self.scroll_info(id)?;
        if !info.scroll_sensitivity.x.contains(context) &&
            !info.scroll_sensitivity.y.contains(context)
        {
            return None;
        }

        let scrollable_size = self.scrollable_size(id)?;
        let current = self
            .sampled_offsets
            .get(&id)
            .copied()
            .unwrap_or_else(LayoutVector2D::zero);
        let mut offset = current;

        if scrollable_size.width > 0.0 && info.scroll_sensitivity.x.contains(context) {
            offset.x = new_offset.x.clamp(0.0, scrollable_size.width);
        }

        if scrollable_size.height > 0.0 && info.scroll_sensitivity.y.contains(context) {
            offset.y = new_offset.y.clamp(0.0, scrollable_size.height);
        }

        Some(offset)
    }

    fn scrollable_size(&self, id: ExternalScrollId) -> Option<LayoutSize> {
        let info = self.scroll_info(id)?;
        Some(info.content_rect.size() - info.clip_rect.size())
    }

    fn node_accepts_input(&self, sensitivity: AxesScrollSensitivity, delta: DVec2) -> bool {
        let wants_x = delta.x.abs() > 0.001;
        let wants_y = delta.y.abs() > 0.001;
        (wants_x && sensitivity.x.contains(ScrollType::InputEvents))
            || (wants_y && sensitivity.y.contains(ScrollType::InputEvents))
            || (!wants_x && !wants_y)
    }

    fn current_visible_rect(&self, id: ExternalScrollId) -> Rect {
        let mut rect = self.current_viewport_rect(id);
        let mut ancestor = self.parent_scroll_node(id);
        while let Some(ancestor_id) = ancestor {
            let ancestor_rect = self.current_viewport_rect(ancestor_id);
            rect = intersect_rects(rect, ancestor_rect);
            ancestor = self.parent_scroll_node(ancestor_id);
        }
        rect
    }

    fn current_viewport_rect(&self, id: ExternalScrollId) -> Rect {
        let info = self
            .scroll_info(id)
            .expect("scroll node missing during viewport rect lookup");
        let mut pos = layout_point_to_dvec2(info.clip_rect.min);
        let size = layout_size_to_dvec2(info.clip_rect.size());
        let mut ancestor = self.parent_scroll_node(id);
        while let Some(ancestor_id) = ancestor {
            let offset = self
                .sampled_offsets
                .get(&ancestor_id)
                .copied()
                .unwrap_or_else(LayoutVector2D::zero);
            pos -= dvec2(offset.x as f64, offset.y as f64);
            ancestor = self.parent_scroll_node(ancestor_id);
        }
        Rect { pos, size }
    }

    fn parent_scroll_node(&self, id: ExternalScrollId) -> Option<ExternalScrollId> {
        let node_id = *self.node_ids.get(&id)?;
        let parent_id = self.structural_tree.get_node(node_id).parent?;
        self.structural_tree.get_node(parent_id).external_id()
    }

    fn scroll_info(&self, id: ExternalScrollId) -> Option<&ScrollableNodeInfo> {
        let node_id = *self.node_ids.get(&id)?;
        self.scroll_info_for_node_id(node_id)
    }

    fn scroll_info_for_node_id(&self, node_id: ScrollTreeNodeId) -> Option<&ScrollableNodeInfo> {
        match &self.structural_tree.get_node(node_id).info {
            SpatialTreeNodeInfo::Scroll(info) => Some(info),
            _ => None,
        }
    }
}

fn collect_pipeline_scroll_tree(
    generation: &FragmentArenaGeneration,
    pipeline_id: PipelineId,
    pipeline_origin: DVec2,
    parent_scroll_node: Option<ScrollTreeNodeId>,
    structural_tree: &mut ScrollTree,
    node_ids: &mut FxHashMap<ExternalScrollId, ScrollTreeNodeId>,
    next_nodes: &mut FxHashSet<ExternalScrollId>,
    paint_order: &mut Vec<ScrollTreeNodeId>,
) -> ScrollTreeNodeId {
    let root_scroll_id = ExternalScrollId(0, pipeline_id);
    next_nodes.insert(root_scroll_id);
    let root_scroll_node = register_scroll_node(
        root_scroll_id,
        parent_scroll_node,
        Rect {
            pos: pipeline_origin + physical_point_to_dvec2(generation.initial_containing_block.origin),
            size: physical_size_to_dvec2(generation.initial_containing_block.size),
        },
        physical_size_to_dvec2(generation.scrollable_overflow.size),
        input_event_scroll_sensitivity(),
        structural_tree,
        node_ids,
        paint_order,
    );

    collect_paint_list_scroll_tree(
        generation,
        generation.paint_roots.as_ref(),
        pipeline_id,
        pipeline_origin,
        root_scroll_node,
        structural_tree,
        node_ids,
        next_nodes,
        paint_order,
    );

    root_scroll_node
}

fn collect_paint_list_scroll_tree(
    generation: &FragmentArenaGeneration,
    children: &[PaintChild],
    pipeline_id: PipelineId,
    pipeline_origin: DVec2,
    current_scroll_parent: ScrollTreeNodeId,
    structural_tree: &mut ScrollTree,
    node_ids: &mut FxHashMap<ExternalScrollId, ScrollTreeNodeId>,
    next_nodes: &mut FxHashSet<ExternalScrollId>,
    paint_order: &mut Vec<ScrollTreeNodeId>,
) {
    for child in children {
        let fragment_id = match child {
            PaintChild::Fragment(fragment_id) => *fragment_id,
            PaintChild::Placement(placement_id) => generation.placement(*placement_id).fragment,
        };
        collect_fragment_scroll_tree(
            generation,
            fragment_id,
            pipeline_id,
            pipeline_origin,
            current_scroll_parent,
            structural_tree,
            node_ids,
            next_nodes,
            paint_order,
        );
    }
}

fn collect_fragment_scroll_tree(
    generation: &FragmentArenaGeneration,
    fragment_id: FragmentId,
    pipeline_id: PipelineId,
    pipeline_origin: DVec2,
    current_scroll_parent: ScrollTreeNodeId,
    structural_tree: &mut ScrollTree,
    node_ids: &mut FxHashMap<ExternalScrollId, ScrollTreeNodeId>,
    next_nodes: &mut FxHashSet<ExternalScrollId>,
    paint_order: &mut Vec<ScrollTreeNodeId>,
) {
    match generation.kind(fragment_id) {
        FragmentKind::Box(fragment) | FragmentKind::Float(fragment) => {
            let mut child_scroll_parent = current_scroll_parent;
            if havi_render::is_scroll_container(fragment) {
                if let Some(tag) = fragment.base.tag {
                    let id = ExternalScrollId(tag.node.0 as u64, pipeline_id);
                    next_nodes.insert(id);
                    child_scroll_parent = register_scroll_node(
                        id,
                        Some(current_scroll_parent),
                        absolute_box_border_rect(generation, fragment_id, pipeline_origin),
                        absolute_box_scrollable_size(generation, fragment_id),
                        scroll_sensitivity_for_box(fragment),
                        structural_tree,
                        node_ids,
                        paint_order,
                    );
                }
            }
            collect_paint_list_scroll_tree(
                generation,
                fragment.paint_children.as_slice(),
                pipeline_id,
                pipeline_origin,
                child_scroll_parent,
                structural_tree,
                node_ids,
                next_nodes,
                paint_order,
            );
        }
        FragmentKind::Positioning(fragment) => {
            collect_paint_list_scroll_tree(
                generation,
                fragment.paint_children.as_slice(),
                pipeline_id,
                pipeline_origin,
                current_scroll_parent,
                structural_tree,
                node_ids,
                next_nodes,
                paint_order,
            );
        }
        FragmentKind::IFrame(fragment) => {
            let child_pipeline_id: PipelineId = fragment.pipeline_id.into();
            let child_origin =
                pipeline_origin + absolute_fragment_origin_in_pipeline(generation, fragment_id);
            if let Some(child_generation) =
                shared_layout_fragment_tree_for_pipeline(fragment.pipeline_id)
                    .get::<FragmentArenaGeneration>()
            {
                collect_pipeline_scroll_tree(
                    child_generation.as_ref(),
                    child_pipeline_id,
                    child_origin,
                    Some(current_scroll_parent),
                    structural_tree,
                    node_ids,
                    next_nodes,
                    paint_order,
                );
            }
        }
        FragmentKind::SVGViewport(svg) => {
            collect_paint_list_scroll_tree(
                generation,
                svg.paint_children.as_slice(),
                pipeline_id,
                pipeline_origin,
                current_scroll_parent,
                structural_tree,
                node_ids,
                next_nodes,
                paint_order,
            );
        }
        FragmentKind::SVGContainer(svg) => {
            collect_paint_list_scroll_tree(
                generation,
                svg.paint_children.as_slice(),
                pipeline_id,
                pipeline_origin,
                current_scroll_parent,
                structural_tree,
                node_ids,
                next_nodes,
                paint_order,
            );
        }
        FragmentKind::Text(_) | FragmentKind::Image(_) | FragmentKind::SVGLeaf(_) => {}
    }
}

fn register_scroll_node(
    id: ExternalScrollId,
    parent: Option<ScrollTreeNodeId>,
    clip_rect: Rect,
    content_size: DVec2,
    sensitivity: AxesScrollSensitivity,
    structural_tree: &mut ScrollTree,
    node_ids: &mut FxHashMap<ExternalScrollId, ScrollTreeNodeId>,
    paint_order: &mut Vec<ScrollTreeNodeId>,
) -> ScrollTreeNodeId {
    let clip_rect = layout_rect_from_rect(clip_rect);
    let node_id = structural_tree.add_scroll_tree_node(
        parent,
        SpatialTreeNodeInfo::Scroll(ScrollableNodeInfo {
            external_id: id,
            content_rect: LayoutRect::from_origin_and_size(
                clip_rect.min,
                layout_size_from_dvec2(content_size),
            ),
            clip_rect,
            scroll_sensitivity: sensitivity,
        }),
    );
    node_ids.insert(id, node_id);
    paint_order.push(node_id);
    node_id
}

fn absolute_fragment_origin_in_pipeline(
    generation: &FragmentArenaGeneration,
    fragment_id: FragmentId,
) -> DVec2 {
    let containing_block_origin = generation.containing_block(fragment_id).origin;
    let local_origin = generation.base(fragment_id).rect.origin;
    dvec2(
        (containing_block_origin.x + local_origin.x).to_f32_px() as f64,
        (containing_block_origin.y + local_origin.y).to_f32_px() as f64,
    )
}

fn absolute_box_border_rect(
    generation: &FragmentArenaGeneration,
    fragment_id: FragmentId,
    pipeline_origin: DVec2,
) -> Rect {
    let (FragmentKind::Box(fragment) | FragmentKind::Float(fragment)) = generation.kind(fragment_id) else {
        unreachable!("box border rect requested for non-box fragment");
    };
    let rect = fragment
        .border_rect()
        .translate(generation.containing_block(fragment_id).origin.to_vector());
    Rect {
        pos: pipeline_origin + physical_point_to_dvec2(rect.origin),
        size: physical_size_to_dvec2(rect.size),
    }
}

fn absolute_box_scrollable_size(
    generation: &FragmentArenaGeneration,
    fragment_id: FragmentId,
) -> DVec2 {
    let rect = generation.scrollable_overflow_for(fragment_id);
    physical_size_to_dvec2(rect.size)
}

fn physical_point_to_dvec2(point: havi_types::PhysicalPoint<app_units::Au>) -> DVec2 {
    dvec2(point.x.to_f32_px() as f64, point.y.to_f32_px() as f64)
}

fn physical_size_to_dvec2(size: havi_types::PhysicalSize<app_units::Au>) -> DVec2 {
    dvec2(size.width.to_f32_px() as f64, size.height.to_f32_px() as f64)
}

fn layout_point_to_dvec2(point: LayoutPoint) -> DVec2 {
    dvec2(point.x as f64, point.y as f64)
}

fn layout_size_to_dvec2(size: LayoutSize) -> DVec2 {
    dvec2(size.width as f64, size.height as f64)
}

fn layout_size_from_dvec2(size: DVec2) -> LayoutSize {
    LayoutSize::new(size.x as f32, size.y as f32)
}

fn layout_rect_from_rect(rect: Rect) -> LayoutRect {
    LayoutRect::from_origin_and_size(
        LayoutPoint::new(rect.pos.x as f32, rect.pos.y as f32),
        layout_size_from_dvec2(rect.size),
    )
}

fn input_event_scroll_sensitivity() -> AxesScrollSensitivity {
    AxesScrollSensitivity {
        x: ScrollType::InputEvents | ScrollType::Script,
        y: ScrollType::InputEvents | ScrollType::Script,
    }
}

fn scroll_sensitivity_for_box(
    fragment: &havi_types::fragment_tree::BoxFragment,
) -> AxesScrollSensitivity {
    let overflow = fragment.style().get_box();
    AxesScrollSensitivity {
        x: scroll_type_from_overflow(overflow.overflow_x),
        y: scroll_type_from_overflow(overflow.overflow_y),
    }
}

fn scroll_type_from_overflow(overflow: ComputedOverflow) -> ScrollType {
    match overflow {
        ComputedOverflow::Hidden => ScrollType::Script,
        ComputedOverflow::Scroll | ComputedOverflow::Auto => {
            ScrollType::InputEvents | ScrollType::Script
        }
        ComputedOverflow::Visible | ComputedOverflow::Clip => ScrollType::empty(),
    }
}

fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let min_x = a.pos.x.max(b.pos.x);
    let min_y = a.pos.y.max(b.pos.y);
    let max_x = (a.pos.x + a.size.x).min(b.pos.x + b.size.x);
    let max_y = (a.pos.y + a.size.y).min(b.pos.y + b.size.y);
    Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_controller() -> BrowserScrollController {
        BrowserScrollController {
            root_pipeline_id: Some(PipelineId(0, 0)),
            ..Default::default()
        }
    }

    fn add_scroll_node(
        controller: &mut BrowserScrollController,
        external_id: ExternalScrollId,
        parent: Option<ScrollTreeNodeId>,
        clip_rect: Rect,
        content_size: DVec2,
        sensitivity: AxesScrollSensitivity,
    ) -> ScrollTreeNodeId {
        register_scroll_node(
            external_id,
            parent,
            clip_rect,
            content_size,
            sensitivity,
            &mut controller.structural_tree,
            &mut controller.node_ids,
            &mut controller.painted_node_order,
        )
    }

    fn input_sensitivity() -> AxesScrollSensitivity {
        AxesScrollSensitivity {
            x: ScrollType::InputEvents | ScrollType::Script,
            y: ScrollType::InputEvents | ScrollType::Script,
        }
    }

    #[test]
    fn root_scroll_clamps_in_controller() {
        let mut controller = build_controller();
        let root_id = ExternalScrollId(0, PipelineId(0, 0));
        add_scroll_node(
            &mut controller,
            root_id,
            None,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(100.0, 100.0),
            },
            dvec2(100.0, 300.0),
            input_sensitivity(),
        );

        let commit = controller
            .apply_root_scroll_delta(dvec2(0.0, 240.0))
            .expect("root scroll should apply");
        assert_eq!(commit.scrolled_node, root_id);
        assert_eq!(commit.offsets.get(&root_id), Some(&LayoutVector2D::new(0.0, 200.0)));
    }

    #[test]
    fn nested_overflow_scroll_targets_inner_node() {
        let mut controller = build_controller();
        let root_id = ExternalScrollId(0, PipelineId(0, 0));
        let root_node = add_scroll_node(
            &mut controller,
            root_id,
            None,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(100.0, 100.0),
            },
            dvec2(100.0, 300.0),
            input_sensitivity(),
        );
        let child_id = ExternalScrollId(1, PipelineId(0, 0));
        add_scroll_node(
            &mut controller,
            child_id,
            Some(root_node),
            Rect {
                pos: dvec2(10.0, 10.0),
                size: dvec2(50.0, 50.0),
            },
            dvec2(50.0, 120.0),
            input_sensitivity(),
        );

        let commit = controller
            .apply_scroll_delta_at_point(dvec2(20.0, 20.0), dvec2(0.0, 30.0))
            .expect("inner scroll should apply");
        assert_eq!(commit.scrolled_node, child_id);
        assert_eq!(commit.offsets.get(&child_id), Some(&LayoutVector2D::new(0.0, 30.0)));
        assert_eq!(commit.offsets.get(&root_id), None);
    }

    #[test]
    fn ancestor_handoff_scrolls_parent_at_child_extent() {
        let mut controller = build_controller();
        let root_id = ExternalScrollId(0, PipelineId(0, 0));
        let root_node = add_scroll_node(
            &mut controller,
            root_id,
            None,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(100.0, 100.0),
            },
            dvec2(100.0, 300.0),
            input_sensitivity(),
        );
        let child_id = ExternalScrollId(1, PipelineId(0, 0));
        add_scroll_node(
            &mut controller,
            child_id,
            Some(root_node),
            Rect {
                pos: dvec2(10.0, 10.0),
                size: dvec2(50.0, 50.0),
            },
            dvec2(50.0, 60.0),
            input_sensitivity(),
        );
        controller
            .sampled_offsets
            .insert(child_id, LayoutVector2D::new(0.0, 10.0));

        let commit = controller
            .apply_scroll_delta_at_point(dvec2(20.0, 20.0), dvec2(0.0, 15.0))
            .expect("parent should take over when child is at extent");
        assert_eq!(commit.scrolled_node, root_id);
        assert_eq!(commit.offsets.get(&root_id), Some(&LayoutVector2D::new(0.0, 15.0)));
        assert_eq!(commit.offsets.get(&child_id), Some(&LayoutVector2D::new(0.0, 10.0)));
    }

    #[test]
    fn overflow_hidden_hands_input_to_ancestor() {
        let mut controller = build_controller();
        let root_id = ExternalScrollId(0, PipelineId(0, 0));
        let root_node = add_scroll_node(
            &mut controller,
            root_id,
            None,
            Rect {
                pos: dvec2(0.0, 0.0),
                size: dvec2(100.0, 100.0),
            },
            dvec2(100.0, 300.0),
            input_sensitivity(),
        );
        let child_id = ExternalScrollId(1, PipelineId(0, 0));
        add_scroll_node(
            &mut controller,
            child_id,
            Some(root_node),
            Rect {
                pos: dvec2(10.0, 10.0),
                size: dvec2(50.0, 50.0),
            },
            dvec2(50.0, 120.0),
            AxesScrollSensitivity {
                x: ScrollType::Script,
                y: ScrollType::Script,
            },
        );

        let commit = controller
            .apply_scroll_delta_at_point(dvec2(20.0, 20.0), dvec2(0.0, 25.0))
            .expect("input scrolling should chain past overflow:hidden child");
        assert_eq!(commit.scrolled_node, root_id);
        assert_eq!(commit.offsets.get(&root_id), Some(&LayoutVector2D::new(0.0, 25.0)));
        assert_eq!(commit.offsets.get(&child_id), None);
    }
}
