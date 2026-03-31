use std::cell::RefCell;

use rustc_hash::FxHashMap;
use xml5ever::TokenizerResult;
use xml5ever::buffer_queue::BufferQueue;
use xml5ever::tendril::StrTendril;
use xml5ever::tokenizer::{ProcessResult, Tag, TagKind, Token, TokenSink, XmlTokenizer};

use crate::layout::{
    SVGLengthValue, SVGViewportData, parse_svg_length, parse_svg_optional_view_box,
    parse_svg_overflow_hidden, parse_svg_preserve_aspect_ratio,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SVGParseError {
    NotSvgDocument,
    MalformedXml(String),
}

#[derive(Clone, Debug)]
pub struct SVGRootMetadata {
    pub viewport: SVGViewportData,
}

pub fn extract_svg_root_metadata(bytes: &[u8]) -> Result<SVGRootMetadata, SVGParseError> {
    let source = std::str::from_utf8(bytes)
        .map_err(|error| SVGParseError::MalformedXml(error.to_string()))?;
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(source));
    let tokenizer = XmlTokenizer::new(SVGMetadataSink::default(), Default::default());
    match tokenizer.feed(&input) {
        TokenizerResult::Done => {}
        TokenizerResult::Script(_) | TokenizerResult::EncodingIndicator(_) => {}
    }
    tokenizer.end();
    tokenizer.sink.into_metadata()
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

    fn typed_length(&self, name: &str) -> Option<SVGLengthValue> {
        self.attr(name).map(|raw| parse_svg_length(Some(raw)))
    }
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
                if tag.name.local.to_string() != "svg" {
                    *self.error.borrow_mut() = Some(SVGParseError::NotSvgDocument);
                    return ProcessResult::Done;
                }
                let attrs = SVGAttributeMap::from_tag(&tag);
                *self.metadata.borrow_mut() = Some(SVGRootMetadata {
                    viewport: SVGViewportData {
                        width: attrs.typed_length("width"),
                        height: attrs.typed_length("height"),
                        view_box: parse_svg_optional_view_box(attrs.attr("viewBox")),
                        preserve_aspect_ratio: parse_svg_preserve_aspect_ratio(
                            attrs.attr("preserveAspectRatio"),
                        ),
                        overflow_hidden: parse_svg_overflow_hidden(attrs.attr("overflow")),
                    },
                });
                ProcessResult::Done
            }
            Token::EndOfFile => ProcessResult::Done,
            _ => ProcessResult::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_root_viewport_metadata() {
        let metadata = extract_svg_root_metadata(
            br#"<svg xmlns='http://www.w3.org/2000/svg' width='80' height='40' viewBox='0 0 20 10' preserveAspectRatio='none'></svg>"#,
        )
        .expect("metadata");
        assert_eq!(metadata.viewport.width.map(|value| value.value), Some(80.0));
        assert_eq!(metadata.viewport.height.map(|value| value.value), Some(40.0));
        assert_eq!(
            metadata.viewport.view_box,
            Some(crate::SVGRectValue {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 10.0,
            })
        );
        assert_eq!(metadata.viewport.preserve_aspect_ratio.align, crate::SVG_PRESERVEASPECTRATIO_NONE);
    }

    #[test]
    fn rejects_non_svg_root() {
        let error = extract_svg_root_metadata(br#"<html></html>"#).unwrap_err();
        assert!(matches!(error, SVGParseError::NotSvgDocument));
    }
}
