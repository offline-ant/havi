/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! This module contains traits in script used generically in the rest of Servo.
//! The traits are here instead of in script so that these modules won't have
//! to depend on script.

#![deny(unsafe_code)]

mod layout_damage;
mod svg_parse;
mod svg_values;
pub mod wrapper_traits;

pub use svg_parse::*;
pub use svg_values::*;

use std::any::Any;
use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use app_units::Au;
use atomic_refcell::AtomicRefCell;
use background_hang_monitor_api::BackgroundHangMonitorRegister;
use base::Epoch;
use base::generic_channel::GenericSender;
use base::id::{BrowsingContextId, PipelineId, WebViewId};
use bitflags::bitflags;
use embedder_traits::{Cursor, Theme, UntrustedNodeAddress, ViewportDetails};
use euclid::{Point2D, Rect, Transform2D};
use fonts::{FontContext, WebFontDocumentContext};
use havi_types::fragment_tree::{
    SVGCoordinateUnits, SVGFillRule, SVGGradientSpreadMethod, SVGLineCap, SVGLineJoin,
    SVGPaintOrder, SVGTextAnchor, SVGVectorEffect,
};
pub use layout_damage::LayoutDamage;
use libc::c_void;
use malloc_size_of::{MallocSizeOf as MallocSizeOfTrait, MallocSizeOfOps, malloc_size_of_is_0};
use malloc_size_of_derive::MallocSizeOf;
use net_traits::image_cache::{ImageCache, ImageCacheFactory, PendingImageId};
use paint_api::CrossProcessPaintApi;
use parking_lot::RwLock;
use pixels::RasterImage;
use profile_traits::mem::Report;
use profile_traits::time;
use rustc_hash::FxHashMap;
use script_traits::{InitialScriptState, Painter, ScriptThreadMessage};
use serde::{Deserialize, Serialize};
use servo_arc::Arc as ServoArc;
use servo_url::{BrowserUrl, ImmutableOrigin};
use style::Atom;
use style::animation::DocumentAnimationSet;
use style::context::QuirksMode;
use style::data::ElementData;
use style::dom::OpaqueNode;
use style::invalidation::element::restyle_hints::RestyleHint;
use style::media_queries::Device;
use style::properties::style_structs::Font;
use style::properties::{ComputedValues, PropertyId};
use style::selector_parser::{PseudoElement, RestyleDamage, Snapshot};
use style::stylesheets::{DocumentStyleSheet, Stylesheet, UrlExtraData};
use style::thread_state::{self, ThreadState};
use style::values::computed::Overflow;
use style_traits::CSSPixel;
use webrender_api::units::{LayoutPoint, LayoutVector2D};
use webrender_api::{ExternalScrollId, ImageKey};

/// Thread-safe container for sharing layout fragment payloads with the render
/// pipeline.
///
/// `layout_api` cannot depend on the concrete `layout` crate, so the payload is
/// type-erased here and downcast by the render side.
#[derive(Clone)]
struct SharedLayoutFragmentPublication {
    generation: u64,
    payload: Arc<dyn Any + Send + Sync>,
}

#[derive(Clone)]
pub struct SharedLayoutFragmentSnapshot<T> {
    pub generation: u64,
    pub payload: Arc<T>,
}

#[derive(Clone, Default)]
pub struct SharedLayoutFragmentTree(Arc<RwLock<Option<SharedLayoutFragmentPublication>>>);

impl SharedLayoutFragmentTree {
    pub fn set_with_generation<T>(&self, generation: u64, fragments: Arc<T>)
    where
        T: Any + Send + Sync + 'static,
    {
        let payload: Arc<dyn Any + Send + Sync> = fragments;
        *self.0.write() = Some(SharedLayoutFragmentPublication { generation, payload });
    }

    pub fn clear(&self) {
        *self.0.write() = None;
    }

    pub fn snapshot<T>(&self) -> Option<SharedLayoutFragmentSnapshot<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        let publication = self.0.read().as_ref().cloned()?;
        Some(SharedLayoutFragmentSnapshot {
            generation: publication.generation,
            payload: publication.payload.downcast::<T>().ok()?,
        })
    }

    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        self.snapshot::<T>().map(|snapshot| snapshot.payload)
    }

    pub fn payload_generation(&self) -> Option<u64> {
        self.0.read().as_ref().map(|publication| publication.generation)
    }

    pub fn payload_ptr(&self) -> Option<usize> {
        self.0
            .read()
            .as_ref()
            .map(|publication| Arc::as_ptr(&publication.payload) as *const () as usize)
    }
}

/// Thread-safe container for sharing diagnostic root scroll state between layout and the embedding.
/// Layout writes root scroll position and extents here after scroll commits;
/// the embedding reads them for shell UI such as the scroll indicator.
#[derive(Clone, Default)]
pub struct SharedScrollState(Arc<RwLock<ScrollStateData>>);

/// Scroll state data shared between layout and the embedding layer.
#[derive(Clone, Default)]
pub struct ScrollStateData {
    /// Root viewport scroll offset in CSS pixels.
    pub scroll_y: f64,
    /// Total content height in CSS pixels (0 if unknown).
    pub content_height: f64,
    /// Viewport height in CSS pixels.
    pub viewport_height: f64,
}

impl SharedScrollState {
    pub fn set(&self, data: ScrollStateData) {
        *self.0.write() = data;
    }

    pub fn get(&self) -> ScrollStateData {
        self.0.read().clone()
    }
}

/// Thread-safe container for committed per-node scroll offsets keyed by ExternalScrollId.
/// Layout updates this from committed scroll state. The embedder uses it only to
/// bootstrap or rebase browser scroll sampling, never as the live render authority.
#[derive(Clone, Default)]
pub struct SharedCommittedScrollOffsets(Arc<RwLock<FxHashMap<ExternalScrollId, LayoutVector2D>>>);

impl SharedCommittedScrollOffsets {
    pub fn set(&self, offsets: FxHashMap<ExternalScrollId, LayoutVector2D>) {
        *self.0.write() = offsets;
    }

    pub fn get(&self) -> FxHashMap<ExternalScrollId, LayoutVector2D> {
        self.0.read().clone()
    }
}

/// Immutable selection snapshot shared with the embedding layer.
#[derive(Clone, Default)]
pub struct DocumentSelectionSnapshot {
    pub rects: Vec<euclid::Rect<f32, euclid::UnknownUnit>>,
    pub text: String,
    pub revision: u64,
}

/// Document-level text selection snapshot.
/// Written by the script thread; read by the embedder renderer and UI.
#[derive(Clone, Default)]
pub struct DocumentSelectionState {
    snapshot: DocumentSelectionSnapshot,
}

#[derive(Clone, Default)]
pub struct SharedDocumentSelection(Arc<RwLock<DocumentSelectionState>>);

impl SharedDocumentSelection {
    /// Atomically update rects + text. Increments revision only when content changes.
    pub fn set_snapshot(&self, rects: Vec<euclid::Rect<f32, euclid::UnknownUnit>>, text: String) {
        let mut guard = self.0.write();
        let changed = guard.snapshot.rects != rects || guard.snapshot.text != text;
        guard.snapshot.rects = rects;
        guard.snapshot.text = text;
        if changed {
            guard.snapshot.revision = guard.snapshot.revision.wrapping_add(1);
        }
    }

    pub fn snapshot(&self) -> DocumentSelectionSnapshot {
        self.0.read().snapshot.clone()
    }
}

/// Global registry of shared layout fragment trees, keyed by WebViewId.
static LAYOUT_FRAGMENT_REGISTRY: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<WebViewId, SharedLayoutFragmentTree>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

/// Global registry of shared layout fragment trees, keyed by PipelineId.
static PIPELINE_LAYOUT_FRAGMENT_REGISTRY: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<PipelineId, SharedLayoutFragmentTree>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

/// Global registry of shared scroll states, keyed by WebViewId.
static SCROLL_REGISTRY: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<WebViewId, SharedScrollState>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

/// Global registry of shared scroll states, keyed by PipelineId.
static PIPELINE_SCROLL_REGISTRY: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<PipelineId, SharedScrollState>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

/// Global registry of committed scroll offsets, keyed by PipelineId.
static PIPELINE_COMMITTED_SCROLL_OFFSETS_REGISTRY: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<PipelineId, SharedCommittedScrollOffsets>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

/// Global registry of document selection states, keyed by WebViewId.
static SELECTION_REGISTRY: std::sync::LazyLock<
    std::sync::Mutex<FxHashMap<WebViewId, SharedDocumentSelection>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(FxHashMap::default()));

/// Get or create a SharedLayoutFragmentTree for a given WebViewId.
pub fn shared_layout_fragment_tree_for(id: WebViewId) -> SharedLayoutFragmentTree {
    LAYOUT_FRAGMENT_REGISTRY
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .clone()
}

/// Get or create a SharedLayoutFragmentTree for a given PipelineId.
pub fn shared_layout_fragment_tree_for_pipeline(id: PipelineId) -> SharedLayoutFragmentTree {
    PIPELINE_LAYOUT_FRAGMENT_REGISTRY
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .clone()
}

/// Get or create a SharedScrollState for a given WebViewId.
pub fn shared_scroll_state_for(id: WebViewId) -> SharedScrollState {
    SCROLL_REGISTRY
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .clone()
}

/// Get or create a SharedScrollState for a given PipelineId.
pub fn shared_scroll_state_for_pipeline(id: PipelineId) -> SharedScrollState {
    PIPELINE_SCROLL_REGISTRY
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .clone()
}

/// Get or create committed scroll offsets for a given PipelineId.
pub fn shared_committed_scroll_offsets_for_pipeline(id: PipelineId) -> SharedCommittedScrollOffsets {
    PIPELINE_COMMITTED_SCROLL_OFFSETS_REGISTRY
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .clone()
}

/// Get or create a SharedDocumentSelection for a given WebViewId.
pub fn shared_document_selection_for(id: WebViewId) -> SharedDocumentSelection {
    SELECTION_REGISTRY
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .clone()
}

/// Remove a SharedLayoutFragmentTree when a WebView is destroyed.
pub fn remove_shared_layout_fragment_tree(id: WebViewId) {
    LAYOUT_FRAGMENT_REGISTRY.lock().unwrap().remove(&id);
}

/// Remove a SharedLayoutFragmentTree when a pipeline is destroyed.
pub fn remove_shared_layout_fragment_tree_for_pipeline(id: PipelineId) {
    PIPELINE_LAYOUT_FRAGMENT_REGISTRY.lock().unwrap().remove(&id);
}

/// Remove a SharedScrollState when a WebView is destroyed.
pub fn remove_shared_scroll_state(id: WebViewId) {
    SCROLL_REGISTRY.lock().unwrap().remove(&id);
}

/// Remove a SharedScrollState when a pipeline is destroyed.
pub fn remove_shared_scroll_state_for_pipeline(id: PipelineId) {
    PIPELINE_SCROLL_REGISTRY.lock().unwrap().remove(&id);
}

/// Remove committed scroll offsets when a pipeline is destroyed.
pub fn remove_shared_committed_scroll_offsets_for_pipeline(id: PipelineId) {
    PIPELINE_COMMITTED_SCROLL_OFFSETS_REGISTRY
        .lock()
        .unwrap()
        .remove(&id);
}

pub trait GenericLayoutDataTrait: Any + MallocSizeOfTrait {
    fn as_any(&self) -> &dyn Any;
}

pub type GenericLayoutData = dyn GenericLayoutDataTrait + Send + Sync;

#[derive(MallocSizeOf)]
pub struct StyleData {
    /// Data that the style system associates with a node. When the
    /// style system is being used standalone, this is all that hangs
    /// off the node. This must be first to permit the various
    /// transmutations between ElementData and PersistentLayoutData.
    pub element_data: AtomicRefCell<ElementData>,

    /// Information needed during parallel traversals.
    pub parallel: DomParallelInfo,
}

impl Default for StyleData {
    fn default() -> Self {
        Self {
            element_data: AtomicRefCell::new(ElementData::default()),
            parallel: DomParallelInfo::default(),
        }
    }
}

/// Information that we need stored in each DOM node.
#[derive(Default, MallocSizeOf)]
pub struct DomParallelInfo {
    /// The number of children remaining to process during bottom-up traversal.
    pub children_to_process: AtomicIsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutNodeType {
    Element(LayoutElementType),
    Text,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutElementType {
    Element,
    HTMLBodyElement,
    HTMLBRElement,
    HTMLCanvasElement,
    HTMLHtmlElement,
    HTMLIFrameElement,
    HTMLImageElement,
    HTMLInputElement,
    HTMLMediaElement,
    HTMLObjectElement,
    HTMLOptGroupElement,
    HTMLOptionElement,
    HTMLParagraphElement,
    HTMLPreElement,
    HTMLSelectElement,
    HTMLTableCellElement,
    HTMLTableColElement,
    HTMLTableElement,
    HTMLTableRowElement,
    HTMLTableSectionElement,
    HTMLTextAreaElement,
    SVGImageElement,
    SVGSVGElement,
}

pub struct HTMLCanvasData {
    pub image_key: Option<ImageKey>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGGeometryElementKind {
    Path,
    Rect,
    Circle,
    Ellipse,
    Line,
    Polyline,
    Polygon,
}

#[derive(Clone, Debug)]
pub enum SVGGeometryData<'dom> {
    Path {
        d: Option<&'dom str>,
    },
    Rect {
        x: Option<SVGLengthValue>,
        y: Option<SVGLengthValue>,
        width: Option<SVGLengthValue>,
        height: Option<SVGLengthValue>,
        rx: Option<SVGLengthValue>,
        ry: Option<SVGLengthValue>,
    },
    Circle {
        cx: Option<SVGLengthValue>,
        cy: Option<SVGLengthValue>,
        r: Option<SVGLengthValue>,
    },
    Ellipse {
        cx: Option<SVGLengthValue>,
        cy: Option<SVGLengthValue>,
        rx: Option<SVGLengthValue>,
        ry: Option<SVGLengthValue>,
    },
    Line {
        x1: Option<SVGLengthValue>,
        y1: Option<SVGLengthValue>,
        x2: Option<SVGLengthValue>,
        y2: Option<SVGLengthValue>,
    },
    Polyline {
        points: Option<&'dom str>,
    },
    Polygon {
        points: Option<&'dom str>,
    },
}

impl SVGGeometryData<'_> {
    pub fn kind(&self) -> SVGGeometryElementKind {
        match self {
            Self::Path { .. } => SVGGeometryElementKind::Path,
            Self::Rect { .. } => SVGGeometryElementKind::Rect,
            Self::Circle { .. } => SVGGeometryElementKind::Circle,
            Self::Ellipse { .. } => SVGGeometryElementKind::Ellipse,
            Self::Line { .. } => SVGGeometryElementKind::Line,
            Self::Polyline { .. } => SVGGeometryElementKind::Polyline,
            Self::Polygon { .. } => SVGGeometryElementKind::Polygon,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SVGViewportData {
    pub width: Option<SVGLengthValue>,
    pub height: Option<SVGLengthValue>,
    pub view_box: Option<SVGRectValue>,
    pub preserve_aspect_ratio: SVGPreserveAspectRatioValue,
    pub overflow_hidden: bool,
}

impl SVGViewportData {
    pub fn ratio_from_view_box(&self) -> Option<f32> {
        self.view_box.and_then(svg_view_box_ratio)
    }
}

#[derive(Clone, Debug)]
pub struct SVGPaintData<'dom> {
    pub color: Option<&'dom str>,
    pub fill: Option<&'dom str>,
    pub fill_opacity: Option<f32>,
    pub fill_rule: Option<SVGFillRule>,
    pub stroke: Option<&'dom str>,
    pub stroke_opacity: Option<f32>,
    pub stroke_width: Option<SVGLengthValue>,
    pub stroke_linejoin: Option<SVGLineJoin>,
    pub stroke_linecap: Option<SVGLineCap>,
    pub stroke_miterlimit: Option<f32>,
    pub stroke_dasharray: Option<Vec<f32>>,
    pub stroke_dashoffset: Option<f32>,
    pub paint_order: Option<SVGPaintOrder>,
    pub opacity: Option<f32>,
    pub pointer_events: Option<SVGPointerEventsValue>,
    pub vector_effect: Option<SVGVectorEffect>,
    pub clip_rule: Option<SVGFillRule>,
    pub clip_path: Option<SVGReferenceValue<'dom>>,
    pub mask: Option<SVGReferenceValue<'dom>>,
    pub filter: Option<SVGReferenceValue<'dom>>,
    pub marker_start: Option<SVGReferenceValue<'dom>>,
    pub marker_mid: Option<SVGReferenceValue<'dom>>,
    pub marker_end: Option<SVGReferenceValue<'dom>>,
}

#[derive(Clone, Debug)]
pub struct SVGTextData {
    pub x: SVGLengthListValue,
    pub y: SVGLengthListValue,
    pub dx: SVGLengthListValue,
    pub dy: SVGLengthListValue,
    pub rotate: SVGNumberListValue,
    pub font_size: Option<SVGLengthValue>,
    pub text_length: Option<SVGLengthValue>,
    pub length_adjust: Option<SVGLengthAdjustValue>,
    pub text_anchor: Option<SVGTextAnchor>,
    pub alignment_baseline: Option<SVGTextBaselineValue>,
    pub dominant_baseline: Option<SVGTextBaselineValue>,
}

#[derive(Clone, Debug)]
pub struct SVGUseData<'dom> {
    pub href: Option<SVGReferenceValue<'dom>>,
    pub x: Option<SVGLengthValue>,
    pub y: Option<SVGLengthValue>,
    pub width: Option<SVGLengthValue>,
    pub height: Option<SVGLengthValue>,
}

#[derive(Clone, Debug)]
pub struct SVGForeignObjectData {
    pub x: Option<SVGLengthValue>,
    pub y: Option<SVGLengthValue>,
    pub width: Option<SVGLengthValue>,
    pub height: Option<SVGLengthValue>,
}

#[derive(Clone, Debug)]
pub enum SVGGradientData<'dom> {
    Linear {
        href: Option<SVGReferenceValue<'dom>>,
        x1: Option<SVGLengthValue>,
        y1: Option<SVGLengthValue>,
        x2: Option<SVGLengthValue>,
        y2: Option<SVGLengthValue>,
        gradient_units: Option<SVGCoordinateUnits>,
        gradient_transform: SVGTransformListValue,
        spread_method: Option<SVGGradientSpreadMethod>,
    },
    Radial {
        href: Option<SVGReferenceValue<'dom>>,
        cx: Option<SVGLengthValue>,
        cy: Option<SVGLengthValue>,
        r: Option<SVGLengthValue>,
        fx: Option<SVGLengthValue>,
        fy: Option<SVGLengthValue>,
        fr: Option<SVGLengthValue>,
        gradient_units: Option<SVGCoordinateUnits>,
        gradient_transform: SVGTransformListValue,
        spread_method: Option<SVGGradientSpreadMethod>,
    },
}

#[derive(Clone, Debug)]
pub struct SVGStopData<'dom> {
    pub offset: Option<f32>,
    pub stop_color: Option<&'dom str>,
    pub stop_opacity: Option<&'dom str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SVGMarkerUnitsValue {
    UserSpaceOnUse,
    StrokeWidth,
}

#[derive(Clone, Debug)]
pub struct SVGPatternData<'dom> {
    pub href: Option<SVGReferenceValue<'dom>>,
    pub x: Option<SVGLengthValue>,
    pub y: Option<SVGLengthValue>,
    pub width: Option<SVGLengthValue>,
    pub height: Option<SVGLengthValue>,
    pub pattern_units: Option<SVGCoordinateUnits>,
    pub pattern_content_units: Option<SVGCoordinateUnits>,
    pub pattern_transform: SVGTransformListValue,
    pub view_box: Option<SVGRectValue>,
    pub preserve_aspect_ratio: SVGPreserveAspectRatioValue,
}

#[derive(Clone, Debug)]
pub struct SVGFilterData {
    pub x: Option<SVGLengthValue>,
    pub y: Option<SVGLengthValue>,
    pub width: Option<SVGLengthValue>,
    pub height: Option<SVGLengthValue>,
    pub filter_units: Option<SVGCoordinateUnits>,
    pub primitive_units: Option<SVGCoordinateUnits>,
}

#[derive(Clone, Debug)]
pub struct SVGMarkerData {
    pub ref_x: Option<SVGLengthValue>,
    pub ref_y: Option<SVGLengthValue>,
    pub marker_width: Option<SVGLengthValue>,
    pub marker_height: Option<SVGLengthValue>,
    pub marker_units: Option<SVGMarkerUnitsValue>,
    pub orient_auto: bool,
    pub view_box: Option<SVGRectValue>,
    pub preserve_aspect_ratio: SVGPreserveAspectRatioValue,
}

#[derive(Clone, Debug)]
pub struct SVGTextPathData<'dom> {
    pub href: Option<SVGReferenceValue<'dom>>,
    pub start_offset: Option<SVGLengthValue>,
    pub text: SVGTextData,
}

#[derive(Clone, Debug)]
pub struct SVGClipPathData {
    pub clip_path_units: Option<SVGCoordinateUnits>,
}

#[derive(Clone, Debug)]
pub struct SVGMaskData {
    pub x: Option<SVGLengthValue>,
    pub y: Option<SVGLengthValue>,
    pub width: Option<SVGLengthValue>,
    pub height: Option<SVGLengthValue>,
    pub mask_units: Option<SVGCoordinateUnits>,
    pub mask_content_units: Option<SVGCoordinateUnits>,
}

#[derive(Clone, Debug)]
pub struct SVGImageData<'dom> {
    pub href: Option<SVGReferenceValue<'dom>>,
    pub x: Option<SVGLengthValue>,
    pub y: Option<SVGLengthValue>,
    pub width: Option<SVGLengthValue>,
    pub height: Option<SVGLengthValue>,
    pub preserve_aspect_ratio: SVGPreserveAspectRatioValue,
}

#[derive(Clone, Debug)]
pub struct SVGCommonData<'dom> {
    pub element_id: Option<&'dom str>,
    pub transform: SVGTransformListValue,
}

#[derive(Clone, Debug)]
pub enum SVGNodeKind<'dom> {
    Viewport(SVGViewportData),
    Group,
    Geometry(SVGGeometryData<'dom>),
    Text(SVGTextData),
    TSpan(SVGTextData),
    TextPath(SVGTextPathData<'dom>),
    Defs,
    Use(SVGUseData<'dom>),
    ForeignObject(SVGForeignObjectData),
    Gradient(SVGGradientData<'dom>),
    Stop(SVGStopData<'dom>),
    ClipPath(SVGClipPathData),
    Mask(SVGMaskData),
    Pattern(SVGPatternData<'dom>),
    Filter(SVGFilterData),
    Marker(SVGMarkerData),
    Image(SVGImageData<'dom>),
}

#[derive(Clone, Debug)]
pub struct SVGElementData<'dom> {
    pub common: SVGCommonData<'dom>,
    pub node_kind: SVGNodeKind<'dom>,
    pub paint: SVGPaintData<'dom>,
}

impl<'dom> SVGElementData<'dom> {
    pub fn viewport(&self) -> Option<&SVGViewportData> {
        match &self.node_kind {
            SVGNodeKind::Viewport(data) => Some(data),
            _ => None,
        }
    }
}

/// The address of a node known to be valid. These are sent from script to layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrustedNodeAddress(pub *const c_void);

#[expect(unsafe_code)]
unsafe impl Send for TrustedNodeAddress {}

/// Whether the pending image needs to be fetched or is waiting on an existing fetch.
#[derive(Debug)]
pub enum PendingImageState {
    Unrequested(BrowserUrl),
    PendingResponse,
}

/// The destination in layout where an image is needed.
#[derive(Debug, MallocSizeOf)]
pub enum LayoutImageDestination {
    BoxTreeConstruction,
    DisplayListBuilding,
}

/// The data associated with an image that is not yet present in the image cache.
/// Used by the script thread to hold on to DOM elements that need to be repainted
/// when an image fetch is complete.
#[derive(Debug)]
pub struct PendingImage {
    pub state: PendingImageState,
    pub node: UntrustedNodeAddress,
    pub id: PendingImageId,
    pub origin: ImmutableOrigin,
    pub destination: LayoutImageDestination,
}

#[derive(Clone, Copy, Debug, MallocSizeOf)]
pub struct MediaFrame {
    pub image_key: webrender_api::ImageKey,
    pub width: i32,
    pub height: i32,
}

pub struct MediaMetadata {
    pub width: u32,
    pub height: u32,
}

pub struct HTMLMediaData {
    pub current_frame: Option<MediaFrame>,
    pub metadata: Option<MediaMetadata>,
}

pub struct LayoutConfig {
    pub id: PipelineId,
    pub webview_id: WebViewId,
    pub url: BrowserUrl,
    pub is_iframe: bool,
    pub script_chan: GenericSender<ScriptThreadMessage>,
    pub image_cache: Arc<dyn ImageCache>,
    pub font_context: Arc<FontContext>,
    pub time_profiler_chan: time::ProfilerChan,
    pub paint_api: CrossProcessPaintApi,
    pub viewport_details: ViewportDetails,
    pub user_stylesheets: Rc<Vec<DocumentStyleSheet>>,
    pub theme: Theme,
    pub accessibility_active: bool,
    pub shared_layout_fragments: SharedLayoutFragmentTree,
    pub shared_layout_fragments_by_pipeline: SharedLayoutFragmentTree,
    pub shared_scroll_state: SharedScrollState,
    pub shared_scroll_state_by_pipeline: SharedScrollState,
}

pub struct PropertyRegistration {
    pub name: String,
    pub syntax: String,
    pub initial_value: Option<String>,
    pub inherits: bool,
    pub url_data: UrlExtraData,
}

#[derive(Debug)]
pub enum RegisterPropertyError {
    InvalidName,
    AlreadyRegistered,
    InvalidSyntax,
    InvalidInitialValue,
    InitialValueNotComputationallyIndependent,
    NoInitialValue,
}

pub trait LayoutFactory: Send + Sync {
    fn create(&self, config: LayoutConfig) -> Box<dyn Layout>;
}

pub trait Layout {
    /// Get a reference to this Layout's Stylo `Device` used to handle media queries and
    /// resolve font metrics.
    fn device(&self) -> &Device;

    /// Set the theme on this [`Layout`]'s [`Device`]. The caller should also trigger a
    /// new layout when this happens, though it can happen later. Returns `true` if the
    /// [`Theme`] actually changed or `false` otherwise.
    fn set_theme(&mut self, theme: Theme) -> bool;

    /// Set the [`ViewportDetails`] on this [`Layout`]'s [`Device`]. The caller should also
    /// trigger a new layout when this happens, though it can happen later. Returns `true`
    /// if the [`ViewportDetails`] actually changed or `false` otherwise.
    fn set_viewport_details(&mut self, viewport_details: ViewportDetails) -> bool;

    /// Load all fonts from the given stylesheet, returning the number of fonts that
    /// need to be loaded.
    fn load_web_fonts_from_stylesheet(
        &self,
        stylesheet: &ServoArc<Stylesheet>,
        font_context: &WebFontDocumentContext,
    );

    /// Add a stylesheet to this Layout. This will add it to the Layout's `Stylist` as well as
    /// loading all web fonts defined in the stylesheet. The second stylesheet is the insertion
    /// point (if it exists, the sheet needs to be inserted before it).
    fn add_stylesheet(
        &mut self,
        stylesheet: ServoArc<Stylesheet>,
        before_stylsheet: Option<ServoArc<Stylesheet>>,
        font_context: &WebFontDocumentContext,
    );

    /// Inform the layout that its ScriptThread is about to exit.
    fn exit_now(&mut self);

    /// Requests that layout measure its memory usage. The resulting reports are sent back
    /// via the supplied channel.
    fn collect_reports(&self, reports: &mut Vec<Report>, ops: &mut MallocSizeOfOps);

    /// Sets quirks mode for the document, causing the quirks mode stylesheet to be used.
    fn set_quirks_mode(&mut self, quirks_mode: QuirksMode);

    /// Removes a stylesheet from the Layout.
    fn remove_stylesheet(&mut self, stylesheet: ServoArc<Stylesheet>);

    /// Removes an image from the Layout image resolver cache.
    fn remove_cached_image(&mut self, image_url: &BrowserUrl);

    /// Requests a reflow.
    fn reflow(&mut self, reflow_request: ReflowRequest) -> Option<ReflowResult>;

    /// Do not request a reflow, but ensure that any previous reflow completes building a stacking
    /// context tree so that it is ready to query the final size of any elements in script.
    fn ensure_stacking_context_tree(&self, viewport_details: ViewportDetails);

    /// Tells layout that script has added some paint worklet modules.
    fn register_paint_worklet_modules(
        &mut self,
        name: Atom,
        properties: Vec<Atom>,
        painter: Box<dyn Painter>,
    );

    /// Set the scroll states of this layout after a `Paint` scroll.
    fn set_scroll_offsets_from_renderer(
        &mut self,
        scroll_states: &FxHashMap<ExternalScrollId, LayoutVector2D>,
    );

    /// Get the scroll offset of the given scroll node with id of [`ExternalScrollId`] or `None` if it does
    /// not exist in the tree.
    fn scroll_offset(&self, id: ExternalScrollId) -> Option<LayoutVector2D>;

    /// Returns true if this layout needs to produce a new display list for rendering updates.
    fn needs_new_display_list(&self) -> bool;

    /// Marks that this layout needs to produce a new display list for rendering updates.
    fn set_needs_new_display_list(&self);

    fn query_padding(&self, node: TrustedNodeAddress) -> Option<PhysicalSides>;
    fn query_box_area(
        &self,
        node: TrustedNodeAddress,
        area: BoxAreaType,
        exclude_transform_and_inline: bool,
    ) -> Option<Rect<Au, CSSPixel>>;
    fn query_box_areas(&self, node: TrustedNodeAddress, area: BoxAreaType) -> CSSPixelRectIterator;
    fn query_client_rect(&self, node: TrustedNodeAddress) -> Rect<i32, CSSPixel>;
    fn query_current_css_zoom(&self, node: TrustedNodeAddress) -> f32;
    fn query_element_inner_outer_text(&self, node: TrustedNodeAddress) -> String;
    fn query_offset_parent(&self, node: TrustedNodeAddress) -> OffsetParentResponse;
    /// Query the scroll container for the given node. If node is `None`, the scroll container for
    /// the viewport is returned.
    fn query_scroll_container(
        &self,
        node: Option<TrustedNodeAddress>,
        flags: ScrollContainerQueryFlags,
    ) -> Option<ScrollContainerResponse>;
    fn query_resolved_style(
        &self,
        node: TrustedNodeAddress,
        pseudo: Option<PseudoElement>,
        property_id: PropertyId,
        animations: DocumentAnimationSet,
        animation_timeline_value: f64,
    ) -> String;
    fn query_resolved_font_style(
        &self,
        node: TrustedNodeAddress,
        value: &str,
        animations: DocumentAnimationSet,
        animation_timeline_value: f64,
    ) -> Option<ServoArc<Font>>;
    fn query_scrolling_area(&self, node: Option<TrustedNodeAddress>) -> Rect<i32, CSSPixel>;
    /// Find the character offset of the point in the given node, if it has text content.
    fn query_text_index(
        &self,
        node: TrustedNodeAddress,
        point: Point2D<Au, CSSPixel>,
    ) -> Option<usize>;
    /// Find the text node and character offset at a viewport point, searching within
    /// descendants of the given node. Returns (opaque_node, offset).
    fn query_text_node_at_point(
        &self,
        node: TrustedNodeAddress,
        point: Point2D<Au, CSSPixel>,
    ) -> Option<(OpaqueNode, usize)>;
    /// Find the closest text node and character offset to a viewport point,
    /// searching all text fragments in the document. Returns (opaque_node, offset).
    fn query_text_at_viewport_point(
        &self,
        point: Point2D<Au, CSSPixel>,
    ) -> Option<(OpaqueNode, usize)>;
    fn query_svg_bbox(
        &self,
        node: TrustedNodeAddress,
        options: SVGBoundingBoxOptionsData,
    ) -> Option<Rect<f32, CSSPixel>>;
    fn query_svg_ctm(
        &self,
        node: TrustedNodeAddress,
    ) -> Option<Transform2D<f32, CSSPixel, CSSPixel>>;
    fn query_svg_screen_ctm(
        &self,
        node: TrustedNodeAddress,
    ) -> Option<Transform2D<f32, CSSPixel, CSSPixel>>;
    fn query_svg_geometry_fill_contains(
        &self,
        node: TrustedNodeAddress,
        point: Point2D<f32, CSSPixel>,
    ) -> Option<bool>;
    fn query_svg_geometry_stroke_contains(
        &self,
        node: TrustedNodeAddress,
        point: Point2D<f32, CSSPixel>,
    ) -> Option<bool>;
    fn query_svg_geometry_total_length(&self, node: TrustedNodeAddress) -> Option<f32>;
    fn query_svg_geometry_point_at_length(
        &self,
        node: TrustedNodeAddress,
        length: f32,
    ) -> Option<Point2D<f32, CSSPixel>>;
    fn query_svg_text_substring_length(
        &self,
        node: TrustedNodeAddress,
        charnum: u32,
        nchars: u32,
    ) -> Option<f32>;
    fn query_svg_text_char_geometry(
        &self,
        node: TrustedNodeAddress,
        charnum: u32,
    ) -> Option<SVGTextCharGeometry>;
    fn query_svg_text_char_num_at_position(
        &self,
        node: TrustedNodeAddress,
        point: Point2D<f32, CSSPixel>,
    ) -> Option<i32>;
    fn query_svg_text_range_bbox(
        &self,
        node: TrustedNodeAddress,
        charnum: u32,
        nchars: u32,
    ) -> Option<Rect<f32, CSSPixel>>;
    fn query_elements_from_point(
        &self,
        point: LayoutPoint,
        flags: ElementsFromPointFlags,
    ) -> Vec<ElementsFromPointResult>;
    fn query_effective_overflow(&self, node: TrustedNodeAddress) -> Option<AxesOverflow>;
    fn register_custom_property(
        &mut self,
        property_registration: PropertyRegistration,
    ) -> Result<(), RegisterPropertyError>;

    fn set_accessibility_active(&self, active: bool);
}

/// This trait is part of `layout_api` because it depends on both `script_traits`
/// and also `LayoutFactory` from this crate. If it was in `script_traits` there would be a
/// circular dependency.
pub trait ScriptThreadFactory {
    /// Create a `ScriptThread`.
    fn create(
        state: InitialScriptState,
        layout_factory: Arc<dyn LayoutFactory>,
        image_cache_factory: Arc<dyn ImageCacheFactory>,
        background_hang_monitor_register: Box<dyn BackgroundHangMonitorRegister>,
    ) -> JoinHandle<()>;
}

/// Type of the area of CSS box for query.
/// See <https://www.w3.org/TR/css-box-3/#box-model>.
#[derive(Copy, Clone)]
pub enum BoxAreaType {
    Content,
    Padding,
    Border,
}

pub type CSSPixelRectIterator = Box<dyn Iterator<Item = Rect<Au, CSSPixel>>>;

#[derive(Clone, Copy, Debug, Default)]
pub struct SVGBoundingBoxOptionsData {
    pub fill: bool,
    pub stroke: bool,
    pub markers: bool,
    pub clipped: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SVGTextCharGeometry {
    pub start: Point2D<f32, CSSPixel>,
    pub end: Point2D<f32, CSSPixel>,
    pub extent: Rect<f32, CSSPixel>,
    pub rotation: f32,
}

#[derive(Default)]
pub struct PhysicalSides {
    pub left: Au,
    pub top: Au,
    pub right: Au,
    pub bottom: Au,
}

#[derive(Clone, Default)]
pub struct OffsetParentResponse {
    pub node_address: Option<UntrustedNodeAddress>,
    pub rect: Rect<Au, CSSPixel>,
}

bitflags! {
    #[derive(PartialEq)]
    pub struct ScrollContainerQueryFlags: u8 {
        /// Whether or not this query is for the purposes of a `scrollParent` layout query.
        const ForScrollParent = 1 << 0;
        /// Whether or not to consider the original element's scroll box for the return value.
        const Inclusive = 1 << 1;
    }
}

#[derive(Clone, Copy, Debug, MallocSizeOf)]
pub struct AxesOverflow {
    pub x: Overflow,
    pub y: Overflow,
}

impl Default for AxesOverflow {
    fn default() -> Self {
        Self {
            x: Overflow::Visible,
            y: Overflow::Visible,
        }
    }
}

impl From<&ComputedValues> for AxesOverflow {
    fn from(style: &ComputedValues) -> Self {
        Self {
            x: style.clone_overflow_x(),
            y: style.clone_overflow_y(),
        }
    }
}

impl AxesOverflow {
    pub fn to_scrollable(&self) -> Self {
        Self {
            x: self.x.to_scrollable(),
            y: self.y.to_scrollable(),
        }
    }

    /// Whether or not the `overflow` value establishes a scroll container.
    pub fn establishes_scroll_container(&self) -> bool {
        // Checking one axis suffices, because the computed value ensures that
        // either both axes are scrollable, or none is scrollable.
        self.x.is_scrollable()
    }
}

#[derive(Clone)]
pub enum ScrollContainerResponse {
    Viewport(AxesOverflow),
    Element(UntrustedNodeAddress, AxesOverflow),
}

#[derive(Debug, PartialEq)]
pub enum QueryMsg {
    BoxArea,
    BoxAreas,
    ClientRectQuery,
    CurrentCSSZoomQuery,
    EffectiveOverflow,
    ElementInnerOuterTextQuery,
    ElementsFromPoint,
    InnerWindowDimensionsQuery,
    NodesFromPointQuery,
    OffsetParentQuery,
    ScrollParentQuery,
    ResolvedFontStyleQuery,
    ResolvedStyleQuery,
    SVGQuery,
    ScrollingAreaOrOffsetQuery,
    StyleQuery,
    TextIndexQuery,
    PaddingQuery,
}

/// The goal of a reflow request.
///
/// Please do not add any other types of reflows. In general, all reflow should
/// go through the *update the rendering* step of the HTML specification. Exceptions
/// should have careful review.
#[derive(Debug, PartialEq)]
pub enum ReflowGoal {
    /// A reflow has been requesting by the *update the rendering* step of the HTML
    /// event loop. This nominally driven by the display's VSync.
    UpdateTheRendering,

    /// Script has done a layout query and this reflow ensurs that layout is up-to-date
    /// with the latest changes to the DOM.
    LayoutQuery(QueryMsg),

    /// Tells layout about a single new scrolling offset from the script. The rest will
    /// remain untouched. Layout will forward whether the element is scrolled through
    /// [ReflowResult].
    UpdateScrollNode(ExternalScrollId, LayoutVector2D),
}

#[derive(Clone, Debug, MallocSizeOf)]
pub struct IFrameSize {
    pub browsing_context_id: BrowsingContextId,
    pub pipeline_id: PipelineId,
    pub viewport_details: ViewportDetails,
}

pub type IFrameSizes = FxHashMap<BrowsingContextId, IFrameSize>;

bitflags! {
    /// Conditions which cause a [`Document`] to need to be restyled during reflow, which
    /// might cause the rest of layout to happen as well.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct RestyleReason: u16 {
        const StylesheetsChanged = 1 << 0;
        const DOMChanged = 1 << 1;
        const PendingRestyles = 1 << 2;
        const HighlightedDOMNodeChanged = 1 << 3;
        const ThemeChanged = 1 << 4;
        const ViewportChanged = 1 << 5;
        const PaintWorkletLoaded = 1 << 6;
    }
}

malloc_size_of_is_0!(RestyleReason);

impl RestyleReason {
    pub fn needs_restyle(&self) -> bool {
        !self.is_empty()
    }
}

/// Information derived from a layout pass that needs to be returned to the script thread.
#[derive(Debug, Default)]
pub struct ReflowResult {
    /// The phases that were run during this reflow.
    pub reflow_phases_run: ReflowPhasesRun,
    pub reflow_statistics: ReflowStatistics,
    /// The list of images that were encountered that are in progress.
    pub pending_images: Vec<PendingImage>,
    /// The list of iframes in this layout and their sizes, used in order
    /// to communicate them with the Constellation and also the `Window`
    /// element of their content pages. Returning None if incremental reflow
    /// finished before reaching this stage of the layout. I.e., no update
    /// required.
    pub iframe_sizes: Option<IFrameSizes>,
}

bitflags! {
    /// The phases of reflow that were run when processing a reflow in layout.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct ReflowPhasesRun: u8 {
        const RanLayout = 1 << 0;
        const CalculatedOverflow = 1 << 1;
        const BuiltStackingContextTree = 1 << 2;
        const BuiltDisplayList = 1 << 3;
        const UpdatedScrollNodeOffset = 1 << 4;
        /// Image data for a WebRender image key has been updated, without necessarily
        /// updating style or layout. This is used when updating canvas contents and
        /// progressing to a new animated image frame.
        const UpdatedImageData = 1 << 5;
    }
}

impl ReflowPhasesRun {
    pub fn needs_frame(&self) -> bool {
        self.intersects(
            Self::BuiltDisplayList | Self::UpdatedScrollNodeOffset | Self::UpdatedImageData,
        )
    }
}

#[derive(Debug, Default)]
pub struct ReflowStatistics {
    pub rebuilt_fragment_count: u32,
    pub restyle_fragment_count: u32,
}

/// Information needed for a script-initiated reflow that requires a restyle
/// and reconstruction of box and fragment trees.
#[derive(Debug)]
pub struct ReflowRequestRestyle {
    /// Whether or not (and for what reasons) restyle needs to happen.
    pub reason: RestyleReason,
    /// The dirty root from which to restyle.
    pub dirty_root: Option<TrustedNodeAddress>,
    /// Whether the document's stylesheets have changed since the last script reflow.
    pub stylesheets_changed: bool,
    /// Restyle snapshot map.
    pub pending_restyles: Vec<(TrustedNodeAddress, PendingRestyle)>,
}

/// Information needed for a script-initiated reflow.
#[derive(Debug)]
pub struct ReflowRequest {
    /// The document node.
    pub document: TrustedNodeAddress,
    /// The current layout [`Epoch`] managed by the script thread.
    pub epoch: Epoch,
    /// If a restyle is necessary, all of the informatio needed to do that restyle.
    pub restyle: Option<ReflowRequestRestyle>,
    /// The current [`ViewportDetails`] to use for this reflow.
    pub viewport_details: ViewportDetails,
    /// The goal of this reflow.
    pub reflow_goal: ReflowGoal,
    /// The number of objects in the dom #10110
    pub dom_count: u32,
    /// The current window origin
    pub origin: ImmutableOrigin,
    /// The current animation timeline value.
    pub animation_timeline_value: f64,
    /// The set of animations for this document.
    pub animations: DocumentAnimationSet,
    /// An [`AnimatingImages`] struct used to track images that are animating.
    pub animating_images: Arc<RwLock<AnimatingImages>>,
    /// The node highlighted by the devtools, if any
    pub highlighted_dom_node: Option<OpaqueNode>,
    /// The current font context.
    pub document_context: WebFontDocumentContext,
    /// The current document text selection, if any.
    pub selection: Option<DocumentSelection>,
}

/// A document-level text selection (not input/textarea selection).
#[derive(Clone, Debug, MallocSizeOf)]
pub struct DocumentSelection {
    /// The start (range start) node and character offset.
    pub start: (OpaqueNode, u32),
    /// The end (range end) node and character offset.
    pub end: (OpaqueNode, u32),
    /// OpaqueNodes of text nodes fully contained within the selection range.
    #[ignore_malloc_size_of = "HashSet<OpaqueNode>"]
    pub interior_nodes: std::collections::HashSet<OpaqueNode>,
}

impl ReflowRequest {
    pub fn stylesheets_changed(&self) -> bool {
        self.restyle
            .as_ref()
            .is_some_and(|restyle| restyle.stylesheets_changed)
    }
}

/// A pending restyle.
#[derive(Debug, Default, MallocSizeOf)]
pub struct PendingRestyle {
    /// If this element had a state or attribute change since the last restyle, track
    /// the original condition of the element.
    pub snapshot: Option<Snapshot>,

    /// Any explicit restyles hints that have been accumulated for this element.
    pub hint: RestyleHint,

    /// Any explicit restyles damage that have been accumulated for this element.
    pub damage: RestyleDamage,
}

/// The type of fragment that a scroll root is created for.
///
/// This can only ever grow to maximum 4 entries. That's because we cram the value of this enum
/// into the lower 2 bits of the `ScrollRootId`, which otherwise contains a 32-bit-aligned
/// heap address.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, MallocSizeOf, PartialEq, Serialize)]
pub enum FragmentType {
    /// A StackingContext for the fragment body itself.
    FragmentBody,
    /// A StackingContext created to contain ::before pseudo-element content.
    BeforePseudoContent,
    /// A StackingContext created to contain ::after pseudo-element content.
    AfterPseudoContent,
}

impl From<Option<PseudoElement>> for FragmentType {
    fn from(value: Option<PseudoElement>) -> Self {
        match value {
            Some(PseudoElement::After) => FragmentType::AfterPseudoContent,
            Some(PseudoElement::Before) => FragmentType::BeforePseudoContent,
            _ => FragmentType::FragmentBody,
        }
    }
}

/// The next ID that will be used for a special scroll root id.
///
/// A special scroll root is a scroll root that is created for generated content.
static NEXT_SPECIAL_SCROLL_ROOT_ID: AtomicU64 = AtomicU64::new(0);

/// If none of the bits outside this mask are set, the scroll root is a special scroll root.
/// Note that we assume that the top 16 bits of the address space are unused on the platform.
const SPECIAL_SCROLL_ROOT_ID_MASK: u64 = 0xffff;

/// Returns a new scroll root ID for a scroll root.
fn next_special_id() -> u64 {
    // We shift this left by 2 to make room for the fragment type ID.
    ((NEXT_SPECIAL_SCROLL_ROOT_ID.fetch_add(1, Ordering::SeqCst) + 1) << 2)
        & SPECIAL_SCROLL_ROOT_ID_MASK
}

pub fn combine_id_with_fragment_type(id: usize, fragment_type: FragmentType) -> u64 {
    debug_assert_eq!(id & (fragment_type as usize), 0);
    if fragment_type == FragmentType::FragmentBody {
        id as u64
    } else {
        next_special_id() | (fragment_type as u64)
    }
}

pub fn node_id_from_scroll_id(id: usize) -> Option<usize> {
    if (id as u64 & !SPECIAL_SCROLL_ROOT_ID_MASK) != 0 {
        return Some(id & !3);
    }
    None
}

#[derive(Clone, Debug, MallocSizeOf)]
pub struct ImageAnimationState {
    #[ignore_malloc_size_of = "RasterImage"]
    pub image: Arc<RasterImage>,
    pub active_frame: usize,
    frame_start_time: f64,
}

impl ImageAnimationState {
    pub fn new(image: Arc<RasterImage>, last_update_time: f64) -> Self {
        Self {
            image,
            active_frame: 0,
            frame_start_time: last_update_time,
        }
    }

    pub fn image_key(&self) -> Option<ImageKey> {
        self.image.id
    }

    pub fn duration_to_next_frame(&self, now: f64) -> Duration {
        let frame_delay = self
            .image
            .frames
            .get(self.active_frame)
            .expect("Image frame should always be valid")
            .delay
            .unwrap_or_default();

        let time_since_frame_start = (now - self.frame_start_time).max(0.0) * 1000.0;
        let time_since_frame_start = Duration::from_secs_f64(time_since_frame_start);
        frame_delay - time_since_frame_start.min(frame_delay)
    }

    /// check whether image active frame need to be updated given current time,
    /// return true if there are image that need to be updated.
    /// false otherwise.
    pub fn update_frame_for_animation_timeline_value(&mut self, now: f64) -> bool {
        if self.image.frames.len() <= 1 {
            return false;
        }
        let image = &self.image;
        let time_interval_since_last_update = now - self.frame_start_time;
        let mut remain_time_interval = time_interval_since_last_update
            - image
                .frames
                .get(self.active_frame)
                .unwrap()
                .delay()
                .unwrap()
                .as_secs_f64();
        let mut next_active_frame_id = self.active_frame;
        while remain_time_interval > 0.0 {
            next_active_frame_id = (next_active_frame_id + 1) % image.frames.len();
            remain_time_interval -= image
                .frames
                .get(next_active_frame_id)
                .unwrap()
                .delay()
                .unwrap()
                .as_secs_f64();
        }
        if self.active_frame == next_active_frame_id {
            return false;
        }
        self.active_frame = next_active_frame_id;
        self.frame_start_time = now;
        true
    }
}

/// Describe an item that matched a hit-test query.
#[derive(Debug)]
pub struct ElementsFromPointResult {
    /// An [`OpaqueNode`] that contains a pointer to the node hit by
    /// this hit test result.
    pub node: OpaqueNode,
    /// The [`Point2D`] of the original query point relative to the
    /// node fragment rectangle.
    pub point_in_target: Point2D<f32, CSSPixel>,
    /// The [`Cursor`] that's defined on the item that is hit by this
    /// hit test result.
    pub cursor: Cursor,
}

bitflags! {
    pub struct ElementsFromPointFlags: u8 {
        /// Whether or not to find all of the items for a hit test or stop at the
        /// first hit.
        const FindAll = 0b00000001;
    }
}

#[derive(Debug, Default, MallocSizeOf)]
pub struct AnimatingImages {
    /// A map from the [`OpaqueNode`] to the state of an animating image. This is used
    /// to update frames in script and to track newly animating nodes.
    pub node_to_state_map: FxHashMap<OpaqueNode, ImageAnimationState>,
    /// Whether or not this map has changed during a layout. This is used by script to
    /// trigger future animation updates.
    pub dirty: bool,
    /// Revision of the image frame selection visible to the render pipeline.
    /// This increments whenever the active frame set changes or animating-image
    /// membership changes, so cross-thread fragment publication can invalidate
    /// retained scene caches only when image content can actually differ.
    pub image_animation_revision: u64,
}

impl AnimatingImages {
    fn bump_image_animation_revision(&mut self) {
        self.image_animation_revision = self.image_animation_revision.wrapping_add(1);
    }

    pub fn maybe_insert_or_update(
        &mut self,
        node: OpaqueNode,
        image: Arc<RasterImage>,
        current_timeline_value: f64,
    ) {
        match self.node_to_state_map.entry(node) {
            Entry::Vacant(entry) => {
                self.dirty = true;
                entry.insert(ImageAnimationState::new(image, current_timeline_value));
                self.bump_image_animation_revision();
            }
            Entry::Occupied(mut entry) => {
                // If the entry exists, but it is for a different image id, replace it as the image
                // has changed during this layout.
                if entry.get().image.id != image.id {
                    self.dirty = true;
                    entry.insert(ImageAnimationState::new(image, current_timeline_value));
                    self.bump_image_animation_revision();
                }
            }
        }
    }

    pub fn remove(&mut self, node: OpaqueNode) {
        if self.node_to_state_map.remove(&node).is_some() {
            self.dirty = true;
            self.bump_image_animation_revision();
        }
    }

    pub fn note_frame_selection_changed(&mut self) {
        self.bump_image_animation_revision();
    }

    pub fn image_animation_revision(&self) -> u64 {
        self.image_animation_revision
    }

    /// Clear the dirty bit on this [`AnimatingImages`] and return the previous value.
    pub fn clear_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn is_empty(&self) -> bool {
        self.node_to_state_map.is_empty()
    }
}

struct ThreadStateRestorer {
    needs_restore: bool,
}

impl ThreadStateRestorer {
    fn new() -> Self {
        #[cfg(debug_assertions)]
        {
            let current = thread_state::get();
            if current.contains(ThreadState::LAYOUT) {
                return Self { needs_restore: false };
            }
            thread_state::exit(ThreadState::SCRIPT);
            thread_state::enter(ThreadState::LAYOUT);
            return Self { needs_restore: true };
        }
        #[cfg(not(debug_assertions))]
        Self { needs_restore: false }
    }
}

impl Drop for ThreadStateRestorer {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            if self.needs_restore {
                thread_state::exit(ThreadState::LAYOUT);
                thread_state::enter(ThreadState::SCRIPT);
            }
        }
    }
}

/// Set up the thread-local state to reflect that layout code is about to run,
/// then call the provided function.
/// This must be used when running code that will interact with the DOM tree
/// through types like `ServoLayoutNode`, `ServoLayoutElement`, and `LayoutDom`,
/// which have rules about how they must be used from layout worker threads.
pub fn with_layout_state<R>(f: impl FnOnce() -> R) -> R {
    let _guard = ThreadStateRestorer::new();
    f()
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use std::time::Duration;

    use pixels::{CorsStatus, ImageFrame, ImageMetadata, PixelFormat, RasterImage};
    use style::dom::OpaqueNode;

    use crate::{AnimatingImages, ImageAnimationState};

    fn raster_image(byte: u8) -> Arc<RasterImage> {
        Arc::new(RasterImage {
            metadata: ImageMetadata {
                width: 100,
                height: 100,
            },
            format: PixelFormat::BGRA8,
            id: None,
            bytes: Arc::new(vec![byte]),
            frames: std::iter::repeat_with(|| ImageFrame {
                delay: Some(Duration::from_millis(100)),
                byte_range: 0..1,
                width: 100,
                height: 100,
            })
            .take(10)
            .collect(),
            cors_status: CorsStatus::Unsafe,
            is_opaque: false,
        })
    }

    #[test]
    fn image_animation_state_advances_frames() {
        let mut image_animation_state = ImageAnimationState::new(raster_image(1), 0.0);

        assert_eq!(image_animation_state.active_frame, 0);
        assert_eq!(image_animation_state.frame_start_time, 0.0);
        assert_eq!(
            image_animation_state.update_frame_for_animation_timeline_value(0.101),
            true
        );
        assert_eq!(image_animation_state.active_frame, 1);
        assert_eq!(image_animation_state.frame_start_time, 0.101);
        assert_eq!(
            image_animation_state.update_frame_for_animation_timeline_value(0.116),
            false
        );
        assert_eq!(image_animation_state.active_frame, 1);
        assert_eq!(image_animation_state.frame_start_time, 0.101);
    }

    #[test]
    fn animating_images_revision_tracks_visible_changes() {
        let mut animating_images = AnimatingImages::default();
        assert_eq!(animating_images.image_animation_revision(), 0);

        animating_images.maybe_insert_or_update(OpaqueNode(1), raster_image(1), 0.0);
        assert_eq!(animating_images.image_animation_revision(), 1);

        animating_images.note_frame_selection_changed();
        assert_eq!(animating_images.image_animation_revision(), 2);

        animating_images.remove(OpaqueNode(1));
        assert_eq!(animating_images.image_animation_revision(), 3);
    }
}
