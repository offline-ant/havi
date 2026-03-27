pub mod bounds;
pub mod dom;
pub mod foreign_object;
pub mod hit_test;
pub mod invalidation;
pub mod layout;
pub mod path;
pub mod resources;
pub mod style;
pub mod text;
pub mod transform;
pub mod use_expansion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGFirstCutTag {
    Svg,
    G,
    Path,
    Rect,
    Circle,
    Ellipse,
    Line,
    Polyline,
    Polygon,
    Defs,
    Use,
    LinearGradient,
    RadialGradient,
    Stop,
    ClipPath,
    ForeignObject,
    Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGFirstCutProperty {
    Transform,
    ViewBox,
    PreserveAspectRatio,
    SolidFillAndStroke,
    GradientPaint,
    CurrentColor,
    Display,
    Visibility,
    Opacity,
    PointerEvents,
    FillRule,
    ClipRule,
    NonScalingStroke,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGDeferredFeature {
    FullFilterCoverage,
    FullMaskCoverage,
    Markers,
    Patterns,
    Animation,
    TextPath,
    FullDomApiParity,
    StandaloneDocuments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGStandaloneDocumentSupport {
    InScope,
    Deferred,
}

#[derive(Clone, Copy, Debug)]
pub struct SVGFirstCutSupport {
    pub supported_tags: &'static [SVGFirstCutTag],
    pub supported_properties: &'static [SVGFirstCutProperty],
    pub deferred_features: &'static [SVGDeferredFeature],
    pub standalone_documents: SVGStandaloneDocumentSupport,
}

pub const FIRST_CUT_SUPPORTED_TAGS: &[SVGFirstCutTag] = &[
    SVGFirstCutTag::Svg,
    SVGFirstCutTag::G,
    SVGFirstCutTag::Path,
    SVGFirstCutTag::Rect,
    SVGFirstCutTag::Circle,
    SVGFirstCutTag::Ellipse,
    SVGFirstCutTag::Line,
    SVGFirstCutTag::Polyline,
    SVGFirstCutTag::Polygon,
    SVGFirstCutTag::Defs,
    SVGFirstCutTag::Use,
    SVGFirstCutTag::LinearGradient,
    SVGFirstCutTag::RadialGradient,
    SVGFirstCutTag::Stop,
    SVGFirstCutTag::ClipPath,
    SVGFirstCutTag::ForeignObject,
    SVGFirstCutTag::Image,
];

pub const FIRST_CUT_SUPPORTED_PROPERTIES: &[SVGFirstCutProperty] = &[
    SVGFirstCutProperty::Transform,
    SVGFirstCutProperty::ViewBox,
    SVGFirstCutProperty::PreserveAspectRatio,
    SVGFirstCutProperty::SolidFillAndStroke,
    SVGFirstCutProperty::GradientPaint,
    SVGFirstCutProperty::CurrentColor,
    SVGFirstCutProperty::Display,
    SVGFirstCutProperty::Visibility,
    SVGFirstCutProperty::Opacity,
    SVGFirstCutProperty::PointerEvents,
    SVGFirstCutProperty::FillRule,
    SVGFirstCutProperty::ClipRule,
    SVGFirstCutProperty::NonScalingStroke,
];

pub const FIRST_CUT_DEFERRED_FEATURES: &[SVGDeferredFeature] = &[
    SVGDeferredFeature::FullFilterCoverage,
    SVGDeferredFeature::FullMaskCoverage,
    SVGDeferredFeature::Markers,
    SVGDeferredFeature::Patterns,
    SVGDeferredFeature::Animation,
    SVGDeferredFeature::TextPath,
    SVGDeferredFeature::FullDomApiParity,
    SVGDeferredFeature::StandaloneDocuments,
];

pub const FIRST_CUT_SUPPORT: SVGFirstCutSupport = SVGFirstCutSupport {
    supported_tags: FIRST_CUT_SUPPORTED_TAGS,
    supported_properties: FIRST_CUT_SUPPORTED_PROPERTIES,
    deferred_features: FIRST_CUT_DEFERRED_FEATURES,
    standalone_documents: SVGStandaloneDocumentSupport::Deferred,
};
