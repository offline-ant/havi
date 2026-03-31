use std::cell::RefCell;

use rustc_hash::FxHashMap;
use xml5ever::TokenizerResult;
use xml5ever::buffer_queue::BufferQueue;
use xml5ever::tendril::StrTendril;
use xml5ever::tokenizer::{ProcessResult, Tag, TagKind, Token, TokenSink, XmlTokenizer};

use super::tree::{SVGNodeData, SVGNodeId, SVGStandaloneTree, SVGTreeChild, SVGTreeNode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SVGParseError {
    NotSvgDocument,
    MalformedXml(String),
}

#[derive(Clone, Debug)]
pub struct SVGRootMetadata {
    pub viewport: layout_api::SVGViewportData,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SVGImageIntrinsicSizes {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub ratio: Option<f32>,
}

pub fn compute_svg_image_intrinsic_sizes(
    viewport: &layout_api::SVGViewportData,
) -> SVGImageIntrinsicSizes {
    let width = viewport
        .width
        .and_then(layout_api::resolve_svg_length_to_user_units)
        .filter(|value| *value > 0.0);
    let height = viewport
        .height
        .and_then(layout_api::resolve_svg_length_to_user_units)
        .filter(|value| *value > 0.0);
    let ratio = match (width, height) {
        (Some(width), Some(height)) => Some(width / height),
        _ => viewport
            .view_box
            .filter(|view_box| view_box.width > 0.0 && view_box.height > 0.0)
            .map(|view_box| view_box.width / view_box.height),
    };
    SVGImageIntrinsicSizes { width, height, ratio }
}

pub fn parse_svg_tree(bytes: &[u8]) -> Result<SVGStandaloneTree, SVGParseError> {
    let source = std::str::from_utf8(bytes)
        .map_err(|error| SVGParseError::MalformedXml(error.to_string()))?;
    let sink = SVGTreeSink::default();
    run_xml_tokenizer(source, sink)?.into_tree()
}

pub fn extract_svg_root_metadata(bytes: &[u8]) -> Result<SVGRootMetadata, SVGParseError> {
    let source = std::str::from_utf8(bytes)
        .map_err(|error| SVGParseError::MalformedXml(error.to_string()))?;
    let sink = SVGMetadataSink::default();
    run_xml_tokenizer(source, sink)?.into_metadata()
}

fn run_xml_tokenizer<S: TokenSink>(
    source: &str,
    sink: S,
) -> Result<S, SVGParseError> {
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(source));
    let tokenizer = XmlTokenizer::new(sink, Default::default());
    match tokenizer.feed(&input) {
        TokenizerResult::Done => {}
        TokenizerResult::Script(_) | TokenizerResult::EncodingIndicator(_) => {}
    }
    tokenizer.end();
    Ok(tokenizer.sink)
}

#[derive(Clone, Debug, Default)]
struct SVGMetadataSink {
    metadata: RefCell<Option<SVGRootMetadata>>,
    error: RefCell<Option<SVGParseError>>,
}

impl SVGMetadataSink {
    fn into_metadata(self) -> Result<SVGRootMetadata, SVGParseError> {
        if let Some(error) = self.error.into_inner() {
            return Err(error);
        }
        self.metadata.into_inner().ok_or(SVGParseError::NotSvgDocument)
    }
}

impl TokenSink for SVGMetadataSink {
    type Handle = ();

    fn process_token(&self, token: Token) -> ProcessResult<Self::Handle> {
        if self.metadata.borrow().is_some() || self.error.borrow().is_some() {
            return ProcessResult::Done;
        }
        match token {
            Token::ParseError(error) => {
                *self.error.borrow_mut() = Some(SVGParseError::MalformedXml(error.into_owned()));
                ProcessResult::Done
            }
            Token::Tag(tag) if matches!(tag.kind, TagKind::StartTag | TagKind::EmptyTag) => {
                let local = tag.name.local.to_string();
                if local != "svg" {
                    *self.error.borrow_mut() = Some(SVGParseError::NotSvgDocument);
                    return ProcessResult::Done;
                }
                let attrs = SVGAttributeMap::from_tag(&tag);
                *self.metadata.borrow_mut() = Some(SVGRootMetadata {
                    viewport: layout_api::SVGViewportData {
                        width: attrs.typed_length("width"),
                        height: attrs.typed_length("height"),
                        view_box: layout_api::parse_svg_optional_view_box(attrs.attr("viewBox")),
                        preserve_aspect_ratio: layout_api::parse_svg_preserve_aspect_ratio(
                            attrs.attr("preserveAspectRatio"),
                        ),
                        overflow_hidden: layout_api::parse_svg_overflow_hidden(attrs.attr("overflow")),
                    },
                });
                ProcessResult::Done
            }
            Token::EndOfFile => ProcessResult::Done,
            _ => ProcessResult::Continue,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SVGTreeSink {
    state: RefCell<SVGTreeParseState>,
}

impl SVGTreeSink {
    fn into_tree(self) -> Result<SVGStandaloneTree, SVGParseError> {
        self.state.into_inner().into_tree()
    }
}

impl TokenSink for SVGTreeSink {
    type Handle = ();

    fn process_token(&self, token: Token) -> ProcessResult<Self::Handle> {
        let mut state = self.state.borrow_mut();
        if state.error.is_some() {
            return ProcessResult::Done;
        }
        match token {
            Token::Tag(tag) => state.process_tag(tag),
            Token::Characters(text) => {
                state.push_text(text.to_string());
                ProcessResult::Continue
            }
            Token::ParseError(error) => {
                state.error = Some(SVGParseError::MalformedXml(error.into_owned()));
                ProcessResult::Done
            }
            Token::EndOfFile => {
                state.finish();
                ProcessResult::Done
            }
            Token::NullCharacter => {
                state.push_text("\0".to_owned());
                ProcessResult::Continue
            }
            _ => ProcessResult::Continue,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SVGTreeParseState {
    next_id: usize,
    root: Option<SVGTreeNode>,
    stack: Vec<OpenElement>,
    error: Option<SVGParseError>,
}

impl SVGTreeParseState {
    fn into_tree(self) -> Result<SVGStandaloneTree, SVGParseError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        self.root.map(SVGStandaloneTree::new).ok_or(SVGParseError::NotSvgDocument)
    }

    fn finish(&mut self) {
        if self.error.is_some() {
            return;
        }
        while let Some(open) = self.stack.pop() {
            self.attach_completed(open);
        }
        if self.root.is_none() {
            self.error = Some(SVGParseError::NotSvgDocument);
        }
    }

    fn process_tag(&mut self, tag: Tag) -> ProcessResult<()> {
        match tag.kind {
            TagKind::StartTag => {
                self.start_element(tag);
                ProcessResult::Continue
            }
            TagKind::EmptyTag => {
                self.start_element(tag.clone());
                if self.error.is_some() {
                    return ProcessResult::Done;
                }
                self.end_element(&tag.name.local.to_string());
                if self.error.is_some() {
                    ProcessResult::Done
                } else {
                    ProcessResult::Continue
                }
            }
            TagKind::EndTag => {
                self.end_element(&tag.name.local.to_string());
                if self.error.is_some() {
                    ProcessResult::Done
                } else {
                    ProcessResult::Continue
                }
            }
            TagKind::ShortTag => ProcessResult::Continue,
        }
    }

    fn start_element(&mut self, tag: Tag) {
        if self.error.is_some() {
            return;
        }
        let local = tag.name.local.to_string();
        let attrs = SVGAttributeMap::from_tag(&tag);
        let parent_allows_children = self
            .stack
            .last()
            .map(|open| open.capture_children)
            .unwrap_or(true);
        if !parent_allows_children {
            self.stack.push(OpenElement::skipped(local));
            return;
        }

        let node = if self.stack.is_empty() {
            if local != "svg" {
                self.error = Some(SVGParseError::NotSvgDocument);
                return;
            }
            Some(parse_svg_node(self.alloc_id(), &local, &attrs))
        } else {
            parse_svg_node_if_supported(self.alloc_id(), &local, &attrs)
        };
        let capture_children = node
            .as_ref()
            .is_some_and(|node| node_captures_children(&node.data));
        self.stack.push(OpenElement {
            local_name: local,
            node,
            capture_children,
        });
    }

    fn end_element(&mut self, local_name: &str) {
        let Some(open) = self.stack.pop() else {
            self.error = Some(SVGParseError::MalformedXml(format!(
                "unexpected end tag </{local_name}>"
            )));
            return;
        };
        if open.local_name != local_name {
            self.error = Some(SVGParseError::MalformedXml(format!(
                "mismatched end tag </{local_name}> for <{}>",
                open.local_name
            )));
            return;
        }
        self.attach_completed(open);
    }

    fn attach_completed(&mut self, open: OpenElement) {
        let Some(node) = open.node else {
            return;
        };
        if let Some(parent) = self
            .stack
            .iter_mut()
            .rev()
            .find_map(|open| open.node.as_mut())
        {
            parent.children.push(SVGTreeChild::Node(node));
        } else if self.root.is_none() {
            self.root = Some(node);
        } else {
            self.error = Some(SVGParseError::MalformedXml(
                "multiple root SVG elements".to_owned(),
            ));
        }
    }

    fn push_text(&mut self, text: String) {
        if text.is_empty() || self.error.is_some() {
            return;
        }
        let Some(open) = self.stack.last_mut() else {
            return;
        };
        if !open.capture_children {
            return;
        }
        let Some(node) = open.node.as_mut() else {
            return;
        };
        node.children.push(SVGTreeChild::Text(text));
    }

    fn alloc_id(&mut self) -> SVGNodeId {
        let id = SVGNodeId(self.next_id);
        self.next_id += 1;
        id
    }
}

#[derive(Clone, Debug)]
struct OpenElement {
    local_name: String,
    node: Option<SVGTreeNode>,
    capture_children: bool,
}

impl OpenElement {
    fn skipped(local_name: String) -> Self {
        Self {
            local_name,
            node: None,
            capture_children: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SVGAttributeMap {
    attrs: FxHashMap<String, String>,
}

impl SVGAttributeMap {
    fn from_tag(tag: &Tag) -> Self {
        let mut attrs = FxHashMap::default();
        for attr in &tag.attrs {
            attrs.insert(attr.name.local.to_string(), attr.value.to_string());
        }
        Self { attrs }
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    fn style_property(&self, name: &str) -> Option<&str> {
        let style = self.attr("style")?;
        style.rsplit(';').find_map(|declaration| {
            let (prop, value) = declaration.split_once(':')?;
            (prop.trim().eq_ignore_ascii_case(name)).then_some(value.trim())
        })
    }

    fn attr_or_style(&self, name: &str) -> Option<&str> {
        self.attr(name).or_else(|| self.style_property(name))
    }

    fn typed_length(&self, name: &str) -> Option<layout_api::SVGLengthValue> {
        self.attr(name).map(|raw| layout_api::parse_svg_length(Some(raw)))
    }

    fn typed_reference(&self) -> Option<layout_api::SVGReferenceValue<'_>> {
        self.attr("href")
            .or_else(|| self.attr("xlink:href"))
            .and_then(|raw| layout_api::parse_svg_reference(Some(raw)))
    }

    fn typed_text(&self) -> layout_api::SVGTextData {
        layout_api::SVGTextData {
            x: layout_api::parse_svg_length_list(self.attr("x")),
            y: layout_api::parse_svg_length_list(self.attr("y")),
            dx: layout_api::parse_svg_length_list(self.attr("dx")),
            dy: layout_api::parse_svg_length_list(self.attr("dy")),
            rotate: layout_api::parse_svg_number_list(self.attr("rotate")),
            font_size: self.typed_length("font-size"),
            text_length: self.typed_length("textLength"),
            length_adjust: layout_api::parse_svg_length_adjust(self.attr("lengthAdjust")),
            text_anchor: layout_api::parse_svg_text_anchor(self.attr("text-anchor")),
            alignment_baseline: layout_api::parse_svg_baseline(self.attr("alignment-baseline")),
            dominant_baseline: layout_api::parse_svg_baseline(self.attr("dominant-baseline")),
        }
    }

    fn paint_data(&self) -> layout_api::SVGPaintData<'_> {
        layout_api::SVGPaintData {
            color: self.attr_or_style("color"),
            fill: self.attr_or_style("fill"),
            fill_opacity: layout_api::parse_svg_unit_interval(self.attr_or_style("fill-opacity")),
            fill_rule: layout_api::parse_svg_fill_rule(self.attr_or_style("fill-rule")),
            stroke: self.attr_or_style("stroke"),
            stroke_opacity: layout_api::parse_svg_unit_interval(self.attr_or_style("stroke-opacity")),
            stroke_width: self.typed_length("stroke-width"),
            stroke_linejoin: layout_api::parse_svg_line_join(self.attr_or_style("stroke-linejoin")),
            stroke_linecap: layout_api::parse_svg_line_cap(self.attr_or_style("stroke-linecap")),
            stroke_miterlimit: layout_api::parse_svg_non_negative_number(self.attr_or_style("stroke-miterlimit")),
            stroke_dasharray: layout_api::parse_svg_dash_array(self.attr_or_style("stroke-dasharray")),
            stroke_dashoffset: layout_api::parse_svg_optional_number(self.attr_or_style("stroke-dashoffset")),
            paint_order: layout_api::parse_svg_paint_order(self.attr_or_style("paint-order")),
            opacity: layout_api::parse_svg_unit_interval(self.attr_or_style("opacity")),
            pointer_events: layout_api::parse_svg_pointer_events(self.attr_or_style("pointer-events")),
            vector_effect: layout_api::parse_svg_vector_effect(self.attr_or_style("vector-effect")),
            clip_rule: layout_api::parse_svg_fill_rule(self.attr_or_style("clip-rule")),
            clip_path: layout_api::parse_svg_reference(self.attr_or_style("clip-path")),
            mask: layout_api::parse_svg_reference(self.attr_or_style("mask")),
            filter: layout_api::parse_svg_reference(self.attr_or_style("filter")),
            marker_start: layout_api::parse_svg_reference(self.attr_or_style("marker-start")),
            marker_mid: layout_api::parse_svg_reference(self.attr_or_style("marker-mid")),
            marker_end: layout_api::parse_svg_reference(self.attr_or_style("marker-end")),
        }
    }
}

fn parse_svg_node_if_supported(
    id: SVGNodeId,
    local_name: &str,
    attrs: &SVGAttributeMap,
) -> Option<SVGTreeNode> {
    Some(parse_svg_node(id, local_name, attrs)).filter(|node| {
        !matches!(node.data.node_kind, super::tree::SVGOwnedNodeKind::Defs)
            || local_name == "defs"
    })
}

fn parse_svg_node(id: SVGNodeId, local_name: &str, attrs: &SVGAttributeMap) -> SVGTreeNode {
    let common = layout_api::SVGCommonData {
        element_id: attrs.attr("id"),
        transform: layout_api::parse_svg_transform_list(attrs.attr("transform")),
    };
    let paint = attrs.paint_data();
    let node_kind = match local_name {
        "svg" => layout_api::SVGNodeKind::Viewport(layout_api::SVGViewportData {
            width: attrs.typed_length("width"),
            height: attrs.typed_length("height"),
            view_box: layout_api::parse_svg_optional_view_box(attrs.attr("viewBox")),
            preserve_aspect_ratio: layout_api::parse_svg_preserve_aspect_ratio(
                attrs.attr("preserveAspectRatio"),
            ),
            overflow_hidden: layout_api::parse_svg_overflow_hidden(attrs.attr("overflow")),
        }),
        "g" => layout_api::SVGNodeKind::Group,
        "defs" => layout_api::SVGNodeKind::Defs,
        "path" => layout_api::SVGNodeKind::Geometry(layout_api::SVGGeometryData::Path {
            d: attrs.attr("d"),
        }),
        "rect" => layout_api::SVGNodeKind::Geometry(layout_api::SVGGeometryData::Rect {
            x: attrs.typed_length("x"),
            y: attrs.typed_length("y"),
            width: attrs.typed_length("width"),
            height: attrs.typed_length("height"),
            rx: attrs.typed_length("rx"),
            ry: attrs.typed_length("ry"),
        }),
        "circle" => layout_api::SVGNodeKind::Geometry(layout_api::SVGGeometryData::Circle {
            cx: attrs.typed_length("cx"),
            cy: attrs.typed_length("cy"),
            r: attrs.typed_length("r"),
        }),
        "ellipse" => layout_api::SVGNodeKind::Geometry(layout_api::SVGGeometryData::Ellipse {
            cx: attrs.typed_length("cx"),
            cy: attrs.typed_length("cy"),
            rx: attrs.typed_length("rx"),
            ry: attrs.typed_length("ry"),
        }),
        "line" => layout_api::SVGNodeKind::Geometry(layout_api::SVGGeometryData::Line {
            x1: attrs.typed_length("x1"),
            y1: attrs.typed_length("y1"),
            x2: attrs.typed_length("x2"),
            y2: attrs.typed_length("y2"),
        }),
        "polyline" => layout_api::SVGNodeKind::Geometry(layout_api::SVGGeometryData::Polyline {
            points: attrs.attr("points"),
        }),
        "polygon" => layout_api::SVGNodeKind::Geometry(layout_api::SVGGeometryData::Polygon {
            points: attrs.attr("points"),
        }),
        "text" => layout_api::SVGNodeKind::Text(attrs.typed_text()),
        "tspan" => layout_api::SVGNodeKind::TSpan(attrs.typed_text()),
        "use" => layout_api::SVGNodeKind::Use(layout_api::SVGUseData {
            href: attrs.typed_reference(),
            x: attrs.typed_length("x"),
            y: attrs.typed_length("y"),
            width: attrs.typed_length("width"),
            height: attrs.typed_length("height"),
        }),
        "foreignObject" => {
            layout_api::SVGNodeKind::ForeignObject(layout_api::SVGForeignObjectData {
                x: attrs.typed_length("x"),
                y: attrs.typed_length("y"),
                width: attrs.typed_length("width"),
                height: attrs.typed_length("height"),
            })
        }
        "linearGradient" => layout_api::SVGNodeKind::Gradient(layout_api::SVGGradientData::Linear {
            href: attrs.typed_reference(),
            x1: attrs.typed_length("x1"),
            y1: attrs.typed_length("y1"),
            x2: attrs.typed_length("x2"),
            y2: attrs.typed_length("y2"),
            gradient_units: layout_api::parse_svg_coordinate_units(attrs.attr("gradientUnits")),
            gradient_transform: layout_api::parse_svg_transform_list(attrs.attr("gradientTransform")),
            spread_method: layout_api::parse_svg_spread_method(attrs.attr("spreadMethod")),
        }),
        "radialGradient" => layout_api::SVGNodeKind::Gradient(layout_api::SVGGradientData::Radial {
            href: attrs.typed_reference(),
            cx: attrs.typed_length("cx"),
            cy: attrs.typed_length("cy"),
            r: attrs.typed_length("r"),
            fx: attrs.typed_length("fx"),
            fy: attrs.typed_length("fy"),
            fr: attrs.typed_length("fr"),
            gradient_units: layout_api::parse_svg_coordinate_units(attrs.attr("gradientUnits")),
            gradient_transform: layout_api::parse_svg_transform_list(attrs.attr("gradientTransform")),
            spread_method: layout_api::parse_svg_spread_method(attrs.attr("spreadMethod")),
        }),
        "stop" => layout_api::SVGNodeKind::Stop(layout_api::SVGStopData {
            offset: layout_api::parse_svg_stop_offset(attrs.attr("offset")),
            stop_color: attrs.attr_or_style("stop-color"),
            stop_opacity: attrs.attr_or_style("stop-opacity"),
        }),
        "clipPath" => layout_api::SVGNodeKind::ClipPath(layout_api::SVGClipPathData {
            clip_path_units: layout_api::parse_svg_coordinate_units(attrs.attr("clipPathUnits")),
        }),
        "mask" => layout_api::SVGNodeKind::Mask(layout_api::SVGMaskData {
            x: attrs.typed_length("x"),
            y: attrs.typed_length("y"),
            width: attrs.typed_length("width"),
            height: attrs.typed_length("height"),
            mask_units: layout_api::parse_svg_coordinate_units(attrs.attr("maskUnits")),
            mask_content_units: layout_api::parse_svg_coordinate_units(attrs.attr("maskContentUnits")),
        }),
        "image" => layout_api::SVGNodeKind::Image(layout_api::SVGImageData {
            href: attrs.typed_reference(),
            x: attrs.typed_length("x"),
            y: attrs.typed_length("y"),
            width: attrs.typed_length("width"),
            height: attrs.typed_length("height"),
            preserve_aspect_ratio: layout_api::parse_svg_preserve_aspect_ratio(
                attrs.attr("preserveAspectRatio"),
            ),
        }),
        "pattern" => layout_api::SVGNodeKind::Pattern(layout_api::SVGPatternData {
            href: attrs.typed_reference(),
            x: attrs.typed_length("x"),
            y: attrs.typed_length("y"),
            width: attrs.typed_length("width"),
            height: attrs.typed_length("height"),
            pattern_units: layout_api::parse_svg_coordinate_units(attrs.attr("patternUnits")),
            pattern_content_units: layout_api::parse_svg_coordinate_units(attrs.attr("patternContentUnits")),
            pattern_transform: layout_api::parse_svg_transform_list(attrs.attr("patternTransform")),
            view_box: layout_api::parse_svg_optional_view_box(attrs.attr("viewBox")),
            preserve_aspect_ratio: layout_api::parse_svg_preserve_aspect_ratio(
                attrs.attr("preserveAspectRatio"),
            ),
        }),
        "filter" => layout_api::SVGNodeKind::Filter(layout_api::SVGFilterData {
            x: attrs.typed_length("x"),
            y: attrs.typed_length("y"),
            width: attrs.typed_length("width"),
            height: attrs.typed_length("height"),
            filter_units: layout_api::parse_svg_coordinate_units(attrs.attr("filterUnits")),
            primitive_units: layout_api::parse_svg_coordinate_units(attrs.attr("primitiveUnits")),
        }),
        "marker" => layout_api::SVGNodeKind::Marker(layout_api::SVGMarkerData {
            ref_x: attrs.typed_length("refX"),
            ref_y: attrs.typed_length("refY"),
            marker_width: attrs.typed_length("markerWidth"),
            marker_height: attrs.typed_length("markerHeight"),
            marker_units: layout_api::parse_svg_marker_units(attrs.attr("markerUnits")),
            orient_auto: attrs.attr("orient").is_some_and(|raw| raw.trim().starts_with("auto")),
            view_box: layout_api::parse_svg_optional_view_box(attrs.attr("viewBox")),
            preserve_aspect_ratio: layout_api::parse_svg_preserve_aspect_ratio(
                attrs.attr("preserveAspectRatio"),
            ),
        }),
        "textPath" => layout_api::SVGNodeKind::TextPath(layout_api::SVGTextPathData {
            href: attrs.typed_reference(),
            start_offset: attrs.attr("startOffset").map(|raw| layout_api::parse_svg_length(Some(raw))),
            text: attrs.typed_text(),
        }),
        _ => return SVGTreeNode::new(id, SVGNodeData::from(layout_api::SVGElementData {
            common,
            node_kind: layout_api::SVGNodeKind::Defs,
            paint,
        }), Vec::new()),
    };

    SVGTreeNode::new(
        id,
        SVGNodeData::from(layout_api::SVGElementData {
            common,
            node_kind,
            paint,
        }),
        Vec::new(),
    )
}

fn node_captures_children(data: &SVGNodeData) -> bool {
    matches!(
        data.node_kind,
        super::tree::SVGOwnedNodeKind::Viewport(_)
            | super::tree::SVGOwnedNodeKind::Group
            | super::tree::SVGOwnedNodeKind::Defs
            | super::tree::SVGOwnedNodeKind::Gradient(_)
            | super::tree::SVGOwnedNodeKind::ClipPath(_)
            | super::tree::SVGOwnedNodeKind::Mask(_)
            | super::tree::SVGOwnedNodeKind::Pattern(_)
            | super::tree::SVGOwnedNodeKind::Marker(_)
            | super::tree::SVGOwnedNodeKind::Text(_)
            | super::tree::SVGOwnedNodeKind::TSpan(_)
            | super::tree::SVGOwnedNodeKind::TextPath(_)
    )
}

#[cfg(test)]
mod tests {
    use servo_arc::Arc as ServoArc;
    use style::properties::ComputedValues;
    use style::properties::style_structs::Font;

    use super::*;
    use super::svg::dom::{SVGLayoutNodeKind, SVGNodeResolvedStyle};
    use super::svg::path::{decorated_bounds, normalize_svg_geometry, path_bounds, transform_svg_path_data};
    use super::svg::transform::parse_svg_transform;

    fn initial_style() -> ServoArc<ComputedValues> {
        ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
    }

    #[test]
    fn parses_svg_tree_with_transform_and_text_content() {
        let tree = parse_svg_tree(
            br#"<svg xmlns='http://www.w3.org/2000/svg' width='120' height='80' viewBox='0 0 120 80'><g transform='translate(5 7)'><rect width='40' height='30' transform='translate(20 10)' fill='green'/><text x='1' y='2'>hi</text></g></svg>"#,
        )
        .expect("tree");
        assert_eq!(tree.root.children.len(), 1);
        let SVGTreeChild::Node(group) = &tree.root.children[0] else {
            panic!("expected group node");
        };
        assert_eq!(group.children.len(), 2);
        let SVGTreeChild::Node(text) = &group.children[1] else {
            panic!("expected text node");
        };
        assert!(matches!(text.data.node_kind, super::super::tree::SVGOwnedNodeKind::Text(_)));
        let SVGTreeChild::Text(text_content) = &text.children[0] else {
            panic!("expected text content");
        };
        assert_eq!(text_content, "hi");
    }

    #[test]
    fn parsed_tree_builds_translated_leaf_geometry() {
        let tree = parse_svg_tree(
            br#"<svg xmlns='http://www.w3.org/2000/svg' width='120' height='80' viewBox='0 0 120 80'><rect width='40' height='30' transform='translate(20 10)' fill='green'/></svg>"#,
        )
        .expect("tree");
        let resolved = tree.resolve_with_default_style(initial_style());
        let child = resolved.element_children().next().expect("leaf child");
        let svg_data = child.svg_data();
        let (geometry, style) = match (&svg_data.node_kind, &child.resolved_style) {
            (layout_api::SVGNodeKind::Geometry(geometry), SVGNodeResolvedStyle::Geometry(style)) => {
                (geometry, style)
            }
            other => panic!("expected geometry leaf, got {other:?}"),
        };
        let path: havi_types::fragment_tree::SVGPathData = normalize_svg_geometry(geometry, style.fill_rule).into();
        let transformed_path = transform_svg_path_data(
            &path,
            parse_svg_transform(&child.svg_data().common.transform),
        );
        let bounds = decorated_bounds(&transformed_path, style.paint.stroke.as_ref())
            .or_else(|| path_bounds(&transformed_path))
            .expect("path bounds");
        assert_eq!(child.summary.kind, SVGLayoutNodeKind::Geometry);
        assert_eq!(bounds.origin.x, 20.0);
        assert_eq!(bounds.origin.y, 10.0);
        assert_eq!(bounds.size.width, 40.0);
        assert_eq!(bounds.size.height, 30.0);
    }

    #[test]
    fn extracts_root_metadata_from_viewbox_only_svg() {
        let metadata = extract_svg_root_metadata(
            br#"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 60 30'><rect width='60' height='30'/></svg>"#,
        )
        .expect("metadata");
        assert_eq!(metadata.viewport.width, None);
        assert_eq!(metadata.viewport.height, None);
        assert_eq!(
            metadata.viewport.view_box,
            Some(layout_api::SVGRectValue {
                x: 0.0,
                y: 0.0,
                width: 60.0,
                height: 30.0,
            })
        );
    }

    #[test]
    fn image_intrinsic_sizes_use_only_explicit_root_dimensions() {
        let viewbox_only = compute_svg_image_intrinsic_sizes(&layout_api::SVGViewportData {
            width: None,
            height: None,
            view_box: Some(layout_api::SVGRectValue {
                x: 0.0,
                y: 0.0,
                width: 60.0,
                height: 30.0,
            }),
            preserve_aspect_ratio: Default::default(),
            overflow_hidden: false,
        });
        assert_eq!(
            viewbox_only,
            SVGImageIntrinsicSizes {
                width: None,
                height: None,
                ratio: Some(2.0),
            }
        );

        let explicit_size = compute_svg_image_intrinsic_sizes(&layout_api::SVGViewportData {
            width: Some(layout_api::parse_svg_length(Some("80"))),
            height: Some(layout_api::parse_svg_length(Some("40"))),
            view_box: Some(layout_api::SVGRectValue {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 10.0,
            }),
            preserve_aspect_ratio: Default::default(),
            overflow_hidden: false,
        });
        assert_eq!(
            explicit_size,
            SVGImageIntrinsicSizes {
                width: Some(80.0),
                height: Some(40.0),
                ratio: Some(2.0),
            }
        );
    }
}
