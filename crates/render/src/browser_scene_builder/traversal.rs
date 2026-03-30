use havi_types::fragment_tree as published;
use makepad_browser_scene::{MpDocument, MpScene, ResourceRegistry};
use makepad_widgets::{dvec2, Cx2d};

use super::box_fragment::build_box_fragment;
use super::iframe::build_iframe_fragment;
use super::svg::{
    build_svg_container_fragment, build_svg_leaf_fragment, build_svg_viewport_fragment,
};
use super::{
    log_builder_skip_once, BrowserDocumentScrollNodes, BuildContext, BuildState, DirectBuilderIds,
};
use crate::browser_scene_primitives::paint_run_item_to_primitives;
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;

pub(super) fn build_paint_list(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    children: &[published::PaintChild],
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&MpDocument>,
) -> Result<(), String> {
    for child in children {
        build_paint_child(
            cx,
            generation,
            child,
            scroll_state,
            scene,
            registry,
            state,
            ids,
            build_cx.clone(),
            scroll_nodes,
            previous_document,
        )?;
    }
    Ok(())
}

fn build_paint_child(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    child: &published::PaintChild,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&MpDocument>,
) -> Result<(), String> {
    let (fragment_id, build_cx) = match child {
        published::PaintChild::Fragment(fragment_id) => (*fragment_id, build_cx),
        published::PaintChild::Placement(placement_id) => {
            let placement = generation.placement(*placement_id);
            let mut placement_cx = build_cx;
            if placement.containing_block.is_none() {
                // Containing block is the ICB — origin is (0, 0).
                placement_cx.containing_block_origin = dvec2(0.0, 0.0);
            }
            (placement.fragment, placement_cx)
        }
    };
    build_fragment(
        cx,
        generation,
        fragment_id,
        scroll_state,
        scene,
        registry,
        state,
        ids,
        build_cx,
        scroll_nodes,
        previous_document,
    )
}

pub(super) fn build_fragment(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&MpDocument>,
) -> Result<(), String> {
    match generation.kind(fragment_id) {
        published::FragmentKind::Box(bf) | published::FragmentKind::Float(bf) => build_box_fragment(
            cx,
            generation,
            fragment_id,
            bf,
            scroll_state,
            scene,
            registry,
            state,
            ids,
            build_cx,
            scroll_nodes,
            previous_document,
        ),
        published::FragmentKind::Text(tf) => {
            if tf.base.flags.intersects(published::FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            push_fragment_primitives(
                cx,
                generation,
                scene,
                registry,
                state,
                &RenderPaintItem {
                    section: StackingContextSection::Foreground,
                    local_origin: build_cx.containing_block_origin,
                    fragment_id,
                },
                owner_node_id_for_fragment(generation, fragment_id),
                build_cx,
            )
        }
        published::FragmentKind::Image(image) => {
            if image.base.flags.intersects(published::FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            push_fragment_primitives(
                cx,
                generation,
                scene,
                registry,
                state,
                &RenderPaintItem {
                    section: StackingContextSection::Foreground,
                    local_origin: build_cx.containing_block_origin,
                    fragment_id,
                },
                owner_node_id_for_fragment(generation, fragment_id),
                build_cx,
            )
        }
        published::FragmentKind::Positioning(positioning) => build_paint_list(
            cx,
            generation,
            &positioning.paint_children,
            scroll_state,
            scene,
            registry,
            state,
            ids,
            build_cx,
            scroll_nodes,
            previous_document,
        ),
        published::FragmentKind::SVGViewport(svg) => build_svg_viewport_fragment(
            cx,
            generation,
            fragment_id,
            svg,
            scroll_state,
            scene,
            registry,
            state,
            ids,
            build_cx,
            scroll_nodes,
            previous_document,
        ),
        published::FragmentKind::SVGContainer(svg) => build_svg_container_fragment(
            cx,
            generation,
            fragment_id,
            svg,
            scroll_state,
            scene,
            registry,
            state,
            ids,
            build_cx,
            scroll_nodes,
            previous_document,
        ),
        published::FragmentKind::SVGLeaf(svg) => {
            if svg.base.flags.intersects(published::FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            build_svg_leaf_fragment(
                cx,
                generation,
                fragment_id,
                svg,
                scene,
                registry,
                state,
                ids,
                build_cx,
            )
        }
        published::FragmentKind::IFrame(iframe) => build_iframe_fragment(
            cx,
            generation,
            fragment_id,
            iframe,
            scroll_state,
            scene,
            registry,
            state,
            ids,
            build_cx,
            scroll_nodes,
            previous_document,
        ),
    }
}

pub(super) fn push_fragment_primitives(
    cx: &mut Cx2d,
    generation: &published::FragmentArenaGeneration,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    item: &RenderPaintItem,
    owner_node_id: Option<usize>,
    build_cx: BuildContext,
) -> Result<(), String> {
    let primitives = match paint_run_item_to_primitives(
        cx,
        generation,
        scene,
        registry,
        &mut state.glyph_runs,
        item,
        owner_node_id,
        build_cx.spatial_id,
        build_cx.clip_chain_id,
        build_cx.effect_id,
        build_cx.svg_paint_context.as_ref(),
    ) {
        Ok(primitives) => primitives,
        Err(reason) => {
            log_builder_skip_once(reason);
            return Ok(());
        }
    };
    for primitive in primitives {
        scene.push_primitive(primitive);
    }
    Ok(())
}

fn owner_node_id_for_box(bf: &published::BoxFragment) -> Option<usize> {
    let node_id = bf.base.tag.map(|tag| tag.node.0)?;
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
    Some((node_id << 8) ^ pseudo_key)
}

pub(super) fn owner_node_id_for_fragment(
    generation: &published::FragmentArenaGeneration,
    fragment_id: published::FragmentId,
) -> Option<usize> {
    match generation.kind(fragment_id) {
        published::FragmentKind::Box(bf) | published::FragmentKind::Float(bf) => owner_node_id_for_box(bf),
        published::FragmentKind::Text(tf) => tf.base.tag.map(|tag| tag.node.0),
        published::FragmentKind::Image(image) => image.base.tag.map(|tag| tag.node.0),
        published::FragmentKind::IFrame(iframe) => iframe.base.tag.map(|tag| tag.node.0),
        published::FragmentKind::Positioning(positioning) => positioning.base.tag.map(|tag| tag.node.0),
        published::FragmentKind::SVGViewport(svg) => {
            Some(svg.identity.current_instance_owner_or_source_tag().node.0)
        }
        published::FragmentKind::SVGContainer(svg) => {
            Some(svg.identity.current_instance_owner_or_source_tag().node.0)
        }
        published::FragmentKind::SVGLeaf(svg) => {
            Some(svg.identity.current_instance_owner_or_source_tag().node.0)
        }
    }
}
