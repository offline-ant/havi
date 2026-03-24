use makepad_browser_scene::{MpClipChainId, MpHitTestTag, MpPrimitive};
use makepad_widgets::{dvec2, vec2, Rect};
use style::color::AbsoluteColor;
use style::properties::ComputedValues;

use crate::color::resolve_color;

pub(super) fn append_box_shadow_primitives(
    primitives: &mut Vec<MpPrimitive>,
    computed: &ComputedValues,
    bounds: Rect,
    corner_radius_px: f32,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
    current_abs: &AbsoluteColor,
) {
    for shadow in computed.get_effects().box_shadow.0.iter().rev() {
        let horizontal = shadow.base.horizontal.px();
        let vertical = shadow.base.vertical.px();
        let blur = shadow.base.blur.px();
        let spread = shadow.spread.px();
        let color = resolve_color(&shadow.base.color, current_abs);
        if color.w <= 0.001 {
            continue;
        }
        let sigma = blur * 0.5;
        let extent = (sigma * 3.0).max(0.0);
        let (primitive_bounds, box_offset, box_size) = if shadow.inset {
            (
                bounds,
                vec2(spread + horizontal, spread + vertical),
                vec2(
                    (bounds.size.x as f32 - 2.0 * spread).max(0.0),
                    (bounds.size.y as f32 - 2.0 * spread).max(0.0),
                ),
            )
        } else {
            let shadow_width = bounds.size.x as f32 + 2.0 * spread;
            let shadow_height = bounds.size.y as f32 + 2.0 * spread;
            (
                Rect {
                    pos: dvec2(
                        bounds.pos.x + (horizontal - spread - extent) as f64,
                        bounds.pos.y + (vertical - spread - extent) as f64,
                    ),
                    size: dvec2(
                        (shadow_width + 2.0 * extent) as f64,
                        (shadow_height + 2.0 * extent) as f64,
                    ),
                },
                vec2(extent, extent),
                vec2(shadow_width.max(0.0), shadow_height.max(0.0)),
            )
        };
        let mut primitive = MpPrimitive {
            id: makepad_browser_scene::MpPrimitiveId(0),
            spatial_id,
            clip_chain_id,
            effect_id,
            bounds: primitive_bounds,
            kind: makepad_browser_scene::MpPrimitiveKind::BoxShadow(makepad_browser_scene::MpBoxShadow {
                color,
                box_offset,
                box_size,
                sigma,
                corner_radius_px,
                inset: shadow.inset,
            }),
            hit_test_tag: owner_node_id.map(|id| MpHitTestTag(id as u64)),
        };
        primitive.effect_id = effect_id;
        primitives.push(primitive);
    }
}
