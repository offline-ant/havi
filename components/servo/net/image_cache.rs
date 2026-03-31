/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{OnceCell, RefCell};
use std::cmp::min;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::{mem, thread};

use base::id::{PipelineId, WebViewId};
use base::threadpool::ThreadPool;
use imsz::imsz_from_reader;
use crate::layout::{
    extract_svg_root_metadata, parse_svg_length, parse_svg_optional_number,
    parse_svg_transform_list, resolve_svg_length_to_user_units,
};
use log::{debug, warn};
use malloc_size_of::{MallocSizeOf as MallocSizeOfTrait, MallocSizeOfOps};
use malloc_size_of_derive::MallocSizeOf;
use mime::Mime;
use net_traits::image_cache::{
    Image, ImageCache, ImageCacheFactory, ImageCacheResult, ImageLoadListener,
    ImageOrMetadataAvailable, ImageResponse, PendingImageId, VectorImage,
};
use net_traits::request::CorsSettings;
use net_traits::{FetchMetadata, FetchResponseMsg, FilteredMetadata, NetworkError};
use crate::paint::{CrossProcessPaintApi, ImageUpdate, SerializableImageData};
use parking_lot::Mutex;
use ::pixels::{CorsStatus, ImageFrame, ImageMetadata, PixelFormat, RasterImage, load_from_memory};
use profile_traits::mem::{Report, ReportKind};
use profile_traits::path;
use rustc_hash::FxHashMap;
use ::servo_url::{ImmutableOrigin, BrowserUrl};
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{
    Affine, BezPath, Cap, Circle, Ellipse, Join, Line, Point, Rect, Shape, Stroke,
};
use vello_cpu::peniko::Fill;
use vello_cpu::{Pixmap, RenderContext};
use webrender_api::ImageKey as WebRenderImageKey;
use webrender_api::units::DeviceIntSize;
use xml5ever::TokenizerResult;
use xml5ever::buffer_queue::BufferQueue;
use xml5ever::tendril::StrTendril;
use xml5ever::tokenizer::{ProcessResult, Tag, TagKind, Token, TokenSink, XmlTokenizer};

// We bake in rippy.png as a fallback, in case the embedder does not provide a broken
// image icon resource. This version is 229 bytes, so don't exchange it against
// something of higher resolution.
const FALLBACK_RIPPY: &[u8] = include_bytes!("../../../resources/rippy.png");

/// SVG favicon rasterization is a small self-contained path that intentionally
/// supports only the native first-cut SVG subset needed for favicons. SVG root
/// dimensions can be arbitrarily large, so clamp the raster target to avoid
/// pathological allocations.
const MAX_SVG_PIXMAP_DIMENSION: u32 = 5000;

//
// TODO(gw): Remaining work on image cache:
//     * Make use of the prefetch support in various parts of the code.
//     * Profile time in GetImageIfAvailable - might be worth caching these
//       results per paint / layout.
//
// MAYBE(Yoric):
//     * For faster lookups, it might be useful to store the LoadKey in the
//       DOM once we have performed a first load.

// ======================================================================
// Helper functions.
// ======================================================================

pub fn rasterize_svg_bytes_sync(
    bytes: &[u8],
    requested_size: DeviceIntSize,
) -> Option<RasterImage> {
    let metadata = extract_svg_root_metadata(bytes)
        .inspect_err(|error| warn!("Error when parsing SVG metadata: {error:?}"))
        .ok()?;
    let scene = parse_svg_favicon_scene(bytes)
        .inspect_err(|error| warn!("Error when parsing SVG data: {error}"))
        .ok()?;

    let width = u32::try_from(requested_size.width)
        .unwrap_or(0)
        .clamp(1, MAX_SVG_PIXMAP_DIMENSION);
    let height = u32::try_from(requested_size.height)
        .unwrap_or(0)
        .clamp(1, MAX_SVG_PIXMAP_DIMENSION);
    let width_u16 = u16::try_from(width).ok()?;
    let height_u16 = u16::try_from(height).ok()?;

    let mut context = RenderContext::new(width_u16, height_u16);
    let root_transform = compute_favicon_root_transform(&metadata.viewport, width as f64, height as f64)?;
    let inherited = FaviconInheritedStyle::default();
    for child in &scene.children {
        render_svg_favicon_node(&mut context, child, root_transform, &inherited);
    }

    let mut pixmap = Pixmap::new(width_u16, height_u16);
    context.flush();
    context.render_to_pixmap(&mut pixmap);
    let bytes = pixmap.data_as_u8_slice().to_vec();
    let frame = ImageFrame {
        delay: None,
        byte_range: 0..bytes.len(),
        width,
        height,
    };

    Some(RasterImage {
        metadata: ImageMetadata { width, height },
        format: PixelFormat::RGBA8,
        frames: vec![frame],
        bytes: Arc::new(bytes),
        id: None,
        cors_status: CorsStatus::Unsafe,
        is_opaque: false,
    })
}

#[derive(Clone)]
struct FaviconScene {
    children: Vec<FaviconNode>,
}

#[derive(Clone)]
struct FaviconNode {
    transform: Affine,
    style: FaviconNodeStyle,
    kind: FaviconNodeKind,
}

#[derive(Clone)]
enum FaviconNodeKind {
    Group { children: Vec<FaviconNode> },
    Path(BezPath),
}

#[derive(Clone, Default)]
struct FaviconNodeStyle {
    fill: Option<Option<[u8; 4]>>,
    stroke: Option<Option<[u8; 4]>>,
    stroke_width: Option<f64>,
    fill_rule: Option<Fill>,
    opacity: Option<f32>,
    line_cap: Option<Cap>,
    line_join: Option<Join>,
    hidden: bool,
}

#[derive(Clone)]
struct FaviconInheritedStyle {
    fill: Option<[u8; 4]>,
    stroke: Option<[u8; 4]>,
    stroke_width: f64,
    fill_rule: Fill,
    opacity: f32,
    line_cap: Cap,
    line_join: Join,
    hidden: bool,
}

impl Default for FaviconInheritedStyle {
    fn default() -> Self {
        Self {
            fill: Some([0, 0, 0, 255]),
            stroke: None,
            stroke_width: 1.0,
            fill_rule: Fill::NonZero,
            opacity: 1.0,
            line_cap: Cap::Butt,
            line_join: Join::Miter,
            hidden: false,
        }
    }
}

impl FaviconInheritedStyle {
    fn with_node_style(&self, style: &FaviconNodeStyle) -> Self {
        Self {
            fill: style.fill.clone().unwrap_or(self.fill),
            stroke: style.stroke.clone().unwrap_or(self.stroke),
            stroke_width: style.stroke_width.unwrap_or(self.stroke_width),
            fill_rule: style.fill_rule.unwrap_or(self.fill_rule),
            opacity: (self.opacity * style.opacity.unwrap_or(1.0)).clamp(0.0, 1.0),
            line_cap: style.line_cap.unwrap_or(self.line_cap),
            line_join: style.line_join.unwrap_or(self.line_join),
            hidden: self.hidden || style.hidden,
        }
    }
}

fn render_svg_favicon_node(
    context: &mut RenderContext,
    node: &FaviconNode,
    current_transform: Affine,
    inherited: &FaviconInheritedStyle,
) {
    let transform = current_transform * node.transform;
    let style = inherited.with_node_style(&node.style);
    if style.hidden || style.opacity <= 0.0 {
        return;
    }

    match &node.kind {
        FaviconNodeKind::Group { children } => {
            for child in children {
                render_svg_favicon_node(context, child, transform, &style);
            }
        }
        FaviconNodeKind::Path(path) => {
            context.set_transform(transform);
            context.set_fill_rule(style.fill_rule);
            if let Some(fill) = style.fill {
                context.set_paint(color_with_opacity(fill, style.opacity));
                context.fill_path(path);
            }
            if let Some(stroke) = style.stroke {
                context.set_paint(color_with_opacity(stroke, style.opacity));
                context.set_stroke(Stroke {
                    width: style.stroke_width.max(0.0),
                    join: style.line_join,
                    start_cap: style.line_cap,
                    end_cap: style.line_cap,
                    ..Default::default()
                });
                context.stroke_path(path);
            }
        }
    }
}

fn color_with_opacity(color: [u8; 4], opacity: f32) -> AlphaColor<Srgb> {
    let alpha = (f32::from(color[3]) * opacity).round().clamp(0.0, 255.0) as u8;
    AlphaColor::from_rgba8(color[0], color[1], color[2], alpha)
}

fn compute_favicon_root_transform(
    viewport: &crate::layout::SVGViewportData,
    requested_width: f64,
    requested_height: f64,
) -> Option<Affine> {
    let (view_box_x, view_box_y, view_box_width, view_box_height) = match viewport.view_box {
        Some(view_box) => (view_box.x as f64, view_box.y as f64, view_box.width as f64, view_box.height as f64),
        None => {
            let width = viewport.width.and_then(resolve_svg_length_to_user_units)? as f64;
            let height = viewport.height.and_then(resolve_svg_length_to_user_units)? as f64;
            (0.0, 0.0, width, height)
        }
    };
    if view_box_width <= 0.0 || view_box_height <= 0.0 {
        return None;
    }

    let preserve = viewport.preserve_aspect_ratio;
    let scale_x = requested_width / view_box_width;
    let scale_y = requested_height / view_box_height;
    let (scale_x, scale_y, align_x, align_y) = if preserve.align == crate::layout::SVG_PRESERVEASPECTRATIO_NONE {
        (scale_x, scale_y, 0.0, 0.0)
    } else {
        let uniform = if preserve.meet_or_slice == crate::layout::SVG_MEETORSLICE_SLICE {
            scale_x.max(scale_y)
        } else {
            scale_x.min(scale_y)
        };
        let extra_x = requested_width - view_box_width * uniform;
        let extra_y = requested_height - view_box_height * uniform;
        let (align_x_factor, align_y_factor) = match preserve.align {
            crate::layout::SVG_PRESERVEASPECTRATIO_XMINYMIN => (0.0, 0.0),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMIDYMIN => (0.5, 0.0),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMAXYMIN => (1.0, 0.0),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMINYMID => (0.0, 0.5),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMIDYMID => (0.5, 0.5),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMAXYMID => (1.0, 0.5),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMINYMAX => (0.0, 1.0),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMIDYMAX => (0.5, 1.0),
            crate::layout::SVG_PRESERVEASPECTRATIO_XMAXYMAX => (1.0, 1.0),
            _ => (0.5, 0.5),
        };
        (
            uniform,
            uniform,
            extra_x * align_x_factor,
            extra_y * align_y_factor,
        )
    };

    Some(Affine::new([
        scale_x,
        0.0,
        0.0,
        scale_y,
        align_x - view_box_x * scale_x,
        align_y - view_box_y * scale_y,
    ]))
}

fn parse_svg_favicon_scene(bytes: &[u8]) -> Result<FaviconScene, String> {
    let source = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let sink = SvgFaviconSink::default();
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(source));
    let tokenizer = XmlTokenizer::new(sink, Default::default());
    match tokenizer.feed(&input) {
        TokenizerResult::Done | TokenizerResult::Script(_) | TokenizerResult::EncodingIndicator(_) => {}
    }
    tokenizer.end();
    tokenizer.sink.into_scene()
}

#[derive(Default)]
struct SvgFaviconSink {
    state: RefCell<SvgFaviconState>,
}

impl SvgFaviconSink {
    fn into_scene(self) -> Result<FaviconScene, String> {
        self.state.into_inner().into_scene()
    }
}

impl TokenSink for SvgFaviconSink {
    type Handle = ();

    fn process_token(&self, token: Token) -> ProcessResult<Self::Handle> {
        let mut state = self.state.borrow_mut();
        if state.error.is_some() {
            return ProcessResult::Done;
        }
        match token {
            Token::Tag(tag) => state.process_tag(tag),
            Token::ParseError(error) => {
                state.error = Some(error.into_owned());
                ProcessResult::Done
            }
            Token::EndOfFile => {
                state.finish();
                ProcessResult::Done
            }
            _ => ProcessResult::Continue,
        }
    }
}

#[derive(Default)]
struct SvgFaviconState {
    root: Option<FaviconNode>,
    stack: Vec<OpenFaviconNode>,
    error: Option<String>,
}

struct OpenFaviconNode {
    local_name: String,
    node: Option<FaviconNode>,
}

impl SvgFaviconState {
    fn into_scene(self) -> Result<FaviconScene, String> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let root = self.root.ok_or_else(|| "not an SVG document".to_string())?;
        let FaviconNodeKind::Group { children } = root.kind else {
            return Err("invalid SVG root".to_string());
        };
        Ok(FaviconScene { children })
    }

    fn finish(&mut self) {
        while let Some(open) = self.stack.pop() {
            self.attach(open);
        }
        if self.root.is_none() && self.error.is_none() {
            self.error = Some("not an SVG document".to_string());
        }
    }

    fn process_tag(&mut self, tag: Tag) -> ProcessResult<()> {
        match tag.kind {
            TagKind::StartTag => {
                self.start(tag);
                ProcessResult::Continue
            }
            TagKind::EmptyTag => {
                self.start(tag.clone());
                if self.error.is_some() {
                    return ProcessResult::Done;
                }
                self.end(&tag.name.local.to_string());
                if self.error.is_some() {
                    ProcessResult::Done
                } else {
                    ProcessResult::Continue
                }
            }
            TagKind::EndTag => {
                self.end(&tag.name.local.to_string());
                if self.error.is_some() {
                    ProcessResult::Done
                } else {
                    ProcessResult::Continue
                }
            }
            TagKind::ShortTag => ProcessResult::Continue,
        }
    }

    fn start(&mut self, tag: Tag) {
        let local_name = tag.name.local.to_string();
        let attrs = collect_svg_attr_map(&tag);
        let node = if self.stack.is_empty() {
            if local_name != "svg" {
                self.error = Some("not an SVG document".to_string());
                return;
            }
            Some(FaviconNode {
                transform: parse_svg_favicon_transform(attrs.get("transform").map(String::as_str)),
                style: parse_svg_favicon_style(&attrs),
                kind: FaviconNodeKind::Group { children: Vec::new() },
            })
        } else {
            parse_svg_favicon_node(&local_name, &attrs)
        };
        self.stack.push(OpenFaviconNode { local_name, node });
    }

    fn end(&mut self, local_name: &str) {
        let Some(open) = self.stack.pop() else {
            self.error = Some(format!("unexpected end tag </{local_name}>"));
            return;
        };
        if open.local_name != local_name {
            self.error = Some(format!("mismatched end tag </{local_name}>"));
            return;
        }
        self.attach(open);
    }

    fn attach(&mut self, open: OpenFaviconNode) {
        let Some(node) = open.node else {
            return;
        };
        if self.stack.iter().any(|open| open.node.is_none()) {
            return;
        }
        if let Some(parent) = self.stack.iter_mut().rev().find_map(|open| open.node.as_mut()) {
            if let FaviconNodeKind::Group { children } = &mut parent.kind {
                children.push(node);
            }
            return;
        }
        self.root = Some(node);
    }
}

fn collect_svg_attr_map(tag: &Tag) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    for attr in &tag.attrs {
        attrs.insert(attr.name.local.to_string(), attr.value.to_string());
    }
    if let Some(style) = attrs.get("style").cloned() {
        for declaration in style.split(';') {
            let Some((name, value)) = declaration.split_once(':') else {
                continue;
            };
            attrs.insert(name.trim().to_string(), value.trim().to_string());
        }
    }
    attrs
}

fn parse_svg_favicon_node(local_name: &str, attrs: &HashMap<String, String>) -> Option<FaviconNode> {
    let transform = parse_svg_favicon_transform(attrs.get("transform").map(String::as_str));
    let style = parse_svg_favicon_style(attrs);
    let kind = match local_name {
        "g" | "svg" => FaviconNodeKind::Group { children: Vec::new() },
        "path" => FaviconNodeKind::Path(parse_svg_favicon_path(attrs.get("d")?)?),
        "rect" => FaviconNodeKind::Path(parse_svg_favicon_rect(attrs)?),
        "circle" => FaviconNodeKind::Path(parse_svg_favicon_circle(attrs)?),
        "ellipse" => FaviconNodeKind::Path(parse_svg_favicon_ellipse(attrs)?),
        "line" => FaviconNodeKind::Path(parse_svg_favicon_line(attrs)?),
        "polyline" => FaviconNodeKind::Path(parse_svg_favicon_poly(attrs.get("points")?, false)?),
        "polygon" => FaviconNodeKind::Path(parse_svg_favicon_poly(attrs.get("points")?, true)?),
        _ => return None,
    };
    Some(FaviconNode { transform, style, kind })
}

fn parse_svg_favicon_style(attrs: &HashMap<String, String>) -> FaviconNodeStyle {
    let opacity = attrs
        .get("opacity")
        .and_then(|value| parse_svg_optional_number(Some(value)))
        .map(|value| value.clamp(0.0, 1.0));
    FaviconNodeStyle {
        fill: parse_svg_favicon_paint(attrs.get("fill").map(String::as_str)),
        stroke: parse_svg_favicon_paint(attrs.get("stroke").map(String::as_str)),
        stroke_width: attrs
            .get("stroke-width")
            .and_then(|value| parse_svg_favicon_length(value))
            .map(f64::from),
        fill_rule: attrs.get("fill-rule").and_then(|value| match value.as_str() {
            "evenodd" => Some(Fill::EvenOdd),
            "nonzero" => Some(Fill::NonZero),
            _ => None,
        }),
        opacity,
        line_cap: attrs.get("stroke-linecap").and_then(|value| match value.as_str() {
            "round" => Some(Cap::Round),
            "square" => Some(Cap::Square),
            "butt" => Some(Cap::Butt),
            _ => None,
        }),
        line_join: attrs.get("stroke-linejoin").and_then(|value| match value.as_str() {
            "round" => Some(Join::Round),
            "bevel" => Some(Join::Bevel),
            "miter" => Some(Join::Miter),
            _ => None,
        }),
        hidden: attrs.get("display").is_some_and(|value| value == "none") ||
            attrs.get("visibility").is_some_and(|value| value == "hidden"),
    }
}

fn parse_svg_favicon_paint(raw: Option<&str>) -> Option<Option<[u8; 4]>> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.eq_ignore_ascii_case("none") {
        return Some(None);
    }
    parse_svg_favicon_color(raw).map(Some)
}

fn parse_svg_favicon_color(raw: &str) -> Option<[u8; 4]> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("currentColor") {
        return Some([0, 0, 0, 255]);
    }
    if raw.eq_ignore_ascii_case("transparent") {
        return Some([0, 0, 0, 0]);
    }
    if let Some(hex) = raw.strip_prefix('#') {
        return match hex.len() {
            3 => Some([
                u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
                255,
            ]),
            4 => Some([
                u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[3..4].repeat(2), 16).ok()?,
            ]),
            6 => Some([
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
                255,
            ]),
            8 => Some([
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
                u8::from_str_radix(&hex[6..8], 16).ok()?,
            ]),
            _ => None,
        };
    }
    if let Some(args) = raw.strip_prefix("rgb(").and_then(|value| value.strip_suffix(')')) {
        let components = parse_svg_color_components(args, false)?;
        return Some([components[0], components[1], components[2], 255]);
    }
    if let Some(args) = raw.strip_prefix("rgba(").and_then(|value| value.strip_suffix(')')) {
        let components = parse_svg_color_components(args, true)?;
        return Some(components);
    }
    match raw.to_ascii_lowercase().as_str() {
        "black" => Some([0, 0, 0, 255]),
        "white" => Some([255, 255, 255, 255]),
        "red" => Some([255, 0, 0, 255]),
        "green" => Some([0, 128, 0, 255]),
        "blue" => Some([0, 0, 255, 255]),
        "yellow" => Some([255, 255, 0, 255]),
        "gray" | "grey" => Some([128, 128, 128, 255]),
        "silver" => Some([192, 192, 192, 255]),
        "maroon" => Some([128, 0, 0, 255]),
        "purple" => Some([128, 0, 128, 255]),
        "fuchsia" | "magenta" => Some([255, 0, 255, 255]),
        "lime" => Some([0, 255, 0, 255]),
        "olive" => Some([128, 128, 0, 255]),
        "navy" => Some([0, 0, 128, 255]),
        "teal" => Some([0, 128, 128, 255]),
        "aqua" | "cyan" => Some([0, 255, 255, 255]),
        _ => None,
    }
}

fn parse_svg_color_components(args: &str, has_alpha: bool) -> Option<[u8; 4]> {
    let parts = args
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let expected = if has_alpha { 4 } else { 3 };
    if parts.len() != expected {
        return None;
    }
    let parse_rgb = |raw: &str| {
        if let Some(percent) = raw.strip_suffix('%') {
            let value: f32 = percent.parse().ok()?;
            Some((value * 2.55).round().clamp(0.0, 255.0) as u8)
        } else {
            let value: f32 = raw.parse().ok()?;
            Some(value.round().clamp(0.0, 255.0) as u8)
        }
    };
    let parse_alpha = |raw: &str| {
        if let Some(percent) = raw.strip_suffix('%') {
            let value: f32 = percent.parse().ok()?;
            Some((value * 2.55).round().clamp(0.0, 255.0) as u8)
        } else {
            let value: f32 = raw.parse().ok()?;
            Some((value * 255.0).round().clamp(0.0, 255.0) as u8)
        }
    };
    Some([
        parse_rgb(parts[0])?,
        parse_rgb(parts[1])?,
        parse_rgb(parts[2])?,
        if has_alpha { parse_alpha(parts[3])? } else { 255 },
    ])
}

fn parse_svg_favicon_transform(raw: Option<&str>) -> Affine {
    let transform = crate::layout::compose_svg_transform_list(&parse_svg_transform_list(raw));
    Affine::new([
        transform.m11 as f64,
        transform.m12 as f64,
        transform.m21 as f64,
        transform.m22 as f64,
        transform.m31 as f64,
        transform.m32 as f64,
    ])
}

fn parse_svg_favicon_length(raw: &str) -> Option<f32> {
    resolve_svg_length_to_user_units(parse_svg_length(Some(raw)))
}

fn parse_svg_favicon_path(raw: &str) -> Option<BezPath> {
    let mut path = BezPath::new();
    for segment in svgtypes::SimplifyingPathParser::from(raw) {
        match segment.ok()? {
            svgtypes::SimplePathSegment::MoveTo { x, y } => path.move_to(Point::new(x, y)),
            svgtypes::SimplePathSegment::LineTo { x, y } => path.line_to(Point::new(x, y)),
            svgtypes::SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } => {
                path.curve_to(Point::new(x1, y1), Point::new(x2, y2), Point::new(x, y));
            }
            svgtypes::SimplePathSegment::Quadratic { x1, y1, x, y } => {
                path.quad_to(Point::new(x1, y1), Point::new(x, y));
            }
            svgtypes::SimplePathSegment::ClosePath => path.close_path(),
        }
    }
    Some(path)
}

fn parse_svg_favicon_rect(attrs: &HashMap<String, String>) -> Option<BezPath> {
    let x = attrs.get("x").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let y = attrs.get("y").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let width = attrs.get("width").and_then(|value| parse_svg_favicon_length(value))?;
    let height = attrs.get("height").and_then(|value| parse_svg_favicon_length(value))?;
    Some(Rect::new(x as f64, y as f64, (x + width) as f64, (y + height) as f64).to_path(0.1))
}

fn parse_svg_favicon_circle(attrs: &HashMap<String, String>) -> Option<BezPath> {
    let cx = attrs.get("cx").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let cy = attrs.get("cy").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let r = attrs.get("r").and_then(|value| parse_svg_favicon_length(value))?;
    Some(Circle::new((cx as f64, cy as f64), r as f64).to_path(0.1))
}

fn parse_svg_favicon_ellipse(attrs: &HashMap<String, String>) -> Option<BezPath> {
    let cx = attrs.get("cx").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let cy = attrs.get("cy").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let rx = attrs.get("rx").and_then(|value| parse_svg_favicon_length(value))?;
    let ry = attrs.get("ry").and_then(|value| parse_svg_favicon_length(value))?;
    Some(Ellipse::new((cx as f64, cy as f64), (rx as f64, ry as f64), 0.0).to_path(0.1))
}

fn parse_svg_favicon_line(attrs: &HashMap<String, String>) -> Option<BezPath> {
    let x1 = attrs.get("x1").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let y1 = attrs.get("y1").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let x2 = attrs.get("x2").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    let y2 = attrs.get("y2").and_then(|value| parse_svg_favicon_length(value)).unwrap_or(0.0);
    Some(Line::new((x1 as f64, y1 as f64), (x2 as f64, y2 as f64)).to_path(0.1))
}

fn parse_svg_favicon_poly(raw: &str, closed: bool) -> Option<BezPath> {
    let mut points = svgtypes::PointsParser::from(raw)
        .map(|(x, y)| Point::new(x, y))
        .collect::<Vec<_>>();
    let first = points.first().copied()?;
    let mut path = BezPath::new();
    path.move_to(first);
    for point in points.drain(1..) {
        path.line_to(point);
    }
    if closed {
        path.close_path();
    }
    Some(path)
}

fn decode_bytes_sync(
    key: LoadKey,
    bytes: &[u8],
    cors: CorsStatus,
    content_type: Option<Mime>,
) -> DecoderMsg {
    let is_svg_document = content_type.is_some_and(|content_type| {
        (
            content_type.type_(),
            content_type.subtype(),
            content_type.suffix(),
        ) == (mime::IMAGE, mime::SVG, Some(mime::XML))
    });

    let image = if is_svg_document {
        extract_svg_root_metadata(bytes).ok().map(|metadata| {
            let width = metadata
                .viewport
                .width
                .and_then(crate::layout::resolve_svg_length_to_user_units)
                .filter(|value| *value > 0.0)
                .or_else(|| metadata.viewport.view_box.map(|view_box| view_box.width.max(0.0)))
                .unwrap_or(0.0) as u32;
            let height = metadata
                .viewport
                .height
                .and_then(crate::layout::resolve_svg_length_to_user_units)
                .filter(|value| *value > 0.0)
                .or_else(|| metadata.viewport.view_box.map(|view_box| view_box.height.max(0.0)))
                .unwrap_or(0.0) as u32;
            DecodedImage::VectorMetadata(VectorImageData {
                metadata: ImageMetadata { width, height },
                bytes: Arc::new(bytes.to_vec()),
                cors_status: cors,
            })
        })
    } else {
        load_from_memory(bytes, cors).map(DecodedImage::Raster)
    };

    DecoderMsg { key, image }
}

fn set_webrender_image_key(
    paint_api: &CrossProcessPaintApi,
    image: &mut RasterImage,
    image_key: WebRenderImageKey,
) {
    if image.id.is_some() {
        return;
    }

    let (descriptor, ipc_shared_memory) = image.webrender_image_descriptor_and_data_for_frame(0);
    let data = SerializableImageData::Raw(ipc_shared_memory);

    paint_api.add_image(image_key, descriptor, data, image.should_animate());
    image.id = Some(image_key);
}

// ======================================================================
// Aux structs and enums.
// ======================================================================

/// <https://html.spec.whatwg.org/multipage/#list-of-available-images>
type ImageKey = (BrowserUrl, ImmutableOrigin, Option<CorsSettings>);

// Represents all the currently pending loads/decodings. For
// performance reasons, loads are indexed by a dedicated load key.
#[derive(MallocSizeOf)]
struct AllPendingLoads {
    // The loads, indexed by a load key. Used during most operations,
    // for performance reasons.
    loads: FxHashMap<LoadKey, PendingLoad>,

    // Get a load key from its url and requesting origin. Used ony when starting and
    // finishing a load or when adding a new listener.
    url_to_load_key: HashMap<ImageKey, LoadKey>,

    // A counter used to generate instances of LoadKey
    keygen: LoadKeyGenerator,
}

impl AllPendingLoads {
    fn new() -> AllPendingLoads {
        AllPendingLoads {
            loads: FxHashMap::default(),
            url_to_load_key: HashMap::default(),
            keygen: LoadKeyGenerator::new(),
        }
    }

    // get a PendingLoad from its LoadKey.
    fn get_by_key_mut(&mut self, key: &LoadKey) -> Option<&mut PendingLoad> {
        self.loads.get_mut(key)
    }

    fn remove(&mut self, key: &LoadKey) -> Option<PendingLoad> {
        self.loads.remove(key).inspect(|pending_load| {
            self.url_to_load_key
                .remove(&(
                    pending_load.url.clone(),
                    pending_load.load_origin.clone(),
                    pending_load.cors_setting,
                ))
                .unwrap();
        })
    }

    fn get_cached(
        &mut self,
        url: BrowserUrl,
        origin: ImmutableOrigin,
        cors_status: Option<CorsSettings>,
    ) -> CacheResult<'_> {
        match self
            .url_to_load_key
            .entry((url.clone(), origin.clone(), cors_status))
        {
            Occupied(url_entry) => {
                let load_key = url_entry.get();
                CacheResult::Hit(*load_key, self.loads.get_mut(load_key).unwrap())
            },
            Vacant(url_entry) => {
                let load_key = self.keygen.next();
                url_entry.insert(load_key);

                let pending_load = PendingLoad::new(url, origin, cors_status);
                match self.loads.entry(load_key) {
                    Occupied(_) => unreachable!(),
                    Vacant(load_entry) => {
                        let mut_load = load_entry.insert(pending_load);
                        CacheResult::Miss(Some((load_key, mut_load)))
                    },
                }
            },
        }
    }
}

/// Result of accessing a cache.
enum CacheResult<'a> {
    /// The value was in the cache.
    Hit(LoadKey, &'a mut PendingLoad),
    /// The value was not in the cache and needed to be regenerated.
    Miss(Option<(LoadKey, &'a mut PendingLoad)>),
}

/// Represents an image that has completed loading.
/// Images that fail to load (due to network or decode
/// failure) are still stored here, so that they aren't
/// fetched again.
#[derive(MallocSizeOf)]
struct CompletedLoad {
    image_response: ImageResponse,
    id: PendingImageId,
}

impl CompletedLoad {
    fn new(image_response: ImageResponse, id: PendingImageId) -> CompletedLoad {
        CompletedLoad { image_response, id }
    }
}

#[derive(Clone, MallocSizeOf)]
struct VectorImageData {
    metadata: ImageMetadata,
    #[conditional_malloc_size_of]
    bytes: Arc<Vec<u8>>,
    cors_status: CorsStatus,
}

enum DecodedImage {
    Raster(RasterImage),
    VectorMetadata(VectorImageData),
}

/// Message that the decoder worker threads send to the image cache.
struct DecoderMsg {
    key: LoadKey,
    image: Option<DecodedImage>,
}

#[derive(MallocSizeOf)]
enum ImageBytes {
    InProgress(Vec<u8>),
    Complete(#[conditional_malloc_size_of] Arc<Vec<u8>>),
}

impl ImageBytes {
    fn extend_from_slice(&mut self, data: &[u8]) {
        match *self {
            ImageBytes::InProgress(ref mut bytes) => bytes.extend_from_slice(data),
            ImageBytes::Complete(_) => panic!("attempted modification of complete image bytes"),
        }
    }

    fn mark_complete(&mut self) -> Arc<Vec<u8>> {
        let bytes = {
            let own_bytes = match *self {
                ImageBytes::InProgress(ref mut bytes) => bytes,
                ImageBytes::Complete(_) => panic!("attempted modification of complete image bytes"),
            };
            mem::take(own_bytes)
        };
        let bytes = Arc::new(bytes);
        *self = ImageBytes::Complete(bytes.clone());
        bytes
    }

    fn as_slice(&self) -> &[u8] {
        match *self {
            ImageBytes::InProgress(ref bytes) => bytes,
            ImageBytes::Complete(ref bytes) => bytes,
        }
    }
}

// A key used to communicate during loading.
type LoadKey = PendingImageId;

#[derive(MallocSizeOf)]
struct LoadKeyGenerator {
    counter: u64,
}

impl LoadKeyGenerator {
    fn new() -> LoadKeyGenerator {
        LoadKeyGenerator { counter: 0 }
    }
    fn next(&mut self) -> PendingImageId {
        self.counter += 1;
        PendingImageId(self.counter)
    }
}

enum LoadResult {
    LoadedRasterImage(RasterImage),
    LoadedVectorMetadata(VectorImageData),
    FailedToLoadOrDecode,
}

impl LoadResult {
    fn label(&self) -> &'static str {
        match self {
            Self::LoadedRasterImage(..) => "raster image",
            Self::LoadedVectorMetadata(..) => "vector metadata",
            Self::FailedToLoadOrDecode => "decode failure",
        }
    }
}

/// Represents an image that is either being loaded
/// by the resource thread, or decoded by a worker thread.
#[derive(MallocSizeOf)]
struct PendingLoad {
    /// The bytes loaded so far. Reset to an empty vector once loading
    /// is complete and the buffer has been transmitted to the decoder.
    bytes: ImageBytes,

    /// Image metadata, if available.
    metadata: Option<ImageMetadata>,

    /// Once loading is complete, the result of the operation.
    result: Option<Result<(), NetworkError>>,

    /// The listeners that are waiting for this response to complete.
    listeners: Vec<ImageLoadListener>,

    /// The url being loaded. Do not forget that this may be several Mb
    /// if we are loading a data: url.
    url: BrowserUrl,

    /// The origin that requested this load.
    load_origin: ImmutableOrigin,

    /// The CORS attribute setting for the requesting
    cors_setting: Option<CorsSettings>,

    /// The CORS status of this image response.
    cors_status: CorsStatus,

    /// The URL of the final response that contains a body.
    final_url: Option<BrowserUrl>,

    /// The MIME type from the `Content-type` header of the HTTP response, if any.
    content_type: Option<Mime>,
}

impl PendingLoad {
    fn new(
        url: BrowserUrl,
        load_origin: ImmutableOrigin,
        cors_setting: Option<CorsSettings>,
    ) -> PendingLoad {
        PendingLoad {
            bytes: ImageBytes::InProgress(vec![]),
            metadata: None,
            result: None,
            listeners: vec![],
            url,
            load_origin,
            final_url: None,
            cors_setting,
            cors_status: CorsStatus::Unsafe,
            content_type: None,
        }
    }

    fn add_listener(&mut self, listener: ImageLoadListener) {
        self.listeners.push(listener);
    }
}

/// Used for storing images that do not have a `WebRenderImageKey` yet.
#[derive(Debug, MallocSizeOf)]
enum PendingKey {
    RasterImage((LoadKey, RasterImage)),
}

/// The state of the `WebRenderImageKey`` cache
#[derive(Debug, MallocSizeOf)]
enum KeyCacheState {
    /// We already requested a batch of keys.
    PendingBatch,
    /// We have some keys in the cache.
    Ready(Vec<WebRenderImageKey>),
}

impl KeyCacheState {
    fn size(&self) -> usize {
        match self {
            KeyCacheState::PendingBatch => 0,
            KeyCacheState::Ready(items) => items.len(),
        }
    }
}

/// As getting new keys takes a round trip over the constellation, we keep a small cache of them.
/// Additionally, this cache will store image resources that do not have a key yet because those
/// are needed to complete the load.
#[derive(MallocSizeOf)]
struct KeyCache {
    /// A cache of `WebRenderImageKey`.
    cache: KeyCacheState,
    /// These images are loaded but have no key assigned to yet.
    images_pending_keys: VecDeque<PendingKey>,
}

impl KeyCache {
    fn new() -> Self {
        KeyCache {
            cache: KeyCacheState::Ready(Vec::new()),
            images_pending_keys: VecDeque::new(),
        }
    }
}

/// ## Image cache implementation.
#[derive(MallocSizeOf)]
struct ImageCacheStore {
    /// Images that are loading over network, or decoding.
    pending_loads: AllPendingLoads,

    /// Images that have finished loading (successful or not)
    completed_loads: HashMap<ImageKey, CompletedLoad>,

    /// Vector (e.g. SVG) images that have been successfully loaded. The cache keeps
    /// the original bytes plus natural dimensions so later stages can hand the SVG to
    /// the browser-scene renderer.
    vector_images: FxHashMap<PendingImageId, VectorImageData>,

    /// The [`RasterImage`] used for the broken image icon, initialized lazily, only when necessary.
    #[conditional_malloc_size_of]
    broken_image_icon_image: OnceCell<Option<Arc<RasterImage>>>,

    /// Cross-process `Paint` API instance.
    paint_api: CrossProcessPaintApi,

    /// The [`WebView`] of the `Webview` associated with this [`ImageCache`].
    webview_id: WebViewId,

    /// The [`PipelineId`] of the `Pipeline` associated with this [`ImageCache`].
    pipeline_id: PipelineId,

    /// Main struct to handle the cache of `WebRenderImageKey` and
    /// images that do not have a key yet.
    key_cache: KeyCache,
}

impl ImageCacheStore {
    /// Finishes loading the image by setting the WebRenderImageKey and completing the load.
    fn set_key_and_finish_load(&mut self, pending_image: PendingKey, image_key: WebRenderImageKey) {
        match pending_image {
            PendingKey::RasterImage((pending_id, mut raster_image)) => {
                set_webrender_image_key(&self.paint_api, &mut raster_image, image_key);
                self.complete_load(pending_id, LoadResult::LoadedRasterImage(raster_image));
            },
        }
    }

    /// If a key is available the image will be immediately loaded, otherwise it will load when the
    /// next batch of keys is received. Only call this if the image does not have a `LoadKey` yet.
    fn load_image_with_keycache(&mut self, pending_image: PendingKey) {
        match self.key_cache.cache {
            KeyCacheState::PendingBatch => {
                self.key_cache.images_pending_keys.push_back(pending_image);
            },
            KeyCacheState::Ready(ref mut cache) => match cache.pop() {
                Some(image_key) => {
                    self.set_key_and_finish_load(pending_image, image_key);
                },
                None => {
                    self.key_cache.images_pending_keys.push_back(pending_image);
                    self.fetch_more_image_keys();
                },
            },
        }
    }

    fn fetch_more_image_keys(&mut self) {
        self.key_cache.cache = KeyCacheState::PendingBatch;
        self.paint_api
            .generate_image_key_async(self.webview_id, self.pipeline_id);
    }

    /// Insert received keys into the cache and complete the loading of images.
    fn insert_keys_and_load_images(&mut self, image_keys: Vec<WebRenderImageKey>) {
        if let KeyCacheState::PendingBatch = self.key_cache.cache {
            self.key_cache.cache = KeyCacheState::Ready(image_keys);
            let len = min(
                self.key_cache.cache.size(),
                self.key_cache.images_pending_keys.len(),
            );
            let images = self
                .key_cache
                .images_pending_keys
                .drain(0..len)
                .collect::<Vec<PendingKey>>();
            for key in images {
                self.load_image_with_keycache(key);
            }
            if !self.key_cache.images_pending_keys.is_empty() {
                self.paint_api
                    .generate_image_key_async(self.webview_id, self.pipeline_id);
                self.key_cache.cache = KeyCacheState::PendingBatch
            }
        } else {
            unreachable!("A batch was received while we didn't request one")
        }
    }

    /// The rest of complete load. This requires that raster images have a valid
    /// `WebRenderImageKey`.
    fn complete_load(&mut self, key: LoadKey, load_result: LoadResult) {
        debug!("Completed decoding for {}", load_result.label());
        let pending_load = match self.pending_loads.remove(&key) {
            Some(load) => load,
            None => return,
        };
        let url = pending_load.final_url.clone();
        let image_response = match load_result {
            LoadResult::LoadedRasterImage(raster_image) => {
                assert!(raster_image.id.is_some());
                ImageResponse::Loaded(Image::Raster(Arc::new(raster_image)), url.unwrap())
            },
            LoadResult::LoadedVectorMetadata(vector_image_data) => {
                let metadata = vector_image_data.metadata;
                let cors_status = vector_image_data.cors_status;
                self.vector_images.insert(key, vector_image_data);

                let vector_image = VectorImage {
                    id: key,
                    metadata,
                    cors_status,
                };
                ImageResponse::Loaded(Image::Vector(vector_image), url.unwrap())
            },
            LoadResult::FailedToLoadOrDecode => ImageResponse::FailedToLoadOrDecode,
        };

        let completed_load = CompletedLoad::new(image_response.clone(), key);
        self.completed_loads.insert(
            (
                pending_load.url,
                pending_load.load_origin,
                pending_load.cors_setting,
            ),
            completed_load,
        );

        for listener in pending_load.listeners {
            listener.respond(image_response.clone());
        }
    }

    fn remove_loaded_image(
        &mut self,
        url: &BrowserUrl,
        origin: &ImmutableOrigin,
        cors_setting: &Option<CorsSettings>,
    ) {
        if let Some(loaded_image) =
            self.completed_loads
                .remove(&(url.clone(), origin.clone(), *cors_setting))
        {
            if let ImageResponse::Loaded(Image::Raster(image), _) = loaded_image.image_response {
                if image.id.is_some() {
                    self.paint_api.update_images(
                        self.webview_id.into(),
                        vec![ImageUpdate::DeleteImage(image.id.unwrap())].into(),
                    );
                }
            }
        }
    }

    /// Return a completed image if it exists, or None if there is no complete load
    /// or the complete load is not fully decoded or is unavailable.
    fn get_completed_image_if_available(
        &self,
        url: BrowserUrl,
        origin: ImmutableOrigin,
        cors_setting: Option<CorsSettings>,
    ) -> Option<Result<(Image, BrowserUrl), ()>> {
        self.completed_loads
            .get(&(url, origin, cors_setting))
            .map(|completed_load| match &completed_load.image_response {
                ImageResponse::Loaded(image, url) => Ok((image.clone(), url.clone())),
                ImageResponse::FailedToLoadOrDecode | ImageResponse::MetadataLoaded(_) => Err(()),
            })
    }

    /// Handle a message from one of the decoder worker threads or from a sync
    /// decoding operation.
    fn handle_decoder(&mut self, msg: DecoderMsg) {
        let image = match msg.image {
            None => LoadResult::FailedToLoadOrDecode,
            Some(DecodedImage::Raster(raster_image)) => {
                self.load_image_with_keycache(PendingKey::RasterImage((msg.key, raster_image)));
                return;
            },
            Some(DecodedImage::VectorMetadata(vector_image_data)) => {
                LoadResult::LoadedVectorMetadata(vector_image_data)
            },
        };
        self.complete_load(msg.key, image);
    }
}

pub struct ImageCacheFactoryImpl {
    /// The data to use for the broken image icon used when images cannot load.
    broken_image_icon_data: Arc<Vec<u8>>,
    /// Thread pool for image decoding
    thread_pool: Arc<ThreadPool>,
}

impl ImageCacheFactoryImpl {
    pub fn new(broken_image_icon_data: Vec<u8>) -> Self {
        debug!("Creating new ImageCacheFactoryImpl");

        // Uses an estimate of the system cpus to decode images
        // See https://doc.rust-lang.org/stable/std/thread/fn.available_parallelism.html
        // If no information can be obtained about the system, uses 4 threads as a default
        let thread_count = thread::available_parallelism()
            .map(|i| i.get())
            .unwrap_or(servo_config::pref!(threadpools_fallback_worker_num) as usize)
            .min(servo_config::pref!(threadpools_image_cache_workers_max).max(1) as usize);

        Self {
            broken_image_icon_data: Arc::new(broken_image_icon_data),
            thread_pool: Arc::new(ThreadPool::new(thread_count, "ImageCache".to_string())),
        }
    }
}

impl ImageCacheFactoryImpl {
    pub fn create(
        &self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
        paint_api: &CrossProcessPaintApi,
    ) -> Arc<dyn ImageCache> {
        Arc::new(ImageCacheImpl {
            store: Arc::new(Mutex::new(ImageCacheStore {
                pending_loads: AllPendingLoads::new(),
                completed_loads: HashMap::new(),
                vector_images: FxHashMap::default(),
                broken_image_icon_image: OnceCell::new(),
                paint_api: paint_api.clone(),
                pipeline_id,
                webview_id,
                key_cache: KeyCache::new(),
            })),
            broken_image_icon_data: self.broken_image_icon_data.clone(),
            thread_pool: self.thread_pool.clone(),
        })
    }
}

impl ImageCacheFactory for ImageCacheFactoryImpl {
    fn create(
        &self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
        paint_api: &CrossProcessPaintApi,
    ) -> Arc<dyn ImageCache> {
        Self::create(self, webview_id, pipeline_id, paint_api)
    }
}

pub struct ImageCacheImpl {
    /// Per-[`ImageCache`] data.
    store: Arc<Mutex<ImageCacheStore>>,
    /// The data to use for the broken image icon used when images cannot load.
    broken_image_icon_data: Arc<Vec<u8>>,
    /// Thread pool for image decoding. This is shared with other [`ImageCache`]s in the
    /// same process.
    thread_pool: Arc<ThreadPool>,
}

impl ImageCache for ImageCacheImpl {
    fn memory_reports(&self, prefix: &str, ops: &mut MallocSizeOfOps) -> Vec<Report> {
        let store_size = self.store.lock().size_of(ops);
        vec![Report {
            path: path![prefix, "image-cache"],
            kind: ReportKind::ExplicitSystemHeapSize,
            size: store_size,
        }]
    }

    fn get_image_key(&self) -> Option<WebRenderImageKey> {
        let mut store = self.store.lock();
        if let KeyCacheState::Ready(ref mut cache) = store.key_cache.cache {
            if let Some(image_key) = cache.pop() {
                return Some(image_key);
            }

            store.fetch_more_image_keys();
        }

        store
            .paint_api
            .generate_image_key_blocking(store.webview_id)
    }

    fn get_image(
        &self,
        url: BrowserUrl,
        origin: ImmutableOrigin,
        cors_setting: Option<CorsSettings>,
    ) -> Option<Image> {
        let store = self.store.lock();
        let result = store.get_completed_image_if_available(url, origin, cors_setting);
        match result {
            Some(Ok((img, _))) => Some(img),
            _ => None,
        }
    }

    fn get_cached_image_status(
        &self,
        url: BrowserUrl,
        origin: ImmutableOrigin,
        cors_setting: Option<CorsSettings>,
    ) -> ImageCacheResult {
        let mut store = self.store.lock();
        if let Some(result) =
            store.get_completed_image_if_available(url.clone(), origin.clone(), cors_setting)
        {
            match result {
                Ok((image, image_url)) => {
                    debug!("{} is available", url);
                    return ImageCacheResult::Available(ImageOrMetadataAvailable::ImageAvailable {
                        image,
                        url: image_url,
                    });
                },
                Err(()) => {
                    debug!("{} is not available", url);
                    return ImageCacheResult::FailedToLoadOrDecode;
                },
            }
        }

        let (key, decoded) = {
            let result = store
                .pending_loads
                .get_cached(url.clone(), origin.clone(), cors_setting);
            match result {
                CacheResult::Hit(key, pl) => match (&pl.result, &pl.metadata) {
                    (&Some(Ok(_)), _) => {
                        debug!("Sync decoding {} ({:?})", url, key);
                        (
                            key,
                            decode_bytes_sync(
                                key,
                                pl.bytes.as_slice(),
                                pl.cors_status,
                                pl.content_type.clone(),
                            ),
                        )
                    },
                    (&None, Some(meta)) => {
                        debug!("Metadata available for {} ({:?})", url, key);
                        return ImageCacheResult::Available(
                            ImageOrMetadataAvailable::MetadataAvailable(*meta, key),
                        );
                    },
                    (&Some(Err(_)), _) | (&None, &None) => {
                        debug!("{} ({:?}) is still pending", url, key);
                        return ImageCacheResult::Pending(key);
                    },
                },
                CacheResult::Miss(Some((key, _pl))) => {
                    debug!("Should be requesting {} ({:?})", url, key);
                    return ImageCacheResult::ReadyForRequest(key);
                },
                CacheResult::Miss(None) => {
                    debug!("Couldn't find an entry for {}", url);
                    return ImageCacheResult::FailedToLoadOrDecode;
                },
            }
        };

        // In the case where a decode is ongoing (or waiting in a queue) but we
        // have the full response available, we decode the bytes synchronously
        // and ignore the async decode when it finishes later.
        // TODO: make this behaviour configurable according to the caller's needs.
        store.handle_decoder(decoded);
        match store.get_completed_image_if_available(url, origin, cors_setting) {
            Some(Ok((image, image_url))) => {
                ImageCacheResult::Available(ImageOrMetadataAvailable::ImageAvailable {
                    image,
                    url: image_url,
                })
            },
            // Note: this happens if we are pending a batch of image keys.
            _ => ImageCacheResult::Pending(key),
        }
    }

    fn get_vector_image_bytes(&self, image_id: PendingImageId) -> Option<Arc<Vec<u8>>> {
        self.store
            .lock()
            .vector_images
            .get(&image_id)
            .map(|vector_image| vector_image.bytes.clone())
    }

    /// Add a new listener for the given pending image id. If the image is already present,
    /// the responder will still receive the expected response.
    fn add_listener(&self, listener: ImageLoadListener) {
        let mut store = self.store.lock();
        self.add_listener_with_store(&mut store, listener);
    }

    fn evict_completed_image(
        &self,
        url: &BrowserUrl,
        origin: &ImmutableOrigin,
        cors_setting: &Option<CorsSettings>,
    ) {
        let mut store = self.store.lock();
        store.remove_loaded_image(url, origin, cors_setting);
    }

    /// Inform the image cache about a response for a pending request.
    fn notify_pending_response(&self, id: PendingImageId, action: FetchResponseMsg) {
        match (action, id) {
            (FetchResponseMsg::ProcessRequestBody(..), _) |
            (FetchResponseMsg::ProcessRequestEOF(..), _) |
            (FetchResponseMsg::ProcessCspViolations(..), _) => (),
            (FetchResponseMsg::ProcessResponse(_, response), _) => {
                debug!("Received {:?} for {:?}", response.as_ref().map(|_| ()), id);
                let mut store = self.store.lock();
                if let Some(pending_load) = store.pending_loads.get_by_key_mut(&id) {
                    let (cors_status, metadata) = match response {
                        Ok(meta) => match meta {
                            FetchMetadata::Unfiltered(m) => (CorsStatus::Safe, Some(m)),
                            FetchMetadata::Filtered { unsafe_, filtered } => (
                                match filtered {
                                    FilteredMetadata::Basic(_) | FilteredMetadata::Cors(_) => {
                                        CorsStatus::Safe
                                    },
                                    FilteredMetadata::Opaque |
                                    FilteredMetadata::OpaqueRedirect(_) => CorsStatus::Unsafe,
                                },
                                Some(unsafe_),
                            ),
                        },
                        Err(_) => (CorsStatus::Unsafe, None),
                    };
                    let final_url = metadata.as_ref().map(|m| m.final_url.clone());
                    pending_load.final_url = final_url;
                    pending_load.cors_status = cors_status;
                    pending_load.content_type = metadata
                        .as_ref()
                        .and_then(|metadata| metadata.content_type.clone())
                        .map(|content_type| content_type.into_inner().into());
                } else {
                    debug!("Pending load for id {:?} already evicted from cache", id);
                }
            },
            (FetchResponseMsg::ProcessResponseChunk(_, data), _) => {
                debug!("Got some data for {:?}", id);
                let mut store = self.store.lock();
                if let Some(pending_load) = store.pending_loads.get_by_key_mut(&id) {
                    pending_load.bytes.extend_from_slice(&data);

                    // jmr0 TODO: possibly move to another task?
                    if pending_load.metadata.is_none() {
                        let mut reader = std::io::Cursor::new(pending_load.bytes.as_slice());
                        if let Ok(info) = imsz_from_reader(&mut reader) {
                            let img_metadata = ImageMetadata {
                                width: info.width as u32,
                                height: info.height as u32,
                            };
                            for listener in &pending_load.listeners {
                                listener.respond(ImageResponse::MetadataLoaded(img_metadata));
                            }
                            pending_load.metadata = Some(img_metadata);
                        }
                    }
                } else {
                    debug!("Pending load for id {:?} already evicted from cache", id);
                }
            },
            (FetchResponseMsg::ProcessResponseEOF(_, result, _), key) => {
                debug!("Received EOF for {:?}", key);
                match result {
                    Ok(_) => {
                        let (bytes, cors_status, content_type) = {
                            let mut store = self.store.lock();
                            if let Some(pending_load) = store.pending_loads.get_by_key_mut(&id) {
                                pending_load.result = Some(Ok(()));
                                debug!("Async decoding {} ({:?})", pending_load.url, key);
                                (
                                    pending_load.bytes.mark_complete(),
                                    pending_load.cors_status,
                                    pending_load.content_type.clone(),
                                )
                            } else {
                                debug!("Pending load for id {:?} already evicted from cache", id);
                                return;
                            }
                        };

                        let local_store = self.store.clone();
                        self.thread_pool.spawn(move || {
                            let msg = decode_bytes_sync(key, &bytes, cors_status, content_type);
                            local_store.lock().handle_decoder(msg);
                        });
                    },
                    Err(error) => {
                        debug!("Processing error for {key:?}: {error:?}");
                        let mut store = self.store.lock();
                        store.complete_load(id, LoadResult::FailedToLoadOrDecode)
                    },
                }
            },
        }
    }

    fn fill_key_cache_with_batch_of_keys(&self, image_keys: Vec<WebRenderImageKey>) {
        let mut store = self.store.lock();
        store.insert_keys_and_load_images(image_keys);
    }

    fn get_broken_image_icon(&self) -> Option<Arc<RasterImage>> {
        let store = self.store.lock();
        store
            .broken_image_icon_image
            .get_or_init(|| {
                let mut image = load_from_memory(&self.broken_image_icon_data, CorsStatus::Unsafe)
                    .or_else(|| load_from_memory(FALLBACK_RIPPY, CorsStatus::Unsafe))?;
                let image_key = store
                    .paint_api
                    .generate_image_key_blocking(store.webview_id)
                    .expect("Could not generate image key for broken image icon");
                set_webrender_image_key(&store.paint_api, &mut image, image_key);
                Some(Arc::new(image))
            })
            .clone()
    }
}

impl Drop for ImageCacheStore {
    fn drop(&mut self) {
        let image_updates = self
            .completed_loads
            .values()
            .filter_map(|load| match &load.image_response {
                ImageResponse::Loaded(Image::Raster(image), _) => {
                    image.id.map(ImageUpdate::DeleteImage)
                },
                _ => None,
            })
            .collect();
        self.paint_api
            .update_images(self.webview_id.into(), image_updates);
    }
}

impl ImageCacheImpl {
    /// Require self.store.lock() before calling.
    fn add_listener_with_store(&self, store: &mut ImageCacheStore, listener: ImageLoadListener) {
        let id = listener.id;
        if let Some(load) = store.pending_loads.get_by_key_mut(&id) {
            if let Some(ref metadata) = load.metadata {
                listener.respond(ImageResponse::MetadataLoaded(*metadata));
            }
            load.add_listener(listener);
            return;
        }
        if let Some(load) = store.completed_loads.values().find(|l| l.id == id) {
            listener.respond(load.image_response.clone());
            return;
        }
        warn!("Couldn't find cached entry for listener {:?}", id);
    }
}

