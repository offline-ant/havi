use layout::fragment_tree::{BoxFragment, Fragment, FragmentFlags};
use makepad_browser_scene::{MpDocument, MpScene, ResourceRegistry};
use makepad_widgets::Cx2d;

use super::box_fragment::build_box_fragment;
use super::iframe::build_iframe_fragment;
use super::{
    log_builder_skip_once, BrowserDocumentScrollNodes, BuildContext, BuildState, DirectBuilderIds,
};
use crate::browser_scene_primitives::paint_run_item_to_primitives;
use crate::layout_stacking_context::StackingContextSection;
use crate::paint_items::RenderPaintItem;

pub(super) fn build_fragment_list(
    cx: &mut Cx2d,
    fragments: &[Fragment],
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&MpDocument>,
) -> Result<(), String> {
    for fragment in fragments {
        build_fragment(
            cx,
            fragment,
            scroll_state,
            scene,
            registry,
            state,
            ids,
            build_cx,
            scroll_nodes,
            previous_document,
        )?;
    }
    Ok(())
}

pub(super) fn build_fragment(
    cx: &mut Cx2d,
    fragment: &Fragment,
    scroll_state: &crate::ScrollState,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    ids: &mut DirectBuilderIds,
    build_cx: BuildContext,
    scroll_nodes: &mut BrowserDocumentScrollNodes,
    previous_document: Option<&MpDocument>,
) -> Result<(), String> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            let bf = bf.borrow();
            build_box_fragment(
                cx,
                fragment,
                &bf,
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
        Fragment::Text(tf) => {
            let tf = tf.borrow();
            if tf.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            push_fragment_primitives(
                cx,
                scene,
                registry,
                state,
                &RenderPaintItem {
                    section: StackingContextSection::Foreground,
                    local_origin: build_cx.containing_block_origin,
                    source: fragment,
                },
                owner_node_id_for_fragment(fragment),
                build_cx,
            )
        }
        Fragment::Image(image) => {
            let image = image.borrow();
            if image.base.flags.intersects(FragmentFlags::DO_NOT_PAINT) {
                return Ok(());
            }
            push_fragment_primitives(
                cx,
                scene,
                registry,
                state,
                &RenderPaintItem {
                    section: StackingContextSection::Foreground,
                    local_origin: build_cx.containing_block_origin,
                    source: fragment,
                },
                owner_node_id_for_fragment(fragment),
                build_cx,
            )
        }
        Fragment::Positioning(positioning) => {
            let positioning = positioning.borrow();
            build_fragment_list(
                cx,
                &positioning.children,
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
        Fragment::AbsoluteOrFixedPositioned(resolved) => {
            let resolved = resolved.borrow().fragment.clone();
            let Some(resolved) = resolved else {
                return Ok(());
            };
            build_fragment(
                cx,
                &resolved,
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
        Fragment::IFrame(iframe) => {
            let iframe = iframe.borrow();
            build_iframe_fragment(
                cx,
                fragment,
                &iframe,
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
    }
}

pub(super) fn push_fragment_primitives(
    cx: &mut Cx2d,
    scene: &mut MpScene,
    registry: &mut ResourceRegistry,
    state: &mut BuildState,
    item: &RenderPaintItem<'_>,
    owner_node_id: Option<usize>,
    build_cx: BuildContext,
) -> Result<(), String> {
    let primitives = match paint_run_item_to_primitives(
        cx,
        scene,
        registry,
        &mut state.glyph_runs,
        item,
        owner_node_id,
        build_cx.spatial_id,
        build_cx.clip_chain_id,
        build_cx.effect_id,
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

fn owner_node_id_for_box(bf: &BoxFragment) -> Option<usize> {
    let node_id = bf.base.tag.map(|tag| tag.node.0)?;
    let pseudo_key = match bf.style().pseudo() {
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

pub(super) fn owner_node_id_for_fragment(fragment: &Fragment) -> Option<usize> {
    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => owner_node_id_for_box(&bf.borrow()),
        Fragment::Text(tf) => tf.borrow().base.tag.map(|tag| tag.node.0),
        Fragment::Image(image) => image.borrow().base.tag.map(|tag| tag.node.0),
        Fragment::IFrame(iframe) => iframe.borrow().base.tag.map(|tag| tag.node.0),
        Fragment::Positioning(positioning) => positioning.borrow().base.tag.map(|tag| tag.node.0),
        Fragment::AbsoluteOrFixedPositioned(_) => None,
    }
}
