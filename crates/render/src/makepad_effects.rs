use havi_types::Fragment;
use makepad_widgets::makepad_draw::draw_list_2d::DrawList2d;
use makepad_widgets::makepad_draw::Texture;
use makepad_widgets::*;

use crate::frame_tree::{FrameId, FrameTree};
use crate::makepad_builder::MakepadDrawState;
use crate::{CssFilters, FilterPass, OpacityPass};

pub(crate) fn frame_effects_for_node(frame_tree: &FrameTree<'_>, frame_id: FrameId) -> (f32, CssFilters) {
    let frame = frame_tree.frame(frame_id);
    for item in &frame.items {
        match item.source {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                return (bf.base.style.get_effects().opacity, crate::resolve_css_filters(&bf.base.style));
            }
            Fragment::IFrame(iframe) => {
                return (iframe.base.style.get_effects().opacity, crate::resolve_css_filters(&iframe.base.style));
            }
            Fragment::AbsoluteOrFixedPositioned { .. } | Fragment::Positioning(_) | Fragment::Text(_) | Fragment::Image(_) => {}
        }
    }
    (1.0, CssFilters::identity())
}

pub(crate) fn begin_filter_pass(
    cx: &mut Cx2d,
    state: &mut MakepadDrawState<'_>,
    node_id: usize,
    size: DVec2,
    shift: DVec2,
) {
    let fp = state.filter_state.entry(node_id).or_insert_with(|| {
        let pass = DrawPass::new(cx.cx);
        let texture = Texture::new_with_format(
            cx.cx,
            TextureFormat::RenderBGRAu8 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        pass.set_color_texture(
            cx.cx,
            &texture,
            DrawPassClearColor::ClearWith(Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 }),
        );
        FilterPass {
            pass,
            texture,
            draw_list: DrawList2d::new(cx.cx),
        }
    });
    fp.pass.set_size(cx.cx, size);
    cx.make_child_pass(&fp.pass);
    cx.begin_pass(&fp.pass, None);
    cx.set_pass_shift_scale(&fp.pass, shift, dvec2(1.0, 1.0));
    fp.draw_list.begin_always(cx);
}

pub(crate) fn end_filter_pass(
    cx: &mut Cx2d,
    state: &mut MakepadDrawState<'_>,
    node_id: usize,
    bounds: Rect,
    opacity: f32,
    filters: &CssFilters,
) {
    let fp = state.filter_state.get_mut(&node_id).unwrap();
    fp.draw_list.end(cx);
    cx.end_pass(&fp.pass);

    state.draw_filter_image.draw_vars.set_texture(0, &fp.texture);
    state.draw_filter_image.opacity = opacity;
    state.draw_filter_image.blur_radius = filters.blur_radius;
    state.draw_filter_image.brightness = filters.brightness;
    state.draw_filter_image.contrast = filters.contrast;
    state.draw_filter_image.grayscale = filters.grayscale;
    state.draw_filter_image.hue_rotate = filters.hue_rotate_deg;
    state.draw_filter_image.invert = filters.invert;
    state.draw_filter_image.saturate = filters.saturate;
    state.draw_filter_image.sepia = filters.sepia;
    state.draw_filter_image.tex_size = Vec2f { x: bounds.size.x as f32, y: bounds.size.y as f32 };
    state.draw_filter_image.draw_abs(cx, bounds);
}

pub(crate) fn begin_opacity_pass(
    cx: &mut Cx2d,
    state: &mut MakepadDrawState<'_>,
    node_id: usize,
    size: DVec2,
    shift: DVec2,
) {
    let op = state.opacity_state.entry(node_id).or_insert_with(|| {
        let pass = DrawPass::new(cx.cx);
        let texture = Texture::new_with_format(
            cx.cx,
            TextureFormat::RenderBGRAu8 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        pass.set_color_texture(
            cx.cx,
            &texture,
            DrawPassClearColor::ClearWith(Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 }),
        );
        OpacityPass {
            pass,
            texture,
            draw_list: DrawList2d::new(cx.cx),
        }
    });
    op.pass.set_size(cx.cx, size);
    cx.make_child_pass(&op.pass);
    cx.begin_pass(&op.pass, None);
    cx.set_pass_shift_scale(&op.pass, shift, dvec2(1.0, 1.0));
    op.draw_list.begin_always(cx);
}

pub(crate) fn end_opacity_pass(
    cx: &mut Cx2d,
    state: &mut MakepadDrawState<'_>,
    node_id: usize,
    bounds: Rect,
    opacity: f32,
) {
    let op = state.opacity_state.get_mut(&node_id).unwrap();
    op.draw_list.end(cx);
    cx.end_pass(&op.pass);

    state.draw_image.draw_vars.set_texture(0, &op.texture);
    state.draw_image.opacity = opacity;
    state.draw_image.draw_abs(cx, bounds);
}
