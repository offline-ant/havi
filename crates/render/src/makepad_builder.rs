//! Walk a stacking context tree and emit Makepad draw calls.
//!
//! Replaces the old `render_fragment()` recursive walk with a stacking-context-ordered
//! traversal that follows CSS 2.1 Appendix E paint ordering.

use havi_types::{Fragment, ImageFragment};
use makepad_widgets::*;
use makepad_widgets::makepad_draw::{ImageBuffer, Texture};
use makepad_widgets::makepad_draw::draw_list_2d::{DrawList2d, DrawListExt};

use crate::{
    DrawBoxShadow, DrawFilterImage, DrawGradient, DrawRoundedColor,
    FilterPass, FilterState, OpacityPass, OpacityState,
    ScrollState, ScrollDrawListState, SelectionHighlight, TextureCache, TransformState,
};
use crate::background::draw_element_box;
use crate::stacking_context::{
    PaintItem, StackingContext, StackingContextContent, StackingContextSection,
};
use crate::text::draw_text_run;
use crate::transform::{compute_css_transform_2d, compute_css_transform_3d, is_3d_matrix};
use crate::{resolve_css_filters, CssFilters, compute_sticky_offset, is_scroll_container};

/// All the Makepad draw state needed for rendering.
pub(crate) struct MakepadDrawState<'a> {
    pub draw_bg: &'a mut DrawColor,
    pub draw_text: &'a mut DrawText,
    pub draw_text_bold: &'a mut DrawText,
    pub draw_text_mono: &'a mut DrawText,
    pub draw_image: &'a mut DrawImage,
    pub texture_cache: &'a mut TextureCache,
    #[allow(dead_code)]
    pub scroll_state: &'a ScrollState,
    pub draw_rounded_bg: &'a mut DrawRoundedColor,
    pub draw_box_shadow: &'a mut DrawBoxShadow,
    pub draw_gradient: &'a mut DrawGradient,
    pub selection: Option<&'a SelectionHighlight>,
    pub transform_state: &'a mut TransformState,
    pub opacity_state: &'a mut OpacityState,
    pub filter_state: &'a mut FilterState,
    pub draw_filter_image: &'a mut DrawFilterImage,
    pub scroll_draw_lists: &'a mut ScrollDrawListState,
}

/// Walk a stacking context tree and paint all fragments.
pub(crate) fn paint_stacking_context(
    cx: &mut Cx2d,
    sc: &StackingContext<'_>,
    origin: DVec2,
    clip: Option<(f32, f32)>,
    parent_opacity: f32,
    state: &mut MakepadDrawState<'_>,
) {
    // Determine if this stacking context needs opacity isolation or filter pass.
    let (element_opacity, css_filters) = match sc.initializing_fragment {
        Some(bf) => (bf.base.style.get_effects().opacity, resolve_css_filters(&bf.base.style)),
        None => (1.0, CssFilters::identity()),
    };
    let needs_filter = !css_filters.is_identity();
    // Optimization: skip opacity isolation for leaf stacking contexts (no child
    // SCs). A leaf has no overlapping child layers, so alpha can be applied
    // per-instance without double-blending artifacts.
    let needs_opacity = element_opacity < 1.0 && !needs_filter && !sc.is_leaf();
    let inner_opacity = if needs_opacity || needs_filter { 1.0 } else { parent_opacity * element_opacity };

    let node_id = sc.initializing_fragment.and_then(|bf| bf.base.tag.map(|t| t.node.0));

    // Filter pass: render to texture, composite with filter shader.
    if needs_filter {
        if let Some(node_id) = node_id {
            let bf = sc.initializing_fragment.unwrap();
            let (bx, by, bw, bh) = sc_border_box(bf, origin);
            let pw = bw.max(1.0);
            let ph = bh.max(1.0);

            let fp = state.filter_state.entry(node_id).or_insert_with(|| {
                let pass = DrawPass::new(cx.cx);
                let texture = Texture::new_with_format(cx.cx, TextureFormat::RenderBGRAu8 {
                    size: TextureSize::Auto, initial: true,
                });
                pass.set_color_texture(cx.cx, &texture, DrawPassClearColor::ClearWith(
                    Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 },
                ));
                FilterPass { pass, texture, draw_list: DrawList2d::new(cx.cx) }
            });
            fp.pass.set_size(cx.cx, dvec2(pw, ph));
            cx.make_child_pass(&fp.pass);
            cx.begin_pass(&fp.pass, None);
            cx.set_pass_shift_scale(&fp.pass, dvec2(bx, by), dvec2(1.0, 1.0));
            fp.draw_list.begin_always(cx);

            paint_sc_contents(cx, sc, origin, clip, 1.0, state);

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
            state.draw_filter_image.draw_abs(cx, Rect {
                pos: dvec2(bx, by), size: dvec2(pw, ph),
            });
            return;
        }
    }

    // Opacity isolation: render to texture, composite with opacity.
    if needs_opacity {
        if let Some(node_id) = node_id {
            let bf = sc.initializing_fragment.unwrap();
            let (bx, by, bw, bh) = sc_border_box(bf, origin);
            let pw = bw.max(1.0);
            let ph = bh.max(1.0);

            let op = state.opacity_state.entry(node_id).or_insert_with(|| {
                let pass = DrawPass::new(cx.cx);
                let texture = Texture::new_with_format(cx.cx, TextureFormat::RenderBGRAu8 {
                    size: TextureSize::Auto, initial: true,
                });
                pass.set_color_texture(cx.cx, &texture, DrawPassClearColor::ClearWith(
                    Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.0 },
                ));
                OpacityPass { pass, texture, draw_list: DrawList2d::new(cx.cx) }
            });
            op.pass.set_size(cx.cx, dvec2(pw, ph));
            cx.make_child_pass(&op.pass);
            cx.begin_pass(&op.pass, None);
            cx.set_pass_shift_scale(&op.pass, dvec2(bx, by), dvec2(1.0, 1.0));
            op.draw_list.begin_always(cx);

            paint_sc_contents(cx, sc, origin, clip, 1.0, state);

            let op = state.opacity_state.get_mut(&node_id).unwrap();
            op.draw_list.end(cx);
            cx.end_pass(&op.pass);

            let combined = parent_opacity * element_opacity;
            state.draw_image.draw_vars.set_texture(0, &op.texture);
            state.draw_image.opacity = combined;
            state.draw_image.draw_abs(cx, Rect {
                pos: dvec2(bx, by), size: dvec2(pw, ph),
            });
            return;
        }
    }

    // Scroll container handling: render children into a cached DrawList2d and
    // apply scroll offset via view transform instead of re-emitting all draw
    // calls on every scroll event.
    let is_scroll = sc.initializing_fragment.map_or(false, is_scroll_container);
    if is_scroll {
        let bf = sc.initializing_fragment.unwrap();
        let node_id = bf.base.tag.map(|t| t.node.0);

        // Look up scroll offset for this container.
        let scroll_offset = node_id
            .and_then(|nid| state.scroll_state.get(&nid))
            .copied()
            .unwrap_or(DVec2 { x: 0.0, y: 0.0 });

        // Paint own backgrounds/borders at original origin (unscrolled, unclipped).
        paint_sc_own_backgrounds(cx, sc, origin, clip, inner_opacity, state);

        // Compute clip rect (padding box in screen space) for children.
        let padding_rect = bf.padding_rect();
        let own_cb_origin = sc.contents.iter().find_map(|c| {
            if let StackingContextContent::Fragment { containing_block_origin, section, .. } = c {
                if *section == StackingContextSection::OwnBackgroundsAndBorders {
                    return Some(*containing_block_origin);
                }
            }
            None
        }).unwrap_or((0.0, 0.0));

        let px = origin.x + own_cb_origin.0 + padding_rect.origin.x.to_f32_px() as f64;
        let py = origin.y + own_cb_origin.1 + padding_rect.origin.y.to_f32_px() as f64;
        let pw = padding_rect.size.width.to_f32_px() as f64;
        let ph = padding_rect.size.height.to_f32_px() as f64;
        let clip_rect = Rect { pos: dvec2(px, py), size: dvec2(pw, ph) };

        cx.push_clip_rect(clip_rect);

        if let Some(nid) = node_id {
            // Use cached DrawList2d for scroll container children.
            // Content is only re-rendered when the fragment tree changes
            // (detected by frag_ptr). Scroll-only changes reuse the existing
            // draw list and update the view transform — O(1) instead of O(n).
            let frag_ptr = sc.contents.as_ptr() as usize;
            let sdl = state.scroll_draw_lists.entry(nid).or_insert_with(|| {
                crate::ScrollDrawList {
                    draw_list: DrawList2d::new(cx.cx),
                    frag_ptr: 0,
                    last_offset: dvec2(0.0, 0.0),
                }
            });

            let content_changed = sdl.frag_ptr != frag_ptr;
            if content_changed {
                // Content changed — clear and re-render children.
                sdl.frag_ptr = frag_ptr;
                sdl.draw_list.begin_always(cx);
                paint_sc_children_only(cx, sc, origin, clip, inner_opacity, state);
                let sdl = state.scroll_draw_lists.get_mut(&nid).unwrap();
                sdl.draw_list.end(cx);
            } else {
                // Content unchanged — begin_maybe with will_redraw=false
                // preserves existing draw items without re-emitting them.
                let _ = sdl.draw_list.begin_maybe(cx, false);
                let sdl = state.scroll_draw_lists.get_mut(&nid).unwrap();
                sdl.draw_list.end(cx);
            }

            // Apply scroll offset via view transform (translation matrix).
            let sdl = state.scroll_draw_lists.get_mut(&nid).unwrap();
            sdl.last_offset = scroll_offset;
            let mat = Mat4f { v: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                -(scroll_offset.x as f32), -(scroll_offset.y as f32), 0.0, 1.0,
            ] };
            sdl.draw_list.set_view_transform(cx.cx, &mat);
        } else {
            // No node id — fall back to full repaint with offset.
            let scrolled_origin = dvec2(origin.x - scroll_offset.x, origin.y - scroll_offset.y);
            paint_sc_children_only(cx, sc, scrolled_origin, clip, inner_opacity, state);
        }

        cx.pop_clip_rect();
    } else {
        paint_sc_contents(cx, sc, origin, clip, inner_opacity, state);
    }
}

/// Paint the contents of a stacking context in CSS paint order.
fn paint_sc_contents(
    cx: &mut Cx2d,
    sc: &StackingContext<'_>,
    origin: DVec2,
    clip: Option<(f32, f32)>,
    opacity: f32,
    state: &mut MakepadDrawState<'_>,
) {
    sc.paint_in_order(&mut |item| {
        match item {
            PaintItem::Content(content) => {
                paint_content(cx, content, origin, clip, opacity, state);
            }
            PaintItem::ChildStackingContext(child) => {
                paint_stacking_context(cx, child, origin, clip, opacity, state);
            }
            PaintItem::Outline(content) => {
                // TODO: outline drawing. Currently outlines are drawn as part of
                // draw_element_box — we would need to split that. For now, skip.
                let _ = content;
            }
        }
    });
}

/// Paint only OwnBackgroundsAndBorders for a stacking context (used for scroll containers).
fn paint_sc_own_backgrounds(
    cx: &mut Cx2d,
    sc: &StackingContext<'_>,
    origin: DVec2,
    clip: Option<(f32, f32)>,
    opacity: f32,
    state: &mut MakepadDrawState<'_>,
) {
    sc.paint_in_order(&mut |item| {
        match item {
            PaintItem::Content(content) => {
                if let StackingContextContent::Fragment { section, .. } = content {
                    if *section == StackingContextSection::OwnBackgroundsAndBorders {
                        paint_content(cx, content, origin, clip, opacity, state);
                    }
                }
            }
            _ => {}
        }
    });
}

/// Paint everything except OwnBackgroundsAndBorders for a stacking context.
fn paint_sc_children_only(
    cx: &mut Cx2d,
    sc: &StackingContext<'_>,
    origin: DVec2,
    clip: Option<(f32, f32)>,
    opacity: f32,
    state: &mut MakepadDrawState<'_>,
) {
    sc.paint_in_order(&mut |item| {
        match item {
            PaintItem::Content(content) => {
                if let StackingContextContent::Fragment { section, .. } = content {
                    if *section != StackingContextSection::OwnBackgroundsAndBorders {
                        paint_content(cx, content, origin, clip, opacity, state);
                    }
                } else {
                    paint_content(cx, content, origin, clip, opacity, state);
                }
            }
            PaintItem::ChildStackingContext(child) => {
                paint_stacking_context(cx, child, origin, clip, opacity, state);
            }
            PaintItem::Outline(content) => {
                let _ = content;
            }
        }
    });
}

/// Check if a fragment's Y extent is entirely outside the viewport.
fn is_culled(fragment: &Fragment, cb_origin: (f64, f64), clip: Option<(f32, f32)>) -> bool {
    let Some((vp_top, vp_bottom)) = clip else {
        return false;
    };
    let (frag_top, frag_bottom) = match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            let br = bf.border_rect();
            let top = cb_origin.1 + br.origin.y.to_f32_px() as f64;
            let bottom = top + br.size.height.to_f32_px() as f64;
            (top, bottom)
        }
        Fragment::Text(tf) => {
            let r = tf.base.rect;
            let top = cb_origin.1 + r.origin.y.to_f32_px() as f64;
            let bottom = top + r.size.height.to_f32_px() as f64;
            (top, bottom)
        }
        Fragment::Image(img) => {
            let r = img.base.rect;
            let top = cb_origin.1 + r.origin.y.to_f32_px() as f64;
            let bottom = top + r.size.height.to_f32_px() as f64;
            (top, bottom)
        }
        _ => return false,
    };
    frag_bottom < vp_top as f64 || frag_top > vp_bottom as f64
}

/// Paint a single content item.
fn paint_content(
    cx: &mut Cx2d,
    content: &StackingContextContent<'_>,
    origin: DVec2,
    clip: Option<(f32, f32)>,
    opacity: f32,
    state: &mut MakepadDrawState<'_>,
) {
    let (section, fragment, cb_origin) = match content {
        StackingContextContent::Fragment {
            section,
            fragment,
            containing_block_origin,
        } => (*section, *fragment, *containing_block_origin),
        StackingContextContent::AtomicInlineStackingContainer { .. } => {
            // Handled by paint_in_order emitting ChildStackingContext.
            return;
        }
    };

    // Viewport culling: skip fragments entirely outside the visible area.
    if is_culled(fragment, cb_origin, clip) {
        return;
    }

    let draw_origin = dvec2(origin.x + cb_origin.0, origin.y + cb_origin.1);

    match fragment {
        Fragment::Box(bf) | Fragment::Float(bf) => {
            // Transform.
            let (tx, ty, view_mat) = {
                let br = bf.border_rect();
                let bw = br.size.width.to_f32_px();
                let bh = br.size.height.to_f32_px();
                let t2d = compute_css_transform_2d(&bf.base.style, bw, bh);
                let t3d = compute_css_transform_3d(&bf.base.style, bw, bh);
                if t3d.as_ref().map_or(false, is_3d_matrix) {
                    let mat = t3d.unwrap();
                    (mat[12], mat[13], Some(mat))
                } else {
                    match t2d {
                        Some(t) if !t.is_translate_only() => (t.tx, t.ty, Some(t.to_mat4f())),
                        Some(t) => (t.tx, t.ty, None),
                        None => (0.0, 0.0, None),
                    }
                }
            };

            let has_transform = view_mat.is_some();
            let node_id = bf.base.tag.map(|t| t.node.0);

            if has_transform {
                if let Some(nid) = node_id {
                    let dl = state.transform_state.entry(nid)
                        .or_insert_with(|| DrawList2d::new(cx.cx));
                    dl.begin_always(cx);
                }
            }

            // Sticky offset.
            let (sticky_dx, sticky_dy) = compute_sticky_offset(fragment, draw_origin, clip);

            match section {
                StackingContextSection::OwnBackgroundsAndBorders
                | StackingContextSection::DescendantBackgroundsAndBorders => {
                    // Draw backgrounds and borders.
                    let border_rect = bf.border_rect();
                    let bx = draw_origin.x + border_rect.origin.x.to_f32_px() as f64 + tx as f64 + sticky_dx;
                    let by = draw_origin.y + border_rect.origin.y.to_f32_px() as f64 + ty as f64 + sticky_dy;
                    let bw = border_rect.size.width.to_f32_px();
                    let bh = border_rect.size.height.to_f32_px();
                    draw_element_box(
                        cx, &bf.base.style, bx, by, bw, bh,
                        state.draw_bg, state.draw_rounded_bg,
                        state.draw_box_shadow, state.draw_gradient, opacity,
                    );
                    if !bf.background_images.is_empty() {
                        crate::background::draw_background_url_images(
                            cx, &bf.background_images, bx, by, bw, bh,
                            state.draw_image, state.texture_cache, opacity,
                        );
                    }

                    // Handle overflow clipping for children if this is the
                    // OwnBackgroundsAndBorders section of a stacking context.
                    // Children will be painted as separate content items, so
                    // clipping is handled at that level.
                }
                StackingContextSection::Foreground => {
                    // Box fragments in foreground section: draw backgrounds/borders
                    // (for inline boxes that appear in foreground).
                    let border_rect = bf.border_rect();
                    let bx = draw_origin.x + border_rect.origin.x.to_f32_px() as f64 + tx as f64 + sticky_dx;
                    let by = draw_origin.y + border_rect.origin.y.to_f32_px() as f64 + ty as f64 + sticky_dy;
                    let bw = border_rect.size.width.to_f32_px();
                    let bh = border_rect.size.height.to_f32_px();
                    draw_element_box(
                        cx, &bf.base.style, bx, by, bw, bh,
                        state.draw_bg, state.draw_rounded_bg,
                        state.draw_box_shadow, state.draw_gradient, opacity,
                    );
                    if !bf.background_images.is_empty() {
                        crate::background::draw_background_url_images(
                            cx, &bf.background_images, bx, by, bw, bh,
                            state.draw_image, state.texture_cache, opacity,
                        );
                    }
                }
                StackingContextSection::Outline => {
                    // TODO: draw outline only.
                }
            }

            // Close transform.
            if has_transform {
                if let Some(nid) = node_id {
                    if let Some(dl) = state.transform_state.get_mut(&nid) {
                        dl.end(cx);
                        let mat = Mat4f { v: view_mat.unwrap() };
                        dl.set_view_transform(cx.cx, &mat);
                    }
                }
            }
        }

        Fragment::Text(text_fragment) => {
            if section != StackingContextSection::Foreground {
                return;
            }
            let rect = text_fragment.base.rect;
            let x = draw_origin.x + rect.origin.x.to_f32_px() as f64;
            let y = draw_origin.y + rect.origin.y.to_f32_px() as f64;
            let w = rect.size.width.to_f32_px();
            let h = rect.size.height.to_f32_px();

            if let Some(sel) = state.selection {
                let text_rect = Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) };
                for sel_rect in &sel.rects {
                    if rects_overlap(&text_rect, sel_rect) {
                        state.draw_bg.color = sel.color;
                        state.draw_bg.draw_abs(cx, text_rect);
                        break;
                    }
                }
            }
            draw_text_run(
                cx, text_fragment, x, y, w, h, opacity,
                state.draw_bg, state.draw_text, state.draw_text_bold, state.draw_text_mono,
            );
        }

        Fragment::Image(img) => {
            if section != StackingContextSection::Foreground {
                return;
            }
            let rect = img.base.rect;
            let x = draw_origin.x + rect.origin.x.to_f32_px() as f64;
            let y = draw_origin.y + rect.origin.y.to_f32_px() as f64;
            let w = rect.size.width.to_f32_px();
            let h = rect.size.height.to_f32_px();
            draw_image_fragment(cx, img, x, y, w, h, state.draw_image, state.texture_cache, opacity);
        }

        Fragment::IFrame(iframe) => {
            if section != StackingContextSection::Foreground {
                return;
            }
            let rect = iframe.base.rect;
            let x = draw_origin.x + rect.origin.x.to_f32_px() as f64;
            let y = draw_origin.y + rect.origin.y.to_f32_px() as f64;
            let w = rect.size.width.to_f32_px();
            let h = rect.size.height.to_f32_px();
            let iframe_rect = Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) };
            cx.push_clip_rect(iframe_rect);
            // IFrame children get their own stacking context tree build.
            // For now, recurse using the simple build.
            let child_sc = crate::stacking_context::build_stacking_context_tree(&iframe.child_fragments);
            paint_stacking_context(cx, &child_sc, dvec2(x, y), clip, opacity, state);
            cx.pop_clip_rect();
        }

        Fragment::Positioning(_) => {
            // Positioning fragments are handled during tree building.
        }
    }
}

fn sc_border_box(bf: &havi_types::fragment_tree::BoxFragment, origin: DVec2) -> (f64, f64, f64, f64) {
    let br = bf.border_rect();
    (
        origin.x + br.origin.x.to_f32_px() as f64,
        origin.y + br.origin.y.to_f32_px() as f64,
        br.size.width.to_f32_px() as f64,
        br.size.height.to_f32_px() as f64,
    )
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
    cx: &mut Cx2d, img: &ImageFragment,
    x: f64, y: f64, w: f32, h: f32,
    draw_image: &mut DrawImage, texture_cache: &mut TextureCache, opacity: f32,
) {
    let node_id = img.base.tag.map(|t| t.node.0).unwrap_or(0);
    if img.frame_byte_range.is_empty() || img.frame_width == 0 || img.frame_height == 0 {
        return;
    }
    let hash = frame_identity_hash(&img.image_data, &img.frame_byte_range);
    let width = img.frame_width as usize;
    let height = img.frame_height as usize;

    let entry = texture_cache.entry(node_id).or_insert_with(|| {
        let data = rgba_to_bgra_u32(&img.image_data[img.frame_byte_range.clone()]);
        crate::TextureCacheEntry {
            texture: ImageBuffer { width, height, data, animation: None }.into_new_texture(cx.cx),
            data_hash: hash,
        }
    });

    // Re-upload only when the frame changed (different byte range or different image data).
    if entry.data_hash != hash {
        entry.data_hash = hash;
        let data = rgba_to_bgra_u32(&img.image_data[img.frame_byte_range.clone()]);
        entry.texture.set_data_u32(cx.cx, width, height, data);
    }

    draw_image.draw_vars.set_texture(0, &entry.texture);
    draw_image.opacity = opacity;
    draw_image.draw_abs(cx, Rect { pos: dvec2(x, y), size: dvec2(w as f64, h as f64) });
}

/// Convert RGBA byte slice to BGRA u32 vec for Makepad textures.
fn rgba_to_bgra_u32(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4).map(|px| {
        (px[2] as u32) | ((px[1] as u32) << 8) | ((px[0] as u32) << 16) | ((px[3] as u32) << 24)
    }).collect()
}

fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.pos.x < b.pos.x + b.size.x && a.pos.x + a.size.x > b.pos.x
        && a.pos.y < b.pos.y + b.size.y && a.pos.y + a.size.y > b.pos.y
}
