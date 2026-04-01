/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::f64;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base::generic_channel::GenericCallback;
use base64::Engine as _;
use content_security_policy::sandboxing_directive::SandboxingFlagSet;
use dom_struct::dom_struct;
use embedder_traits::{EmbedderMsg, HpprControlRequest, HpprControlResponse, MediaPositionState, MediaSessionEvent};
use headers::{ContentLength, ContentRange, HeaderMapExt};
use hppr_packet::Packet;
use html5ever::{LocalName, Prefix, QualName, local_name, ns};
use http::StatusCode;
use http::header::{self, HeaderMap, HeaderValue};
use js::realm::{AutoRealm, CurrentRealm};
use layout_api::MediaFrame;
use media::controller::{MediaController, MediaEvent, MediaOrigin, register_event_sender};
use net::hppr_media::ResolvedHpprMediaAsset;
use net_traits::request::{Destination, RequestId};
use net_traits::{
    CoreResourceThread, FetchMetadata, FilteredMetadata, NetworkError, ResourceFetchTiming,
};
use pixels::RasterImage;
use script_bindings::codegen::GenericUnionTypes::BlobOrMediaSource;
use script_bindings::codegen::InheritTypes::{
    ElementTypeId, HTMLElementTypeId, HTMLMediaElementTypeId, NodeTypeId,
};
use script_bindings::script_runtime::temp_cx;
use servo_config::pref;
use servo_media::player::audio::AudioRenderer;
use servo_url::BrowserUrl;
use stylo_atoms::Atom;
use uuid::Uuid;

use crate::document_loader::{LoadBlocker, LoadType};
use crate::dom::attr::Attr;
use crate::dom::audio::audiotracklist::AudioTrackList;
use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::HTMLMediaElementBinding::{
    CanPlayTypeResult, HTMLMediaElementConstants, HTMLMediaElementMethods,
};
use crate::dom::bindings::codegen::Bindings::MediaErrorBinding::MediaErrorConstants::*;
use crate::dom::bindings::codegen::Bindings::MediaErrorBinding::MediaErrorMethods;
use crate::dom::bindings::codegen::Bindings::NavigatorBinding::Navigator_Binding::NavigatorMethods;
use crate::dom::bindings::codegen::Bindings::NodeBinding::Node_Binding::NodeMethods;
use crate::dom::bindings::codegen::Bindings::TextTrackBinding::{TextTrackKind, TextTrackMode};
use crate::dom::bindings::codegen::Bindings::URLBinding::URLMethods;
use crate::dom::bindings::codegen::Bindings::WindowBinding::Window_Binding::WindowMethods;
use crate::dom::bindings::codegen::UnionTypes::{
    MediaStreamOrMediaSourceOrBlob, VideoTrackOrAudioTrackOrTextTrack,
};
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::num::Finite;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::DomGlobal;
use crate::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::blob::Blob;
use crate::dom::csp::{GlobalCspReporting, Violation};
use crate::dom::document::Document;
use crate::dom::element::{
    AttributeMutation, AttributeMutationReason, CustomElementCreationMode, Element, ElementCreator,
    cors_setting_for_element, reflect_cross_origin_attribute, set_cross_origin_attribute,
};
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::html::htmlelement::HTMLElement;
use crate::dom::html::htmlsourceelement::HTMLSourceElement;
use crate::dom::html::htmlvideoelement::HTMLVideoElement;
use crate::dom::mediaerror::MediaError;
use crate::dom::mediafragmentparser::MediaFragmentParser;
use crate::dom::medialist::MediaList;
use crate::dom::media::mediasource::MediaSource;
use crate::dom::mediastream::MediaStream;
use crate::dom::node::{Node, NodeDamage, NodeTraits, UnbindContext};
use crate::dom::performance::performanceresourcetiming::InitiatorType;
use crate::dom::promise::Promise;
use crate::dom::texttrack::TextTrack;
use crate::dom::texttracklist::TextTrackList;
use crate::dom::timeranges::{TimeRanges, TimeRangesContainer};
use crate::dom::url::URL;
use crate::dom::videotracklist::VideoTrackList;
use crate::dom::virtualmethods::VirtualMethods;
use crate::fetch::{FetchCanceller, RequestWithGlobalScope, create_a_potential_cors_request};
use crate::microtask::{Microtask, MicrotaskRunnable};
use crate::network_listener::{self, FetchResponseListener, ResourceTimingListener};
use crate::realms::enter_auto_realm;
use crate::script_runtime::CanGc;
use crate::script_thread::ScriptThread;

mod backend;
mod dom_api;
mod lifecycle;
mod playback;
mod resource;

use self::lifecycle::{CancelReason, HTMLMediaElementFetchListener};
pub(crate) use self::lifecycle::{HTMLMediaElementFetchContext, MediaElementMicrotask};

/// A CSS file to style the media controls.
static MEDIA_CONTROL_CSS: &str = include_str!("../../../resources/media-controls.css");

/// A JS file to control the media controls.
static MEDIA_CONTROL_JS: &str = include_str!("../../../resources/media-controls.js");

/// Keeps the current and poster frame for a video element.
#[derive(MallocSizeOf)]
pub(crate) struct VideoFrameState {
    pub current_frame: Option<MediaFrame>,
    pub poster_frame: Option<MediaFrame>,
}

impl VideoFrameState {
    fn new() -> Self {
        Self {
            current_frame: None,
            poster_frame: None,
        }
    }

    fn set_poster_frame(&mut self, image: Option<Arc<RasterImage>>) {
        self.poster_frame = image.and_then(|image| {
            image.id.map(|image_key| MediaFrame {
                image_key,
                width: image.metadata.width as i32,
                height: image.metadata.height as i32,
            })
        });
    }
}

#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
#[derive(JSTraceable, MallocSizeOf)]
enum SrcObject {
    MediaStream(Dom<MediaStream>),
    MediaSource(Dom<MediaSource>),
    Blob(Dom<Blob>),
}

impl From<MediaStreamOrMediaSourceOrBlob> for SrcObject {
    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    fn from(src_object: MediaStreamOrMediaSourceOrBlob) -> SrcObject {
        match src_object {
            MediaStreamOrMediaSourceOrBlob::Blob(blob) => SrcObject::Blob(Dom::from_ref(&*blob)),
            MediaStreamOrMediaSourceOrBlob::MediaSource(media_source) => {
                SrcObject::MediaSource(Dom::from_ref(&*media_source))
            },
            MediaStreamOrMediaSourceOrBlob::MediaStream(stream) => {
                SrcObject::MediaStream(Dom::from_ref(&*stream))
            },
        }
    }
}

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq)]
enum LoadState {
    NotLoaded,
    LoadingFromSrcObject,
    LoadingFromSrcAttribute,
    LoadingFromSourceChild,
    WaitingForSource,
}

/// <https://html.spec.whatwg.org/multipage/#loading-the-media-resource:media-element-29>
#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
#[derive(JSTraceable, MallocSizeOf)]
struct SourceChildrenPointer {
    source_before_pointer: Dom<HTMLSourceElement>,
    inclusive: bool,
}

impl SourceChildrenPointer {
    fn new(source_before_pointer: DomRoot<HTMLSourceElement>, inclusive: bool) -> Self {
        Self {
            source_before_pointer: source_before_pointer.as_traced(),
            inclusive,
        }
    }
}

/// Generally the presence of the loop attribute should be considered to mean playback has not
/// "ended", as "ended" and "looping" are mutually exclusive.
/// <https://html.spec.whatwg.org/multipage/#ended-playback>
#[derive(Clone, Copy, Debug, PartialEq)]
enum LoopCondition {
    Included,
    Ignored,
}

#[dom_struct]
pub(crate) struct HTMLMediaElement {
    htmlelement: HTMLElement,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-networkstate>
    network_state: Cell<NetworkState>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-readystate>
    ready_state: Cell<ReadyState>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-srcobject>
    src_object: DomRefCell<Option<SrcObject>>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-currentsrc>
    current_src: DomRefCell<String>,
    /// Incremented whenever tasks associated with this element are cancelled.
    generation_id: Cell<u32>,
    /// <https://html.spec.whatwg.org/multipage/#fire-loadeddata>
    ///
    /// Reset to false every time the load algorithm is invoked.
    fired_loadeddata_event: Cell<bool>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-error>
    error: MutNullableDom<MediaError>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-paused>
    paused: Cell<bool>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-defaultplaybackrate>
    default_playback_rate: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-playbackrate>
    playback_rate: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#attr-media-autoplay>
    autoplaying: Cell<bool>,
    /// <https://html.spec.whatwg.org/multipage/#delaying-the-load-event-flag>
    delaying_the_load_event_flag: DomRefCell<Option<LoadBlocker>>,
    /// <https://html.spec.whatwg.org/multipage/#list-of-pending-play-promises>
    #[conditional_malloc_size_of]
    pending_play_promises: DomRefCell<Vec<Rc<Promise>>>,
    /// Play promises which are soon to be fulfilled by a queued task.
    #[expect(clippy::type_complexity)]
    #[conditional_malloc_size_of]
    in_flight_play_promises_queue: DomRefCell<VecDeque<(Box<[Rc<Promise>]>, ErrorResult)>>,
    /// Makepad-based media controller (replaces servo-media Player).
    #[ignore_malloc_size_of = "media controller"]
    #[no_trace]
    media_controller: DomRefCell<Option<MediaController>>,
    /// Attached MediaSource object for current MSE playback.
    attached_media_source: DomRefCell<Option<Dom<MediaSource>>>,
    /// Current and poster video frame state for layout queries.
    #[conditional_malloc_size_of]
    #[no_trace]
    video_frame_state: Arc<std::sync::Mutex<VideoFrameState>>,
    /// Audio renderer for Web Audio tap (kept for Web Audio compatibility).
    #[ignore_malloc_size_of = "servo_media"]
    #[no_trace]
    audio_renderer: DomRefCell<Option<Arc<std::sync::Mutex<dyn AudioRenderer>>>>,
    /// <https://html.spec.whatwg.org/multipage/#show-poster-flag>
    show_poster: Cell<bool>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-duration>
    duration: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#current-playback-position>
    current_playback_position: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#official-playback-position>
    official_playback_position: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#default-playback-start-position>
    default_playback_start_position: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-volume>
    volume: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-seeking>
    seeking: Cell<bool>,
    /// The latest seek position (in seconds) is used to distinguish whether the seek request was
    /// initiated by a script or by the user agent itself, rather than by the media engine and to
    /// abort other running instance of the `seek` algorithm.
    current_seek_position: Cell<f64>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-muted>
    muted: Cell<bool>,
    /// Loading state from source, if any.
    load_state: Cell<LoadState>,
    source_children_pointer: DomRefCell<Option<SourceChildrenPointer>>,
    current_source_child: MutNullableDom<HTMLSourceElement>,
    /// URL of the media resource, if any.
    #[no_trace]
    resource_url: DomRefCell<Option<BrowserUrl>>,
    /// URL of the media resource, if the resource is set through the src_object attribute and it
    /// is a blob.
    #[no_trace]
    blob_url: DomRefCell<Option<BrowserUrl>>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-played>
    played: DomRefCell<TimeRangesContainer>,
    // https://html.spec.whatwg.org/multipage/#dom-media-audiotracks
    audio_tracks_list: MutNullableDom<AudioTrackList>,
    // https://html.spec.whatwg.org/multipage/#dom-media-videotracks
    video_tracks_list: MutNullableDom<VideoTrackList>,
    /// <https://html.spec.whatwg.org/multipage/#dom-media-texttracks>
    text_tracks_list: MutNullableDom<TextTrackList>,
    /// Time of last timeupdate notification.
    #[ignore_malloc_size_of = "Defined in std::time"]
    next_timeupdate_event: Cell<Instant>,
    /// Latest fetch request context.
    current_fetch_context: RefCell<Option<HTMLMediaElementFetchContext>>,
    /// Media controls id.
    /// In order to workaround the lack of privileged JS context, we secure the
    /// the access to the "privileged" document.servoGetMediaControls(id) API by
    /// keeping a whitelist of media controls identifiers.
    media_controls_id: DomRefCell<Option<String>>,
}

/// <https://html.spec.whatwg.org/multipage/#dom-media-networkstate>
#[derive(Clone, Copy, JSTraceable, MallocSizeOf, PartialEq)]
#[repr(u8)]
pub(crate) enum NetworkState {
    Empty = HTMLMediaElementConstants::NETWORK_EMPTY as u8,
    Idle = HTMLMediaElementConstants::NETWORK_IDLE as u8,
    Loading = HTMLMediaElementConstants::NETWORK_LOADING as u8,
    NoSource = HTMLMediaElementConstants::NETWORK_NO_SOURCE as u8,
}

/// <https://html.spec.whatwg.org/multipage/#dom-media-readystate>
#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq, PartialOrd)]
#[repr(u8)]
#[expect(clippy::enum_variant_names)] // Clippy warning silenced here because these names are from the specification.
pub(crate) enum ReadyState {
    HaveNothing = HTMLMediaElementConstants::HAVE_NOTHING as u8,
    HaveMetadata = HTMLMediaElementConstants::HAVE_METADATA as u8,
    HaveCurrentData = HTMLMediaElementConstants::HAVE_CURRENT_DATA as u8,
    HaveFutureData = HTMLMediaElementConstants::HAVE_FUTURE_DATA as u8,
    HaveEnoughData = HTMLMediaElementConstants::HAVE_ENOUGH_DATA as u8,
}

/// <https://html.spec.whatwg.org/multipage/#direction-of-playback>
#[derive(Clone, Copy, PartialEq)]
enum PlaybackDirection {
    Forwards,
    Backwards,
}

enum Resource {
    Object,
    Url(BrowserUrl),
}

impl HTMLMediaElement {
    pub(crate) fn new_inherited(
        tag_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> Self {
        Self {
            htmlelement: HTMLElement::new_inherited(tag_name, prefix, document),
            network_state: Cell::new(NetworkState::Empty),
            ready_state: Cell::new(ReadyState::HaveNothing),
            src_object: Default::default(),
            current_src: DomRefCell::new("".to_owned()),
            generation_id: Cell::new(0),
            fired_loadeddata_event: Cell::new(false),
            error: Default::default(),
            paused: Cell::new(true),
            default_playback_rate: Cell::new(1.0),
            playback_rate: Cell::new(1.0),
            muted: Cell::new(false),
            load_state: Cell::new(LoadState::NotLoaded),
            source_children_pointer: DomRefCell::new(None),
            current_source_child: Default::default(),
            // FIXME(nox): Why is this initialised to true?
            autoplaying: Cell::new(true),
            delaying_the_load_event_flag: Default::default(),
            pending_play_promises: Default::default(),
            in_flight_play_promises_queue: Default::default(),
            media_controller: Default::default(),
            attached_media_source: Default::default(),
            video_frame_state: Arc::new(std::sync::Mutex::new(VideoFrameState::new())),
            audio_renderer: Default::default(),
            show_poster: Cell::new(true),
            duration: Cell::new(f64::NAN),
            current_playback_position: Cell::new(0.),
            official_playback_position: Cell::new(0.),
            default_playback_start_position: Cell::new(0.),
            volume: Cell::new(1.0),
            seeking: Cell::new(false),
            current_seek_position: Cell::new(f64::NAN),
            resource_url: DomRefCell::new(None),
            blob_url: DomRefCell::new(None),
            played: DomRefCell::new(TimeRangesContainer::default()),
            audio_tracks_list: Default::default(),
            video_tracks_list: Default::default(),
            text_tracks_list: Default::default(),
            next_timeupdate_event: Cell::new(Instant::now() + Duration::from_millis(250)),
            current_fetch_context: RefCell::new(None),
            media_controls_id: DomRefCell::new(None),
        }
    }
    pub(crate) fn network_state(&self) -> NetworkState {
        self.network_state.get()
    }
    pub(crate) fn get_ready_state(&self) -> ReadyState {
        self.ready_state.get()
    }
    fn media_type_id(&self) -> HTMLMediaElementTypeId {
        match self.upcast::<Node>().type_id() {
            NodeTypeId::Element(ElementTypeId::HTMLElement(
                HTMLElementTypeId::HTMLMediaElement(media_type_id),
            )) => media_type_id,
            _ => unreachable!(),
        }
    }
    /// Marks that element as delaying the load event or not.
    ///
    /// Nothing happens if the element was already delaying the load event and
    /// we pass true to that method again.
    ///
    /// <https://html.spec.whatwg.org/multipage/#delaying-the-load-event-flag>
    pub(crate) fn delay_load_event(&self, delay: bool, cx: &mut js::context::JSContext) {
        let blocker = &self.delaying_the_load_event_flag;
        if delay && blocker.borrow().is_none() {
            *blocker.borrow_mut() = Some(LoadBlocker::new(&self.owner_document(), LoadType::Media));
        } else if !delay && blocker.borrow().is_some() {
            LoadBlocker::terminate(blocker, cx);
        }
    }
}
