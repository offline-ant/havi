use makepad_browser_scene::{MpClipChainId, MpHitTestTag, MpPerCornerRadius, MpPrimitive};
use makepad_widgets::{dvec2, Rect, Vec4f};
use style::color::AbsoluteColor;
use style::properties::ComputedValues;
use style::values::specified::border::BorderStyle;

use crate::background::resolve_border_radii;
use crate::color::resolve_color;

#[derive(Clone, Copy)]
pub(super) struct BorderSidePaint {
    width: f64,
    color: Vec4f,
    style: BorderStyle,
}

#[derive(Default)]
pub(super) struct BorderPaint {
    top: Option<BorderSidePaint>,
    right: Option<BorderSidePaint>,
    bottom: Option<BorderSidePaint>,
    left: Option<BorderSidePaint>,
}

#[derive(Clone, Copy)]
pub(super) struct OutlinePaint {
    width: f64,
    offset: f64,
    color: Vec4f,
    style: BorderStyle,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BorderSide {
    Top,
    Right,
    Bottom,
    Left,
}

pub(super) fn border_radius(computed: &ComputedValues) -> MpPerCornerRadius {
    let radii = resolve_border_radii(computed);
    MpPerCornerRadius {
        tl: radii.tl,
        tr: radii.tr,
        br: radii.br,
        bl: radii.bl,
    }
}

pub(super) fn border_paint(computed: &ComputedValues, current_abs: &AbsoluteColor) -> BorderPaint {
    let border = computed.get_border();
    let make_side = |style: BorderStyle,
                     width: style::values::computed::BorderSideWidth,
                     color: &style::values::computed::Color|
     -> Option<BorderSidePaint> {
        let width = width.0.to_f32_px().max(0.0) as f64;
        if width <= 0.0 || matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            return None;
        }
        Some(BorderSidePaint {
            width,
            color: resolve_color(color, current_abs),
            style,
        })
    };

    BorderPaint {
        top: make_side(
            border.clone_border_top_style(),
            border.clone_border_top_width(),
            &border.clone_border_top_color(),
        ),
        right: make_side(
            border.clone_border_right_style(),
            border.clone_border_right_width(),
            &border.clone_border_right_color(),
        ),
        bottom: make_side(
            border.clone_border_bottom_style(),
            border.clone_border_bottom_width(),
            &border.clone_border_bottom_color(),
        ),
        left: make_side(
            border.clone_border_left_style(),
            border.clone_border_left_width(),
            &border.clone_border_left_color(),
        ),
    }
}

pub(super) fn outline_paint(computed: &ComputedValues, current_abs: &AbsoluteColor) -> Option<OutlinePaint> {
    let outline = computed.get_outline();
    let width = outline.outline_width.0.to_f32_px().max(0.0) as f64;
    if outline.outline_style.none_or_hidden() || width <= 0.0 {
        return None;
    }
    let style = match outline.outline_style {
        style::values::specified::outline::OutlineStyle::Auto => BorderStyle::Solid,
        style::values::specified::outline::OutlineStyle::BorderStyle(style) => style,
    };
    Some(OutlinePaint {
        width,
        offset: outline.outline_offset.to_f32_px() as f64,
        color: resolve_color(&outline.outline_color, current_abs),
        style,
    })
}

pub(super) fn append_box_border_primitives(
    primitives: &mut Vec<MpPrimitive>,
    bounds: Rect,
    radius: MpPerCornerRadius,
    border: &BorderPaint,
    outline: Option<OutlinePaint>,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) -> Result<(), String> {
    if radius.max() > 0.0 {
        if let Some((width, color)) = uniform_rounded_border(border) {
            let mut primitive = MpPrimitive::border(
                makepad_browser_scene::MpPrimitiveId(0),
                spatial_id,
                clip_chain_id,
                bounds,
                color,
                width as f32,
                radius,
            );
            primitive.effect_id = effect_id;
            primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
            primitives.push(primitive);
        } else if has_border_paint(border) {
            return Err("rounded borders with non-uniform edges are not supported by browser-scene adapter yet".to_string());
        }

        if let Some(outline) = outline {
            if matches!(outline.style, BorderStyle::Solid) {
                let expanded = Rect {
                    pos: dvec2(
                        bounds.pos.x - outline.offset - outline.width,
                        bounds.pos.y - outline.offset - outline.width,
                    ),
                    size: dvec2(
                        bounds.size.x + 2.0 * (outline.offset + outline.width),
                        bounds.size.y + 2.0 * (outline.offset + outline.width),
                    ),
                };
                let mut primitive = MpPrimitive::border(
                    makepad_browser_scene::MpPrimitiveId(0),
                    spatial_id,
                    clip_chain_id,
                    expanded,
                    outline.color,
                    outline.width as f32,
                    radius.outset(outline.offset as f32 + outline.width as f32),
                );
                primitive.effect_id = effect_id;
                primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
                primitives.push(primitive);
            } else {
                return Err("rounded outlines with non-solid styles are not supported by browser-scene adapter yet".to_string());
            }
        }
        return Ok(());
    }

    append_border_primitives(
        primitives,
        bounds,
        border,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_outline_primitives(
        primitives,
        bounds,
        outline,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    Ok(())
}

fn has_border_paint(border: &BorderPaint) -> bool {
    border.top.is_some() || border.right.is_some() || border.bottom.is_some() || border.left.is_some()
}

fn uniform_rounded_border(border: &BorderPaint) -> Option<(f64, Vec4f)> {
    let top = border.top?;
    let right = border.right?;
    let bottom = border.bottom?;
    let left = border.left?;
    if top.style != BorderStyle::Solid
        || right.style != BorderStyle::Solid
        || bottom.style != BorderStyle::Solid
        || left.style != BorderStyle::Solid
    {
        return None;
    }
    if top.width != right.width
        || top.width != bottom.width
        || top.width != left.width
        || top.color != right.color
        || top.color != bottom.color
        || top.color != left.color
    {
        return None;
    }
    Some((top.width, top.color))
}

fn append_border_primitives(
    primitives: &mut Vec<MpPrimitive>,
    bounds: Rect,
    border: &BorderPaint,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) {
    if let Some(side) = border.top {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Top,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
    if let Some(side) = border.right {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Right,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
    if let Some(side) = border.bottom {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Bottom,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
    if let Some(side) = border.left {
        append_border_side_primitives(
            primitives,
            bounds,
            side,
            BorderSide::Left,
            spatial_id,
            clip_chain_id,
            effect_id,
            owner_node_id,
        );
    }
}

fn append_outline_primitives(
    primitives: &mut Vec<MpPrimitive>,
    bounds: Rect,
    outline: Option<OutlinePaint>,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) {
    let Some(outline) = outline else {
        return;
    };
    let expanded = Rect {
        pos: dvec2(
            bounds.pos.x - outline.offset - outline.width,
            bounds.pos.y - outline.offset - outline.width,
        ),
        size: dvec2(
            bounds.size.x + 2.0 * (outline.offset + outline.width),
            bounds.size.y + 2.0 * (outline.offset + outline.width),
        ),
    };
    let side = BorderSidePaint {
        width: outline.width,
        color: outline.color,
        style: outline.style,
    };
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Top,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Right,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Bottom,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
    append_border_side_primitives(
        primitives,
        expanded,
        side,
        BorderSide::Left,
        spatial_id,
        clip_chain_id,
        effect_id,
        owner_node_id,
    );
}

fn append_border_side_primitives(
    primitives: &mut Vec<MpPrimitive>,
    bounds: Rect,
    side: BorderSidePaint,
    border_side: BorderSide,
    spatial_id: makepad_browser_scene::MpSpatialId,
    clip_chain_id: MpClipChainId,
    effect_id: Option<makepad_browser_scene::MpEffectId>,
    owner_node_id: Option<usize>,
) {
    let is_horizontal = matches!(border_side, BorderSide::Top | BorderSide::Bottom);
    let thickness = side.width;
    let push_rect = |primitives: &mut Vec<MpPrimitive>, rect: Rect, color| {
        if rect.size.x <= 0.0 || rect.size.y <= 0.0 {
            return;
        }
        let mut primitive = MpPrimitive::solid_rect(
            makepad_browser_scene::MpPrimitiveId(0),
            spatial_id,
            clip_chain_id,
            rect,
            color,
        );
        primitive.effect_id = effect_id;
        primitive.hit_test_tag = owner_node_id.map(|id| MpHitTestTag(id as u64));
        primitives.push(primitive);
    };
    let rect_for_offset = |offset: f64, length: f64| -> Rect {
        match border_side {
            BorderSide::Top => Rect {
                pos: dvec2(bounds.pos.x + offset, bounds.pos.y),
                size: dvec2(length, thickness),
            },
            BorderSide::Right => Rect {
                pos: dvec2(bounds.pos.x + bounds.size.x - thickness, bounds.pos.y + offset),
                size: dvec2(thickness, length),
            },
            BorderSide::Bottom => Rect {
                pos: dvec2(bounds.pos.x + offset, bounds.pos.y + bounds.size.y - thickness),
                size: dvec2(length, thickness),
            },
            BorderSide::Left => Rect {
                pos: dvec2(bounds.pos.x, bounds.pos.y + offset),
                size: dvec2(thickness, length),
            },
        }
    };
    let side_length = if is_horizontal { bounds.size.x } else { bounds.size.y };

    match side.style {
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Solid => {
            push_rect(primitives, rect_for_offset(0.0, side_length), side.color);
        }
        BorderStyle::Double => {
            let line = (thickness / 3.0).max(1.0);
            let outer = match border_side {
                BorderSide::Top => Rect {
                    pos: bounds.pos,
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Right => Rect {
                    pos: dvec2(bounds.pos.x + bounds.size.x - thickness, bounds.pos.y),
                    size: dvec2(line, bounds.size.y),
                },
                BorderSide::Bottom => Rect {
                    pos: dvec2(bounds.pos.x, bounds.pos.y + bounds.size.y - line),
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Left => Rect {
                    pos: bounds.pos,
                    size: dvec2(line, bounds.size.y),
                },
            };
            let inner = match border_side {
                BorderSide::Top => Rect {
                    pos: dvec2(bounds.pos.x, bounds.pos.y + thickness - line),
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Right => Rect {
                    pos: dvec2(bounds.pos.x + bounds.size.x - line, bounds.pos.y),
                    size: dvec2(line, bounds.size.y),
                },
                BorderSide::Bottom => Rect {
                    pos: dvec2(bounds.pos.x, bounds.pos.y + bounds.size.y - thickness),
                    size: dvec2(bounds.size.x, line),
                },
                BorderSide::Left => Rect {
                    pos: dvec2(bounds.pos.x + thickness - line, bounds.pos.y),
                    size: dvec2(line, bounds.size.y),
                },
            };
            push_rect(primitives, outer, side.color);
            push_rect(primitives, inner, side.color);
        }
        BorderStyle::Dotted | BorderStyle::Dashed => {
            let segment = if side.style == BorderStyle::Dotted {
                thickness.max(1.0)
            } else {
                (thickness * 3.0).max(1.0)
            };
            let count = (side_length / (segment * 2.0)).max(1.0) as i32;
            let spacing = side_length / count as f64;
            let drawn = segment.min(spacing * 0.5);
            for index in 0..count {
                push_rect(
                    primitives,
                    rect_for_offset(index as f64 * spacing, drawn),
                    side.color,
                );
            }
        }
        BorderStyle::Groove => {
            let half = (thickness / 2.0).max(0.5);
            let (first, second) = groove_colors(side.color, border_side);
            push_rect(primitives, rect_for_split(bounds, border_side, 0.0, half), first);
            push_rect(primitives, rect_for_split(bounds, border_side, half, thickness - half), second);
        }
        BorderStyle::Ridge => {
            let half = (thickness / 2.0).max(0.5);
            let (dark, light) = groove_colors(side.color, border_side);
            push_rect(primitives, rect_for_split(bounds, border_side, 0.0, half), light);
            push_rect(primitives, rect_for_split(bounds, border_side, half, thickness - half), dark);
        }
        BorderStyle::Inset => {
            let color = if matches!(border_side, BorderSide::Top | BorderSide::Left) {
                darken(side.color, 0.6)
            } else {
                side.color
            };
            push_rect(primitives, rect_for_offset(0.0, side_length), color);
        }
        BorderStyle::Outset => {
            let color = if matches!(border_side, BorderSide::Bottom | BorderSide::Right) {
                darken(side.color, 0.6)
            } else {
                side.color
            };
            push_rect(primitives, rect_for_offset(0.0, side_length), color);
        }
    }
}

fn rect_for_split(bounds: Rect, border_side: BorderSide, offset: f64, thickness: f64) -> Rect {
    match border_side {
        BorderSide::Top => Rect {
            pos: dvec2(bounds.pos.x, bounds.pos.y + offset),
            size: dvec2(bounds.size.x, thickness),
        },
        BorderSide::Right => Rect {
            pos: dvec2(bounds.pos.x + bounds.size.x - offset - thickness, bounds.pos.y),
            size: dvec2(thickness, bounds.size.y),
        },
        BorderSide::Bottom => Rect {
            pos: dvec2(bounds.pos.x, bounds.pos.y + bounds.size.y - offset - thickness),
            size: dvec2(bounds.size.x, thickness),
        },
        BorderSide::Left => Rect {
            pos: dvec2(bounds.pos.x + offset, bounds.pos.y),
            size: dvec2(thickness, bounds.size.y),
        },
    }
}

fn groove_colors(color: Vec4f, border_side: BorderSide) -> (Vec4f, Vec4f) {
    let dark = darken(color, 0.6);
    let light = lighten(color, 1.4);
    match border_side {
        BorderSide::Top | BorderSide::Left => (dark, light),
        BorderSide::Bottom | BorderSide::Right => (light, dark),
    }
}

fn darken(color: Vec4f, factor: f32) -> Vec4f {
    Vec4f {
        x: color.x * factor,
        y: color.y * factor,
        z: color.z * factor,
        w: color.w,
    }
}

fn lighten(color: Vec4f, factor: f32) -> Vec4f {
    Vec4f {
        x: (color.x * factor).min(1.0),
        y: (color.y * factor).min(1.0),
        z: (color.z * factor).min(1.0),
        w: color.w,
    }
}
