use havi_types::{Fragment, ImageFragment};
use makepad_widgets::makepad_draw::ImageBuffer;
use makepad_widgets::*;
use style::computed_values::visibility::T as Visibility;

use crate::DrawVideoYuv;

use crate::background::draw_element_box;
use crate::frame_tree::FramePaintItem;
use crate::makepad_builder::MakepadDrawState;
use crate::text::draw_text_run;

pub(crate) fn paint_fragment_item(
    cx: &mut Cx2d,
    item: &FramePaintItem<'_>,
    state: &mut MakepadDrawState<'_>,
    opacity: f32,
) {
    let fragment = item.source;
    if fragment.base().style.get_inherited_box().visibility != Visibility::Visible {
        return;
    }

    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => match item.section {
            crate::layout_stacking_context::StackingContextSection::OwnBackgroundsAndBorders
            | crate::layout_stacking_context::StackingContextSection::DescendantBackgroundsAndBorders
            | crate::layout_stacking_context::StackingContextSection::Foreground => {
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
                        &bf.base.style,
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
        },
        Fragment::Text(text_fragment) => {
            if item.section != crate::layout_stacking_context::StackingContextSection::Foreground {
                return;
            }
            let rect = text_fragment.base.rect;
            draw_text_run(
                cx,
                text_fragment,
                item.local_origin.x + rect.origin.x.to_f32_px() as f64,
                item.local_origin.y + rect.origin.y.to_f32_px() as f64,
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
            if item.section != crate::layout_stacking_context::StackingContextSection::Foreground {
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
            if item.section != crate::layout_stacking_context::StackingContextSection::Foreground {
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
        Fragment::Positioning(_) | Fragment::AbsoluteOrFixedPositioned { .. } => {}
    }
}

pub(crate) fn paint_selection_overlay(cx: &mut Cx2d, state: &mut MakepadDrawState<'_>) {
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
    texture_cache: &mut crate::TextureCache,
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
