use super::*;

impl VirtualMethods for HTMLMediaElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<HTMLElement>() as &dyn VirtualMethods)
    }

    #[expect(unsafe_code)]
    fn attribute_mutated(&self, attr: &Attr, mutation: AttributeMutation, _can_gc: CanGc) {
        // TODO: https://github.com/servo/servo/issues/42812
        let mut cx = unsafe { temp_cx() };
        let cx = &mut cx;
        self.super_type()
            .unwrap()
            .attribute_mutated(attr, mutation, CanGc::from_cx(cx));

        match *attr.local_name() {
            local_name!("muted") => {
                // <https://html.spec.whatwg.org/multipage/#dom-media-muted>
                // When a media element is created, if the element has a muted content attribute
                // specified, then the muted IDL attribute should be set to true.
                if let AttributeMutation::Set(
                    _,
                    AttributeMutationReason::ByCloning | AttributeMutationReason::ByParser,
                ) = mutation
                {
                    self.SetMuted(true);
                }
            },
            local_name!("src") => {
                // <https://html.spec.whatwg.org/multipage/#location-of-the-media-resource>
                // If a src attribute of a media element is set or changed, the user agent must invoke
                // the media element's media element load algorithm (Removing the src attribute does
                // not do this, even if there are source elements present).
                if !mutation.is_removal() {
                    self.media_element_load_algorithm(cx);
                }
            },
            local_name!("controls") => {
                if mutation.new_value(attr).is_some() {
                    self.render_controls(CanGc::from_cx(cx));
                } else {
                    self.remove_controls();
                }
            },
            _ => (),
        };
    }

    /// <https://html.spec.whatwg.org/multipage/#playing-the-media-resource:remove-an-element-from-a-document>
    fn unbind_from_tree(&self, context: &UnbindContext, can_gc: CanGc) {
        self.super_type().unwrap().unbind_from_tree(context, can_gc);

        self.remove_controls();

        if context.tree_connected {
            let task = MediaElementMicrotask::PauseIfNotInDocument {
                elem: DomRoot::from_ref(self),
            };
            ScriptThread::await_stable_state(Microtask::MediaElement(task));
        }
    }

    fn adopting_steps(&self, old_doc: &Document, can_gc: CanGc) {
        self.super_type().unwrap().adopting_steps(old_doc, can_gc);

        // Note that media control id should be adopting between documents so "privileged"
        // document.servoGetMediaControls(id) API is keeping access to the whitelist of media
        // controls identifiers.
        if let Some(id) = &*self.media_controls_id.borrow() {
            let Some(shadow_root) = self.upcast::<Element>().shadow_root() else {
                error!("Missing media controls shadow root");
                return;
            };

            old_doc.unregister_media_controls(id);
            self.owner_document()
                .register_media_controls(id, &shadow_root);
        }
    }
}

#[derive(JSTraceable, MallocSizeOf)]
pub(crate) enum MediaElementMicrotask {
    ResourceSelection {
        elem: DomRoot<HTMLMediaElement>,
        generation_id: u32,
        #[no_trace]
        base_url: BrowserUrl,
    },
    PauseIfNotInDocument {
        elem: DomRoot<HTMLMediaElement>,
    },
    SelectNextSourceChild {
        elem: DomRoot<HTMLMediaElement>,
        generation_id: u32,
    },
    SelectNextSourceChildAfterWait {
        elem: DomRoot<HTMLMediaElement>,
        generation_id: u32,
    },
}

impl MicrotaskRunnable for MediaElementMicrotask {
    fn handler(&self, cx: &mut js::context::JSContext) {
        match self {
            &MediaElementMicrotask::ResourceSelection {
                ref elem,
                generation_id,
                ref base_url,
            } => {
                if generation_id == elem.generation_id.get() {
                    elem.resource_selection_algorithm_sync(base_url.clone(), cx);
                }
            },
            MediaElementMicrotask::PauseIfNotInDocument { elem } => {
                if !elem.upcast::<Node>().is_connected() {
                    elem.internal_pause_steps();
                }
            },
            &MediaElementMicrotask::SelectNextSourceChild {
                ref elem,
                generation_id,
            } => {
                if generation_id == elem.generation_id.get() {
                    elem.select_next_source_child(CanGc::from_cx(cx));
                }
            },
            &MediaElementMicrotask::SelectNextSourceChildAfterWait {
                ref elem,
                generation_id,
            } => {
                if generation_id == elem.generation_id.get() {
                    elem.select_next_source_child_after_wait(cx);
                }
            },
        }
    }

    fn enter_realm<'cx>(&self, cx: &'cx mut js::context::JSContext) -> AutoRealm<'cx> {
        match self {
            &MediaElementMicrotask::ResourceSelection { ref elem, .. }
            | &MediaElementMicrotask::PauseIfNotInDocument { ref elem }
            | &MediaElementMicrotask::SelectNextSourceChild { ref elem, .. }
            | &MediaElementMicrotask::SelectNextSourceChildAfterWait { ref elem, .. } => {
                enter_auto_realm(cx, &**elem)
            },
        }
    }
}

/// Indicates the reason why a fetch request was cancelled.
#[derive(Debug, MallocSizeOf, PartialEq)]
pub(super) enum CancelReason {
    /// An error ocurred while fetching the media data.
    Error,
    /// The fetching process is aborted by the user.
    Abort,
}

#[derive(MallocSizeOf)]
pub(crate) struct HTMLMediaElementFetchContext {
    /// The fetch request id.
    request_id: RequestId,
    /// Some if the request has been cancelled.
    cancel_reason: Option<CancelReason>,
    /// Indicates whether the fetched stream is seekable.
    is_seekable: bool,
    /// Indicates whether the fetched stream is origin clean.
    origin_clean: bool,
    /// Fetch canceller. Allows cancelling the current fetch request by
    /// manually calling its .cancel() method or automatically on Drop.
    fetch_canceller: FetchCanceller,
}

impl HTMLMediaElementFetchContext {
    pub(super) fn new(
        request_id: RequestId,
        core_resource_thread: CoreResourceThread,
    ) -> HTMLMediaElementFetchContext {
        HTMLMediaElementFetchContext {
            request_id,
            cancel_reason: None,
            is_seekable: false,
            origin_clean: true,
            fetch_canceller: FetchCanceller::new(request_id, false, core_resource_thread.clone()),
        }
    }

    pub(super) fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub(super) fn set_seekable(&mut self, seekable: bool) {
        self.is_seekable = seekable;
    }

    pub(super) fn origin_is_clean(&self) -> bool {
        self.origin_clean
    }

    pub(super) fn set_origin_clean(&mut self, origin_clean: bool) {
        self.origin_clean = origin_clean;
    }

    pub(super) fn cancel(&mut self, reason: CancelReason) {
        if self.cancel_reason.is_some() {
            return;
        }
        self.cancel_reason = Some(reason);
        self.fetch_canceller.abort();
    }

    pub(super) fn cancel_reason(&self) -> &Option<CancelReason> {
        &self.cancel_reason
    }
}

pub(super) struct HTMLMediaElementFetchListener {
    /// The element that initiated the request.
    element: Trusted<HTMLMediaElement>,
    /// The generation of the media element when this fetch started.
    generation_id: u32,
    /// The fetch request id.
    request_id: RequestId,
    /// Time of last progress notification.
    next_progress_event: Instant,
    /// Url for the resource.
    url: BrowserUrl,
    /// Expected content length of the media asset being fetched or played.
    expected_content_length: Option<u64>,
    /// Actual content length of the media asset was fetched.
    fetched_content_length: u64,
    /// Whether this fetch should hand browser-owned bytes into custom playback.
    buffer_response_body: bool,
    /// Buffered response bytes for custom baked playback.
    response_body: Vec<u8>,
    /// Response content type used to select the baked playback ingress path.
    content_type: Option<String>,
}

impl FetchResponseListener for HTMLMediaElementFetchListener {
    fn process_request_body(&mut self, _: RequestId) {}

    fn process_request_eof(&mut self, _: RequestId) {}

    #[expect(unsafe_code)]
    fn process_response(&mut self, _: RequestId, metadata: Result<FetchMetadata, NetworkError>) {
        // TODO: https://github.com/servo/servo/issues/42840
        let mut cx = unsafe { temp_cx() };
        let cx = &mut cx;
        let element = self.element.root();

        let (metadata, origin_clean) = match metadata {
            Ok(fetch_metadata) => match fetch_metadata {
                FetchMetadata::Unfiltered(metadata) => (Some(metadata), true),
                FetchMetadata::Filtered { filtered, unsafe_ } => (
                    Some(unsafe_),
                    matches!(
                        filtered,
                        FilteredMetadata::Basic(_) | FilteredMetadata::Cors(_)
                    ),
                ),
            },
            Err(_) => (None, true),
        };

        let (status_is_success, is_seekable) =
            metadata.as_ref().map_or((false, false), |metadata| {
                let status = &metadata.status;
                (status.is_success(), *status == StatusCode::PARTIAL_CONTENT)
            });

        // <https://html.spec.whatwg.org/multipage/#media-data-processing-steps-list>
        if !status_is_success {
            if element.ready_state.get() == ReadyState::HaveNothing {
                // => "If the media data cannot be fetched at all, due to network errors..."
                element.media_data_processing_failure_steps();
            } else {
                // => "If the connection is interrupted after some media data has been received..."
                element.media_data_processing_fatal_steps(MEDIA_ERR_NETWORK, cx);
            }
            return;
        }

        if let Some(ref mut current_fetch_context) = *element.current_fetch_context.borrow_mut() {
            current_fetch_context.set_seekable(is_seekable);
            current_fetch_context.set_origin_clean(origin_clean);
        }

        if let Some(metadata) = metadata.as_ref() {
            if let Some(headers) = metadata.headers.as_ref() {
                let content_length =
                    if let Some(content_range) = headers.typed_get::<ContentRange>() {
                        content_range.bytes_len()
                    } else {
                        headers
                            .typed_get::<ContentLength>()
                            .map(|content_length| content_length.0)
                    };

                if content_length != self.expected_content_length {
                    if let Some(content_length) = content_length {
                        self.expected_content_length = Some(content_length);
                    }
                }

                if self.buffer_response_body {
                    self.content_type = headers
                        .get(header::CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                }
            }
        }
    }

    fn process_response_chunk(&mut self, _: RequestId, chunk: Vec<u8>) {
        let element = self.element.root();

        self.fetched_content_length += chunk.len() as u64;
        if self.buffer_response_body {
            self.response_body.extend_from_slice(&chunk);
        }

        if Instant::now() > self.next_progress_event {
            element.queue_media_element_task_to_fire_event(atom!("progress"));
            self.next_progress_event = Instant::now() + Duration::from_millis(350);
        }
    }

    fn process_response_eof(
        mut self,
        cx: &mut js::context::JSContext,
        _: RequestId,
        status: Result<(), NetworkError>,
        timing: ResourceFetchTiming,
    ) {
        let element = self.element.root();

        // <https://html.spec.whatwg.org/multipage/#media-data-processing-steps-list>
        if status.is_ok() && self.fetched_content_length != 0 {
            if self.buffer_response_body {
                let Some(mime) = element.infer_baked_custom_mime(&self.url, self.content_type.as_deref()) else {
                    info!("media: baked fetch mime inference failed url={}", self.url);
                    element.media_data_processing_failure_steps();
                    network_listener::submit_timing(&self, &status, &timing, CanGc::from_cx(cx));
                    return;
                };
                let bytes = std::mem::take(&mut self.response_body);
                let asset = element.create_in_memory_baked_asset(bytes, Some(mime.clone()));
                if element.create_baked_media_player(asset, mime).is_err() {
                    info!("media: baked fetch create_baked_media_player failed url={}", self.url);
                    element.media_data_processing_failure_steps();
                    network_listener::submit_timing(&self, &status, &timing, CanGc::from_cx(cx));
                    return;
                }
            }

            element
                .upcast::<EventTarget>()
                .fire_event(atom!("progress"), CanGc::from_cx(cx));

            element.network_state.set(NetworkState::Idle);

            element
                .upcast::<EventTarget>()
                .fire_event(atom!("suspend"), CanGc::from_cx(cx));
        } else if status.is_err() && element.ready_state.get() != ReadyState::HaveNothing {
            // => "If the connection is interrupted after some media data has been received..."
            element.media_data_processing_fatal_steps(MEDIA_ERR_NETWORK, cx);
        } else {
            // => "If the media data can be fetched but is found by inspection to be in an
            // unsupported format, or can otherwise not be rendered at all"
            element.media_data_processing_failure_steps();
        }

        network_listener::submit_timing(&self, &status, &timing, CanGc::from_cx(cx));
    }

    fn process_csp_violations(&mut self, _request_id: RequestId, violations: Vec<Violation>) {
        let global = &self.resource_timing_global();
        global.report_csp_violations(violations, None, None);
    }

    fn should_invoke(&self) -> bool {
        let element = self.element.root();

        if element.generation_id.get() != self.generation_id {
            return false;
        }

        let Some(ref current_fetch_context) = *element.current_fetch_context.borrow() else {
            return false;
        };

        // Whether the new fetch request was triggered.
        if current_fetch_context.request_id() != self.request_id {
            return false;
        }

        // Whether the current fetch request was cancelled due to a network or decoding error, or
        // was aborted by the user.
        if let Some(cancel_reason) = current_fetch_context.cancel_reason() {
            if matches!(*cancel_reason, CancelReason::Error | CancelReason::Abort) {
                return false;
            }
        }

        true
    }
}

impl ResourceTimingListener for HTMLMediaElementFetchListener {
    fn resource_timing_information(&self) -> (InitiatorType, BrowserUrl) {
        let initiator_type = InitiatorType::LocalName(
            self.element
                .root()
                .upcast::<Element>()
                .local_name()
                .to_string(),
        );
        (initiator_type, self.url.clone())
    }

    fn resource_timing_global(&self) -> DomRoot<GlobalScope> {
        self.element.root().owner_document().global()
    }
}

impl HTMLMediaElementFetchListener {
    pub(super) fn new(
        element: &HTMLMediaElement,
        request_id: RequestId,
        url: BrowserUrl,
        _offset: u64,
    ) -> Self {
        Self {
            element: Trusted::new(element),
            generation_id: element.generation_id.get(),
            request_id,
            next_progress_event: Instant::now() + Duration::from_millis(350),
            buffer_response_body: HTMLMediaElement::should_use_custom_baked_hppr_playback(&url),
            response_body: Vec::new(),
            content_type: None,
            url,
            expected_content_length: None,
            fetched_content_length: 0,
        }
    }
}
