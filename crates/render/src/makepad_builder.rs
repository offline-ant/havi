//! Paint a pre-built frame tree using Makepad draw lists.

use havi_types::{Fragment, ImageFragment};
use makepad_widgets::makepad_draw::draw_list_2d::{DrawList2d, DrawListExt};
use makepad_widgets::makepad_draw::{ImageBuffer, Texture};
use makepad_widgets::*;

use crate::background::draw_element_box;
use crate::clip_tree::{ClipId, ClipTree};
use crate::frame_tree::{FrameId, FramePaintCommand, FrameTree};
use crate::text::draw_text_run;
use crate::{CssFilters, DrawBoxShadow, DrawFilterImage, DrawGradient, DrawRoundedColor, DrawVideoYuv};
use crate::{FilterPass, FilterState, FrameDrawList, FrameDrawListState, OpacityPass, OpacityState, SelectionHighlight, TextureCache};

pub(crate) struct MakepadDrawState<'a> {
    pub draw_bg: &'a mut DrawColor,
    pub draw_text: &'a mut DrawText,
    pub draw_text_bold: &'a mut DrawText,
    pub draw_text_mono: &'a mut DrawText,
    pub draw_image: &'a mut DrawImage,
    pub texture_cache: &'a mut TextureCache,
    pub draw_rounded_bg: &'a mut DrawRoundedColor,
    pub draw_box_shadow: &'a mut DrawBoxShadow,
    pub draw_gradient: &'a mut DrawGradient,
    pub draw_video_yuv: &'a mut DrawVideoYuv,
    pub selection: Option<&'a SelectionHighlight>,
    pub opacity_state: &'a mut OpacityState,
    pub filter_state: &'a mut FilterState,
    pub draw_filter_image: &'a mut DrawFilterImage,
    pub frame_draw_lists: &'a mut FrameDrawListState,
    pub image_overrides: &'a havi_types::ImageOverrides,
}

pub(crate) fn paint_scene(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    paint_frame(cx, frame_tree, clip_tree, frame_tree.root, state, parent_opacity);
    paint_selection_overlay(cx, state);
}

fn paint_frame(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    if frame_id == frame_tree.root {
        paint_frame_with_effects(cx, frame_tree, clip_tree, frame_id, state, parent_opacity);
        return;
    }

    let frame = frame_tree.frame(frame_id);
    state
        .frame_draw_lists
        .entry(frame.key)
        .or_insert_with(|| FrameDrawList {
            draw_list: DrawList2d::new(cx.cx),
        })
        .draw_list
        .begin_always(cx);
    state
        .frame_draw_lists
        .get_mut(&frame.key)
        .unwrap()
        .draw_list
        .set_view_transform_self_only(cx.cx, &frame.matrix.world);
    paint_frame_with_effects(cx, frame_tree, clip_tree, frame_id, state, parent_opacity);
    state
        .frame_draw_lists
        .get_mut(&frame.key)
        .unwrap()
        .draw_list
        .end(cx);
}

fn paint_frame_with_effects(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    state: &mut MakepadDrawState<'_>,
    parent_opacity: f32,
) {
    let (element_opacity, css_filters) = frame_effects_for_node(frame_tree, frame_id);
    let needs_filter = !css_filters.is_identity();
    let needs_opacity = element_opacity < 1.0 && !needs_filter;
    if (needs_filter || needs_opacity) && frame_id != frame_tree.root {
        if let Some((node_id, bounds)) = frame_owner_bounds(frame_tree, frame_id) {
            let pw = bounds.size.x.max(1.0);
            let ph = bounds.size.y.max(1.0);
            if needs_filter {
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
                fp.pass.set_size(cx.cx, dvec2(pw, ph));
                cx.make_child_pass(&fp.pass);
                cx.begin_pass(&fp.pass, None);
                cx.set_pass_shift_scale(&fp.pass, bounds.pos, dvec2(1.0, 1.0));
                fp.draw_list.begin_always(cx);
                paint_frame_contents(cx, frame_tree, clip_tree, frame_id, state, 1.0);
                let fp = state.filter_state.get_mut(&node_id).unwrap();
                fp.draw_list.end(cx);
                cx.end_pass(&fp.pass);

                let combined = parent_opacity * element_opacity * css_filters.filter_opacity;
                state.draw_filter_image.draw_vars.set_texture(0, &fp.texture);
                state.draw_filter_image.opacity = combined;
                state.draw_filter_image.blur_radius = css_filters.blur_radius;
                state.draw_filter_image.brightness = css_filters.brightness;
                state.draw_filter_image.contrast = css_filters.contrast;
                state.draw_filter_image.grayscale = css_filters.grayscale;
                state.draw_filter_image.hue_rotate = css_filters.hue_rotate_deg;
                state.draw_filter_image.invert = css_filters.invert;
                state.draw_filter_image.saturate = css_filters.saturate;
                state.draw_filter_image.sepia = css_filters.sepia;
                state.draw_filter_image.tex_size = Vec2f { x: pw as f32, y: ph as f32 };
                state.draw_filter_image.draw_abs(cx, bounds);
                return;
            }

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
            op.pass.set_size(cx.cx, dvec2(pw, ph));
            cx.make_child_pass(&op.pass);
            cx.begin_pass(&op.pass, None);
            cx.set_pass_shift_scale(&op.pass, bounds.pos, dvec2(1.0, 1.0));
            op.draw_list.begin_always(cx);
            paint_frame_contents(cx, frame_tree, clip_tree, frame_id, state, 1.0);
            let op = state.opacity_state.get_mut(&node_id).unwrap();
            op.draw_list.end(cx);
            cx.end_pass(&op.pass);

            state.draw_image.draw_vars.set_texture(0, &op.texture);
            state.draw_image.opacity = parent_opacity * element_opacity;
            state.draw_image.draw_abs(cx, bounds);
            return;
        }
    }

    paint_frame_contents(cx, frame_tree, clip_tree, frame_id, state, parent_opacity * element_opacity);
}

fn paint_frame_contents(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    state: &mut MakepadDrawState<'_>,
    opacity: f32,
) {
    let paint_list = frame_tree.frame(frame_id).paint_list.clone();
    for command in paint_list {
        match command {
            FramePaintCommand::Item(item_index) => {
                let item = &frame_tree.frame(frame_id).items[item_index];
                let pushed = push_local_clip_chain(cx, clip_tree, frame_id, item.clip_id);
                paint_fragment_item(cx, item, state, opacity);
                pop_clip_chain(cx, pushed);
            }
            FramePaintCommand::ChildFrame(child_frame_id) => {
                let pushed = push_clip_chain(
                    cx,
                    frame_tree,
                    clip_tree,
                    frame_id,
                    frame_tree.frame(child_frame_id).clip_id,
                );
                paint_frame(cx, frame_tree, clip_tree, child_frame_id, state, opacity);
                pop_clip_chain(cx, pushed);
            }
        }
    }
}

fn paint_fragment_item(
    cx: &mut Cx2d,
    item: &crate::frame_tree::FramePaintItem<'_>,
    state: &mut MakepadDrawState<'_>,
    opacity: f32,
) {
    match item.fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => match item.section {
            crate::stacking_context::StackingContextSection::OwnBackgroundsAndBorders
            | crate::stacking_context::StackingContextSection::DescendantBackgroundsAndBorders
            | crate::stacking_context::StackingContextSection::Foreground => {
                let border_rect = bf.border_rect();
                let bx = item.local_origin.x + border_rect.origin.x.to_f32_px() as f64;
                let by = item.local_origin.y + border_rect.origin.y.to_f32_px() as f64;
                let bw = border_rect.size.width.to_f32_px();
                let bh = border_rect.size.height.to_f32_px();
                draw_element_box(
                    cx,
                    &bf.base.style,
                    bx,
                    by,
                    bw,
                    bh,
                    state.draw_bg,
                    state.draw_rounded_bg,
                    state.draw_box_shadow,
                    state.draw_gradient,
                    opacity,
                );
                if !bf.background_images.is_empty() {
                    crate::background::draw_background_url_images(
                        cx,
                        &bf.background_images,
                        bx,
                        by,
                        bw,
                        bh,
                        state.draw_image,
                        state.texture_cache,
                        opacity,
                    );
                }
            }
            crate::stacking_context::StackingContextSection::Outline => {}
        },
        Fragment::Text(text_fragment) => {
            if item.section != crate::stacking_context::StackingContextSection::Foreground {
                return;
            }
            let rect = text_fragment.base.rect;
            let x = item.local_origin.x + rect.origin.x.to_f32_px() as f64;
            let y = item.local_origin.y + rect.origin.y.to_f32_px() as f64;
            draw_text_run(
                cx,
                text_fragment,
                x,
                y,
                rect.size.width.to_f32_px(),
                rect.size.height.to_f32_px(),
                opacity,
                state.draw_bg,
                state.draw_text,
                state.draw_text_bold,
                state.draw_text_mono,
            );
        }
        Fragment::Image(img) => {
            if item.section != crate::stacking_context::StackingContextSection::Foreground {
                return;
            }
            let rect = img.base.rect;
            draw_image_fragment(
                cx,
                img,
                item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                rect.size.width.to_f32_px(),
                rect.size.height.to_f32_px(),
                state.draw_image,
                state.draw_video_yuv,
                state.texture_cache,
                state.image_overrides,
                opacity,
            );
        }
        Fragment::IFrame(iframe) => {
            if item.section != crate::stacking_context::StackingContextSection::Foreground {
                return;
            }
            let rect = iframe.base.rect;
            draw_element_box(
                cx,
                &iframe.base.style,
                item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                rect.size.width.to_f32_px(),
                rect.size.height.to_f32_px(),
                state.draw_bg,
                state.draw_rounded_bg,
                state.draw_box_shadow,
                state.draw_gradient,
                opacity,
            );
        }
        Fragment::Positioning(_) => {}
    }
}

fn push_clip_chain(
    cx: &mut Cx2d,
    frame_tree: &FrameTree<'_>,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    clip_id: ClipId,
) -> usize {
    if clip_id == ClipId::INVALID {
        return 0;
    }
    let mut chain = Vec::new();
    let mut current = clip_id;
    while current != ClipId::INVALID {
        let node = clip_tree.get(current);
        chain.push(map_rect_between_frames(
            frame_tree,
            node.parent_frame_id,
            frame_id,
            node.rect,
        ));
        current = node.parent_clip_id;
    }
    chain.reverse();
    for rect in &chain {
        cx.push_clip_rect(*rect);
    }
    chain.len()
}

fn push_local_clip_chain(
    cx: &mut Cx2d,
    clip_tree: &ClipTree,
    frame_id: FrameId,
    clip_id: ClipId,
) -> usize {
    if clip_id == ClipId::INVALID {
        return 0;
    }
    let mut chain = Vec::new();
    let mut current = clip_id;
    while current != ClipId::INVALID {
        let node = clip_tree.get(current);
        if node.parent_frame_id != frame_id {
            break;
        }
        chain.push(node.rect);
        current = node.parent_clip_id;
    }
    chain.reverse();
    for rect in &chain {
        cx.push_clip_rect(*rect);
    }
    chain.len()
}

fn pop_clip_chain(cx: &mut Cx2d, pushed_count: usize) {
    for _ in 0..pushed_count {
        cx.pop_clip_rect();
    }
}

fn frame_effects_for_node(frame_tree: &FrameTree<'_>, frame_id: FrameId) -> (f32, CssFilters) {
    let frame = frame_tree.frame(frame_id);
    for item in &frame.items {
        match item.fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                return (bf.base.style.get_effects().opacity, crate::resolve_css_filters(&bf.base.style));
            }
            Fragment::IFrame(iframe) => {
                return (iframe.base.style.get_effects().opacity, crate::resolve_css_filters(&iframe.base.style));
            }
            _ => {}
        }
    }
    (1.0, CssFilters::identity())
}

fn frame_owner_bounds(frame_tree: &FrameTree<'_>, frame_id: FrameId) -> Option<(usize, Rect)> {
    let frame = frame_tree.frame(frame_id);
    let owner_node_id = frame.owner_node_id?;
    for item in &frame.items {
        match item.fragment {
            Fragment::Box(bf) | Fragment::Float(bf) => {
                let br = bf.border_rect();
                let local = Rect {
                    pos: dvec2(
                        item.local_origin.x + br.origin.x.to_f32_px() as f64,
                        item.local_origin.y + br.origin.y.to_f32_px() as f64,
                    ),
                    size: dvec2(br.size.width.to_f32_px() as f64, br.size.height.to_f32_px() as f64),
                };
                return Some((owner_node_id, transform_rect(&frame.matrix.world, local)));
            }
            Fragment::IFrame(iframe) => {
                let rect = iframe.base.rect;
                let local = Rect {
                    pos: dvec2(
                        item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                        item.local_origin.y + rect.origin.y.to_f32_px() as f64,
                    ),
                    size: dvec2(rect.size.width.to_f32_px() as f64, rect.size.height.to_f32_px() as f64),
                };
                return Some((owner_node_id, transform_rect(&frame.matrix.world, local)));
            }
            _ => {}
        }
    }
    None
}

fn transform_rect(matrix: &Mat4f, rect: Rect) -> Rect {
    let points = [
        dvec2(rect.pos.x, rect.pos.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y),
        dvec2(rect.pos.x, rect.pos.y + rect.size.y),
        dvec2(rect.pos.x + rect.size.x, rect.pos.y + rect.size.y),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in points {
        let mapped = matrix.transform_vec4(vec4f(point.x as f32, point.y as f32, 0.0, 1.0));
        let x = if mapped.w.abs() > 1e-6 { mapped.x / mapped.w } else { mapped.x } as f64;
        let y = if mapped.w.abs() > 1e-6 { mapped.y / mapped.w } else { mapped.y } as f64;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    Rect {
        pos: dvec2(min_x, min_y),
        size: dvec2((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
    }
}

fn map_rect_between_frames(
    frame_tree: &FrameTree<'_>,
    from_frame_id: FrameId,
    to_frame_id: FrameId,
    rect: Rect,
) -> Rect {
    if from_frame_id == to_frame_id {
        return rect;
    }
    let world_rect = transform_rect(&frame_tree.frame(from_frame_id).matrix.world, rect);
    transform_rect(&frame_tree.frame(to_frame_id).matrix.world_inverse, world_rect)
}

fn paint_selection_overlay(cx: &mut Cx2d, state: &mut MakepadDrawState<'_>) {
    let Some(selection) = state.selection else {
        return;
    };
    state.draw_bg.color = selection.color;
    for rect in &selection.rects {
        if rect.size.x > 0.0 && rect.size.y > 0.0 {
            state.draw_bg.draw_abs(cx, *rect);
        }
    }
}

/// Cheap hash of a byte range identity (pointer + offset + len) to detect frame changes
/// without comparing pixel data.
fn frame_identity_hash(image_data: &[u8], range: &std::ops::Range<usize>) -> u64 {
    let ptr = image_data.as_ptr() as u64;
    ptr.wrapping_mul(0x517cc1b727220a95)
        .wrapping_add(range.start as u64)
        .wrapping_mul(0x6c62272e07bb0142)
        .wrapping_add(range.end as u64)
}

fn draw_image_fragment(
    cx: &mut Cx2d,
    img: &ImageFragment,
    x: f64,
    y: f64,
    w: f32,
    h: f32,
    draw_image: &mut DrawImage,
    draw_video_yuv: &mut DrawVideoYuv,
    texture_cache: &mut TextureCache,
    image_overrides: &havi_types::ImageOverrides,
    opacity: f32,
) {
    let node_id = img.base.tag.map(|t| t.node.0).unwrap_or(0);

    if let Some(key) = img.image_key {
        if let Some(binding) = crate::video_texture_map::get_video_binding(key) {
            if binding.yuv_metadata.enabled {
                if let Some(planes) = binding.yuv_planes.as_ref() {
                    draw_video_yuv.draw_vars.set_texture(0, &planes.tex_y);
                    draw_video_yuv.draw_vars.set_texture(1, &planes.tex_u);
                    draw_video_yuv.draw_vars.set_texture(2, &planes.tex_v);
                    draw_video_yuv.yuv_type = binding.yuv_metadata.matrix;
                    draw_video_yuv.yuv_biplanar = binding.yuv_metadata.shader_biplanar();
                    draw_video_yuv.yuv_rotation_steps = binding.yuv_metadata.rotation_steps;
                    draw_video_yuv.opacity = opacity;
                    draw_video_yuv.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
                    return;
                }
            }

            if let Some(video_tex) = binding.external_texture {
                draw_image.draw_vars.set_texture(0, &video_tex);
                draw_image.opacity = opacity;
                draw_image.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
                return;
            }
        }
    }

    let (image_data, frame_start, frame_end, width, height) =
        if let Some(ov) = img.image_key.and_then(|k| image_overrides.get(&k)) {
            let frame_size = (ov.width as usize) * (ov.height as usize) * 4;
            let start = ov.offset;
            let end = (start + frame_size).min(ov.data.len());
            (&*ov.data as &[u8], start, end, ov.width as usize, ov.height as usize)
        } else {
            (
                &*img.image_data as &[u8],
                img.frame_byte_range.start,
                img.frame_byte_range.end,
                img.frame_width as usize,
                img.frame_height as usize,
            )
        };

    if frame_start >= frame_end || width == 0 || height == 0 {
        return;
    }
    let range = frame_start..frame_end;
    let hash = frame_identity_hash(image_data, &range);
    let frame_bytes = &image_data[range];

    let entry = texture_cache.entry(node_id).or_insert_with(|| {
        let data = rgba_to_bgra_u32(frame_bytes);
        crate::TextureCacheEntry {
            texture: ImageBuffer {
                width,
                height,
                data,
                animation: None,
            }
            .into_new_texture(cx.cx),
            data_hash: hash,
        }
    });

    if entry.data_hash != hash {
        entry.data_hash = hash;
        let data = rgba_to_bgra_u32(frame_bytes);
        entry.texture.set_data_u32(cx.cx, width, height, data);
    }

    draw_image.draw_vars.set_texture(0, &entry.texture);
    draw_image.opacity = opacity;
    draw_image.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
}

fn rgba_to_bgra_u32(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4)
        .map(|px| {
            (px[2] as u32) | ((px[1] as u32) << 8) | ((px[0] as u32) << 16) | ((px[3] as u32) << 24)
        })
        .collect()
}
