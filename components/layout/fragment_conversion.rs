/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Converts layout's internal fragment types to `havi_types::Fragment` for rendering.

use std::sync::Arc;

use app_units::Au;

use fonts_traits::FontIdentifier;
use crate::fragment_tree::Fragment as LayoutFragment;

/// Convert a slice of layout fragments to havi_types fragments for rendering.
pub(crate) fn convert_fragments(fragments: &[LayoutFragment]) -> Vec<havi_types::Fragment> {
    fragments
        .iter()
        .filter_map(convert_fragment)
        .collect()
}

fn convert_fragment(fragment: &LayoutFragment) -> Option<havi_types::Fragment> {
    match fragment {
        LayoutFragment::Box(arc) => {
            let f = arc.borrow();
            Some(havi_types::Fragment::Box(convert_box_fragment(&f)))
        },
        LayoutFragment::Float(arc) => {
            let f = arc.borrow();
            Some(havi_types::Fragment::Float(convert_box_fragment(&f)))
        },
        LayoutFragment::Positioning(arc) => {
            let f = arc.borrow();
            Some(havi_types::Fragment::Positioning(havi_types::PositioningFragment {
                base: convert_base_fragment(&f.base),
                children: convert_fragments(&f.children),
            }))
        },
        LayoutFragment::Text(arc) => {
            let f = arc.borrow();
            let font_size_px = f.font_metrics.em_size.to_f32_px();
            let glyphs = f
                .glyphs
                .iter()
                .flat_map(|store| {
                    store.glyphs().map(|g| {
                        let offset = g.offset();
                        havi_types::ShapedGlyph {
                            glyph_id: g.id(),
                            advance: g.advance(),
                            x_offset: offset.map_or(Au(0), |o| o.x),
                            y_offset: offset.map_or(Au(0), |o| o.y),
                        }
                    })
                })
                .collect();

            Some(havi_types::Fragment::Text(havi_types::TextFragment {
                base: convert_base_fragment(&f.base),
                text: String::new(),
                font_size_px,
                glyphs,
                baseline_ascent: f.font_metrics.ascent,
                underline_offset: f.font_metrics.underline_offset,
                underline_size: f.font_metrics.underline_size,
                strikeout_offset: f.font_metrics.strikeout_offset,
                strikeout_size: f.font_metrics.strikeout_size,
                font_handle: font_handle_from_font(&f.font),
            }))
        },
        LayoutFragment::Image(arc) => {
            let f = arc.borrow();
            Some(havi_types::Fragment::Image(havi_types::ImageFragment {
                base: convert_base_fragment(&f.base),
                image_width: 0,
                image_height: 0,
                pixels: Vec::new(),
            }))
        },
        LayoutFragment::IFrame(arc) => {
            let f = arc.borrow();
            Some(havi_types::Fragment::IFrame(havi_types::IFrameFragment {
                base: convert_base_fragment(&f.base),
                child_fragments: Arc::new(Vec::new()),
                child_content_height: 0.0,
            }))
        },
        LayoutFragment::AbsoluteOrFixedPositioned(_) => None,
    }
}

fn convert_base_fragment(
    base: &crate::fragment_tree::BaseFragment,
) -> havi_types::BaseFragment {
    let style_ref = base.style();
    let style = (*style_ref).clone();

    let tag = base.tag.map(|t| havi_types::Tag {
        node: havi_types::OpaqueNode(t.node.id()),
    });

    let mut flags = havi_types::FragmentFlags::empty();
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT)
    {
        flags |= havi_types::FragmentFlags::IS_BODY_ELEMENT_OF_HTML_ELEMENT_ROOT;
    }
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::IS_BR_ELEMENT)
    {
        flags |= havi_types::FragmentFlags::IS_BR_ELEMENT;
    }
    if base
        .flags
        .contains(crate::fragment_tree::FragmentFlags::IS_ROOT_ELEMENT)
    {
        flags |= havi_types::FragmentFlags::IS_ROOT_ELEMENT;
    }

    havi_types::BaseFragment {
        tag,
        flags,
        style,
        rect: base.rect,
    }
}

fn convert_box_fragment(
    f: &crate::fragment_tree::BoxFragment,
) -> havi_types::BoxFragment {
    let block_level_info = f.block_level_layout_info.as_ref().map(|info| {
        Box::new(havi_types::BlockLevelLayoutInfo {
            clearance: info.clearance,
            block_margins_collapsed_with_children: convert_collapsed_block_margins(
                &info.block_margins_collapsed_with_children,
            ),
        })
    });

    let writing_mode = f.style().writing_mode;
    let baselines = f.baselines(writing_mode);

    havi_types::BoxFragment {
        base: convert_base_fragment(&f.base),
        children: convert_fragments(&f.children),
        padding: f.padding,
        border: f.border,
        margin: f.margin,
        baselines: havi_types::Baselines {
            first: baselines.first,
            last: baselines.last,
        },
        block_level_info,
    }
}

fn convert_collapsed_block_margins(
    m: &crate::fragment_tree::CollapsedBlockMargins,
) -> havi_types::CollapsedBlockMargins {
    havi_types::CollapsedBlockMargins {
        collapsed_through: m.collapsed_through,
        start: havi_types::CollapsedMargin::new(m.start.solve()),
        end: havi_types::CollapsedMargin::new(m.end.solve()),
    }
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
        },
        FontIdentifier::Web(_) => None,
    }
}
