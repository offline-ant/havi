/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::sync::Arc;

use app_units::Au;
use base::id::PipelineId;
use fonts_traits::FontIdentifier;
use havi_fragment_semantics as semantics;

use crate::context::ImageResolver;
use crate::fragment_tree::{BaseFragment, BoxFragment, Fragment as LayoutFragment};

pub(crate) trait ToSemanticFragmentTree {
    fn to_semantic_fragments(
        &self,
        image_resolver: &Arc<ImageResolver>,
        visited_pipelines: &mut HashSet<PipelineId>,
    ) -> Vec<semantics::Fragment>;
}

impl ToSemanticFragmentTree for [LayoutFragment] {
    fn to_semantic_fragments(
        &self,
        image_resolver: &Arc<ImageResolver>,
        visited_pipelines: &mut HashSet<PipelineId>,
    ) -> Vec<semantics::Fragment> {
        self.iter()
            .filter_map(|fragment| fragment.to_semantic_fragment(image_resolver, visited_pipelines))
            .collect()
    }
}

trait ToSemanticFragment {
    fn to_semantic_fragment(
        &self,
        image_resolver: &Arc<ImageResolver>,
        visited_pipelines: &mut HashSet<PipelineId>,
    ) -> Option<semantics::Fragment>;
}

impl ToSemanticFragment for LayoutFragment {
    fn to_semantic_fragment(
        &self,
        image_resolver: &Arc<ImageResolver>,
        visited_pipelines: &mut HashSet<PipelineId>,
    ) -> Option<semantics::Fragment> {
        match self {
            LayoutFragment::Box(arc) => {
                let fragment = arc.borrow();
                Some(semantics::Fragment::Box(fragment.to_semantic_box(image_resolver, visited_pipelines)))
            }
            LayoutFragment::Float(arc) => {
                let fragment = arc.borrow();
                Some(semantics::Fragment::Float(fragment.to_semantic_box(image_resolver, visited_pipelines)))
            }
            LayoutFragment::Positioning(arc) => {
                let fragment = arc.borrow();
                Some(semantics::Fragment::Positioning(semantics::PositioningFragment {
                    base: base_fragment_to_semantic(&fragment.base),
                    children: fragment
                        .children
                        .as_slice()
                        .to_semantic_fragments(image_resolver, visited_pipelines),
                }))
            }
            LayoutFragment::Text(arc) => {
                let fragment = arc.borrow();
                let font_size_px = fragment.font_metrics.em_size.to_f32_px();
                let glyphs: Vec<semantics::ShapedGlyph> = fragment
                    .glyphs
                    .iter()
                    .flat_map(|store| {
                        store.glyphs().map(|glyph| {
                            let offset = glyph.offset();
                            semantics::ShapedGlyph {
                                glyph_id: glyph.id(),
                                advance: glyph.advance(),
                                x_offset: offset.map_or(Au(0), |o| o.x),
                                y_offset: offset.map_or(Au(0), |o| o.y),
                                char_count: glyph.character_count() as u32,
                            }
                        })
                    })
                    .collect();
                Some(semantics::Fragment::Text(semantics::TextFragment {
                    base: base_fragment_to_semantic(&fragment.base),
                    text: fragment.text.clone(),
                    font_size_px,
                    glyphs,
                    baseline_ascent: fragment.font_metrics.ascent,
                    underline_offset: fragment.font_metrics.underline_offset,
                    underline_size: fragment.font_metrics.underline_size,
                    strikeout_offset: fragment.font_metrics.strikeout_offset,
                    strikeout_size: fragment.font_metrics.strikeout_size,
                    font_handle: font_handle_from_font(&fragment.font),
                    font_data: font_data_from_font(&fragment.font),
                }))
            }
            LayoutFragment::Image(arc) => {
                let fragment = arc.borrow();
                let image_key = fragment.image_key.map(|k| (k.0.0, k.1));
                let node = fragment.base.tag.map(|t| t.node);
                let (frame_width, frame_height, image_data, frame_byte_range) = fragment
                    .raster_image
                    .as_ref()
                    .map(|img| {
                        let active_frame = node.and_then(|n| {
                            image_resolver
                                .animating_images
                                .read()
                                .node_to_state_map
                                .get(&n)
                                .map(|s| s.active_frame)
                        });
                        let frame_idx = active_frame.unwrap_or(0);
                        let frame = img.frames.get(frame_idx).or_else(|| img.frames.first());
                        let (w, h, range) = match frame {
                            Some(frame) => (frame.width, frame.height, frame.byte_range.clone()),
                            None => (
                                img.metadata.width as u32,
                                img.metadata.height as u32,
                                0..img.bytes.len(),
                            ),
                        };
                        (w, h, img.bytes.clone(), range)
                    })
                    .unwrap_or_else(|| (0, 0, Arc::new(Vec::new()), 0..0));
                Some(semantics::Fragment::Image(semantics::ImageFragment {
                    base: base_fragment_to_semantic(&fragment.base),
                    image_key,
                    frame_width,
                    frame_height,
                    image_data,
                    frame_byte_range,
                }))
            }
            LayoutFragment::IFrame(arc) => {
                let fragment = arc.borrow();
                let (child_fragments, child_content_height) =
                    resolve_iframe_child_fragments(fragment.pipeline_id, visited_pipelines);
                Some(semantics::Fragment::IFrame(semantics::IFrameFragment {
                    base: base_fragment_to_semantic(&fragment.base),
                    child_fragments,
                    child_content_height,
                }))
            }
            LayoutFragment::AbsoluteOrFixedPositioned(arc) => {
                let shared = arc.borrow();
                let resolved = shared.fragment.as_ref()?;
                let resolved = resolved.to_semantic_fragment(image_resolver, visited_pipelines)?;
                Some(semantics::Fragment::AbsoluteOrFixedPositioned {
                    resolved: Box::new(resolved),
                })
            }
        }
    }
}

trait ToSemanticBoxFragment {
    fn to_semantic_box(
        &self,
        image_resolver: &Arc<ImageResolver>,
        visited_pipelines: &mut HashSet<PipelineId>,
    ) -> semantics::BoxFragment;
}

impl ToSemanticBoxFragment for BoxFragment {
    fn to_semantic_box(
        &self,
        image_resolver: &Arc<ImageResolver>,
        visited_pipelines: &mut HashSet<PipelineId>,
    ) -> semantics::BoxFragment {
        let block_level_info = self.block_level_layout_info.as_ref().map(|info| {
            Box::new(semantics::BlockLevelLayoutInfo {
                clearance: info.clearance,
                block_margins_collapsed_with_children: semantics::CollapsedBlockMargins {
                    collapsed_through: info.block_margins_collapsed_with_children.collapsed_through,
                    start: semantics::CollapsedMargin::new(
                        info.block_margins_collapsed_with_children.start.solve(),
                    ),
                    end: semantics::CollapsedMargin::new(
                        info.block_margins_collapsed_with_children.end.solve(),
                    ),
                },
            })
        });
        let writing_mode = self.style().writing_mode;
        let baselines = self.baselines(writing_mode);
        let node = self.base.tag.map(|t| t.node);
        let background_images = resolve_background_images(&self.base.style(), node, image_resolver);

        semantics::BoxFragment {
            base: base_fragment_to_semantic(&self.base),
            children: self
                .children
                .as_slice()
                .to_semantic_fragments(image_resolver, visited_pipelines),
            cumulative_containing_block_rect: self.cumulative_containing_block_rect,
            scrollable_overflow: if self
                .base
                .flags
                .contains(crate::fragment_tree::FragmentFlags::DO_NOT_PAINT)
            {
                None
            } else {
                Some(self.scrollable_overflow())
            },
            resolved_sticky_insets: if self.style().get_box().position
                == style::computed_values::position::T::Static
            {
                None
            } else {
                Some(self.calculate_resolved_insets_if_positioned())
            },
            padding: self.padding,
            border: self.border,
            margin: self.margin,
            baselines: semantics::Baselines {
                first: baselines.first,
                last: baselines.last,
            },
            block_level_info,
            background_images,
        }
    }
}

fn base_fragment_to_semantic(base: &BaseFragment) -> semantics::BaseFragment {
    let style_ref = base.style();
    let style = (*style_ref).clone();
    let tag = base.tag.map(|t| semantics::Tag {
        node: semantics::OpaqueNode(t.node.id()),
    });
    let mut flags = semantics::FragmentFlags::empty();
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT)
    {
        flags |= semantics::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT;
    }
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::IS_BR_ELEMENT)
    {
        flags |= semantics::FragmentFlags::IS_BR_ELEMENT;
    }
    if base.flags.contains(crate::fragment_tree::FragmentFlags::IS_WIDGET) {
        flags |= semantics::FragmentFlags::IS_WIDGET;
    }
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::IS_REPLACED)
    {
        flags |= semantics::FragmentFlags::IS_REPLACED;
    }
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::IS_ROOT_ELEMENT)
    {
        flags |= semantics::FragmentFlags::IS_ROOT_ELEMENT;
    }
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::DO_NOT_PAINT)
    {
        flags |= semantics::FragmentFlags::DO_NOT_PAINT;
    }
    semantics::BaseFragment { tag, flags, style, rect: base.rect }
}

fn resolve_iframe_child_fragments(
    pipeline_id: PipelineId,
    visited_pipelines: &mut HashSet<PipelineId>,
) -> (Arc<Vec<semantics::Fragment>>, f32) {
    if !visited_pipelines.insert(pipeline_id) {
        return (Arc::new(Vec::new()), 0.0);
    }
    let child_fragments = layout_api::shared_layout_fragment_tree_for_pipeline(pipeline_id)
        .get()
        .unwrap_or_else(|| Arc::new(Vec::new()));
    let child_content_height = layout_api::shared_scroll_state_for_pipeline(pipeline_id)
        .get()
        .content_height as f32;
    visited_pipelines.remove(&pipeline_id);
    (child_fragments, child_content_height)
}

fn resolve_background_images(
    style: &style::properties::ComputedValues,
    node: Option<style::dom::OpaqueNode>,
    image_resolver: &Arc<ImageResolver>,
) -> Vec<semantics::BackgroundImage> {
    use style::values::computed::image::Image;

    let bg = style.get_background();
    let mut images = Vec::new();
    for image in bg.background_image.0.iter() {
        match image {
            Image::Url(url_value) => {
                let Some(url) = url_value.url() else {
                    continue;
                };
                let Ok(cached) = image_resolver.get_cached_image_for_url(
                    node.unwrap_or(style::dom::OpaqueNode(0)),
                    url.clone().into(),
                    layout_api::LayoutImageDestination::DisplayListBuilding,
                ) else {
                    continue;
                };
                let Some(raster) = cached.as_raster_image() else {
                    continue;
                };
                images.push(semantics::BackgroundImage {
                    width: raster.metadata.width as u32,
                    height: raster.metadata.height as u32,
                    pixels: raster.bytes.as_ref().clone(),
                });
            }
            _ => {}
        }
    }
    images
}

#[cfg(not(target_os = "windows"))]
fn font_handle_from_font(font: &fonts::FontRef) -> Option<havi_fonts::FontHandle> {
    match font.identifier() {
        FontIdentifier::Local(ref local) => Some(havi_fonts::FontHandle {
            path: std::path::PathBuf::from(&*local.path),
            index: local.index(),
        }),
        FontIdentifier::Web(_) => None,
    }
}

#[cfg(target_os = "windows")]
fn font_handle_from_font(font: &fonts::FontRef) -> Option<havi_fonts::FontHandle> {
    match font.identifier() {
        FontIdentifier::Local(ref local) => {
            let native = local.native_font_handle();
            Some(havi_fonts::FontHandle {
                path: native.path,
                index: native.index,
            })
        }
        FontIdentifier::Web(_) => None,
    }
}

fn font_data_from_font(font: &fonts::FontRef) -> Option<havi_fonts::FontData> {
    let data_and_index = font.font_data_and_index().ok()?;
    let bytes: &[u8] = data_and_index.data.as_ref();
    Some(Arc::new(bytes.to_vec()))
}
