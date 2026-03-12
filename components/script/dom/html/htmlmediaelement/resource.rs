use super::*;

impl HTMLMediaElement {
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn invoke_resource_selection_algorithm(&self, cx: &mut js::context::JSContext) {
        // Step 1. Set the element's networkState attribute to the NETWORK_NO_SOURCE value.
        self.network_state.set(NetworkState::NoSource);

        // Step 2. Set the element's show poster flag to true.
        self.show_poster.set(true);

        // Step 3. Set the media element's delaying-the-load-event flag to true (this delays the
        // load event).
        self.delay_load_event(true, cx);

        // Step 4. Await a stable state, allowing the task that invoked this algorithm to continue.
        // If the resource selection mode in the synchronous section is
        // "attribute", the URL of the resource to fetch is relative to the
        // media element's node document when the src attribute was last
        // changed, which is why we need to pass the base URL in the task
        // right here.
        let task = MediaElementMicrotask::ResourceSelection {
            elem: DomRoot::from_ref(self),
            generation_id: self.generation_id.get(),
            base_url: self.owner_document().base_url(),
        };

        // FIXME(nox): This will later call the resource_selection_algorithm_sync
        // method from below, if microtasks were trait objects, we would be able
        // to put the code directly in this method, without the boilerplate
        // indirections.
        ScriptThread::await_stable_state(Microtask::MediaElement(task));
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn resource_selection_algorithm_sync(
        &self,
        base_url: BrowserUrl,
        cx: &mut js::context::JSContext,
    ) {
        // TODO Step 5. If the media element's blocked-on-parser flag is false, then populate the
        // list of pending text tracks.
        // FIXME(ferjm): Implement blocked_on_parser logic
        // https://html.spec.whatwg.org/multipage/#blocked-on-parser
        // FIXME(nox): Maybe populate the list of pending text tracks.

        enum Mode {
            Object,
            Attribute(String),
            Children(DomRoot<HTMLSourceElement>),
        }

        // Step 6.
        let mode = if self.src_object.borrow().is_some() {
            // If the media element has an assigned media provider object, then let mode be object.
            Mode::Object
        } else if let Some(attribute) = self
            .upcast::<Element>()
            .get_attribute(&ns!(), &local_name!("src"))
        {
            // Otherwise, if the media element has no assigned media provider object but has a src
            // attribute, then let mode be attribute.
            Mode::Attribute((**attribute.value()).to_owned())
        } else if let Some(source) = self
            .upcast::<Node>()
            .children()
            .find_map(DomRoot::downcast::<HTMLSourceElement>)
        {
            // Otherwise, if the media element does not have an assigned media provider object and
            // does not have a src attribute, but does have a source element child, then let mode be
            // children and let candidate be the first such source element child in tree order.
            Mode::Children(source)
        } else {
            // Otherwise, the media element has no assigned media provider object and has neither a
            // src attribute nor a source element child:
            self.load_state.set(LoadState::NotLoaded);

            // Step 6.none.1. Set the networkState to NETWORK_EMPTY.
            self.network_state.set(NetworkState::Empty);

            // Step 6.none.2. Set the element's delaying-the-load-event flag to false. This stops
            // delaying the load event.
            self.delay_load_event(false, cx);

            // Step 6.none.3. End the synchronous section and return.
            return;
        };

        // Step 7. Set the media element's networkState to NETWORK_LOADING.
        self.network_state.set(NetworkState::Loading);

        // Step 8. Queue a media element task given the media element to fire an event named
        // loadstart at the media element.
        self.queue_media_element_task_to_fire_event(atom!("loadstart"));

        // Step 9. Run the appropriate steps from the following list:
        match mode {
            Mode::Object => {
                // => "If mode is object"
                self.load_from_src_object();
            },
            Mode::Attribute(src) => {
                // => "If mode is attribute"
                self.load_from_src_attribute(base_url, &src);
            },
            Mode::Children(source) => {
                // => "Otherwise (mode is children)""
                self.load_from_source_child(&source);
            },
        }
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn load_from_src_object(&self) {
        self.load_state.set(LoadState::LoadingFromSrcObject);

        // Step 9.object.1. Set the currentSrc attribute to the empty string.
        "".clone_into(&mut self.current_src.borrow_mut());

        // Step 9.object.3. Run the resource fetch algorithm with the assigned media
        // provider object. If that algorithm returns without aborting this one, then the
        // load failed.
        // Note that the resource fetch algorithm itself takes care of the cleanup in case
        // of failure itself.
        self.resource_fetch_algorithm(Resource::Object);
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn load_from_src_attribute(&self, base_url: BrowserUrl, src: &str) {
        self.load_state.set(LoadState::LoadingFromSrcAttribute);

        // Step 9.attribute.1. If the src attribute's value is the empty string, then end
        // the synchronous section, and jump down to the failed with attribute step below.
        if src.is_empty() {
            self.queue_dedicated_media_source_failure_steps();
            return;
        }

        // Step 9.attribute.2. Let urlRecord be the result of encoding-parsing a URL given
        // the src attribute's value, relative to the media element's node document when the
        // src attribute was last changed.
        let Ok(url_record) = base_url.join(src) else {
            self.queue_dedicated_media_source_failure_steps();
            return;
        };

        // Step 9.attribute.3. If urlRecord is not failure, then set the currentSrc
        // attribute to the result of applying the URL serializer to urlRecord.
        *self.current_src.borrow_mut() = url_record.as_str().into();

        // Step 9.attribute.5. If urlRecord is not failure, then run the resource fetch
        // algorithm with urlRecord. If that algorithm returns without aborting this one,
        // then the load failed.
        // Note that the resource fetch algorithm itself takes care
        // of the cleanup in case of failure itself.
        self.resource_fetch_algorithm(Resource::Url(url_record));
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn load_from_source_child(&self, source: &HTMLSourceElement) {
        self.load_state.set(LoadState::LoadingFromSourceChild);

        // Step 9.children.1. Let pointer be a position defined by two adjacent nodes in the media
        // element's child list, treating the start of the list (before the first child in the list,
        // if any) and end of the list (after the last child in the list, if any) as nodes in their
        // own right. One node is the node before pointer, and the other node is the node after
        // pointer. Initially, let pointer be the position between the candidate node and the next
        // node, if there are any, or the end of the list, if it is the last node.
        *self.source_children_pointer.borrow_mut() =
            Some(SourceChildrenPointer::new(DomRoot::from_ref(source), false));

        let element = source.upcast::<Element>();

        // Step 9.children.2. Process candidate: If candidate does not have a src attribute, or if
        // its src attribute's value is the empty string, then end the synchronous section, and jump
        // down to the failed with elements step below.
        let Some(src) = element
            .get_attribute(&ns!(), &local_name!("src"))
            .filter(|attribute| !attribute.value().is_empty())
        else {
            self.load_from_source_child_failure_steps(source);
            return;
        };

        // Step 9.children.3. If candidate has a media attribute whose value does not match the
        // environment, then end the synchronous section, and jump down to the failed with elements
        // step below.
        if let Some(media) = element.get_attribute(&ns!(), &local_name!("media")) {
            if !MediaList::matches_environment(&element.owner_document(), &media.value()) {
                self.load_from_source_child_failure_steps(source);
                return;
            }
        }

        // Step 9.children.4. Let urlRecord be the result of encoding-parsing a URL given
        // candidate's src attribute's value, relative to candidate's node document when the src
        // attribute was last changed.
        let Ok(url_record) = source.owner_document().base_url().join(&src.value()) else {
            // Step 9.children.5. If urlRecord is failure, then end the synchronous section,
            // and jump down to the failed with elements step below.
            self.load_from_source_child_failure_steps(source);
            return;
        };

        // Step 9.children.6. If candidate has a type attribute whose value, when parsed as a MIME
        // type (including any codecs described by the codecs parameter, for types that define that
        // parameter), represents a type that the user agent knows it cannot render, then end the
        // synchronous section, and jump down to the failed with elements step below.
        if let Some(type_) = element.get_attribute(&ns!(), &local_name!("type")) {
            if media::controller::can_play_type(&type_.value()) == "" {
                self.load_from_source_child_failure_steps(source);
                return;
            }
        }

        // Reset the media player before loading the next source child.
        self.reset_media_player();

        self.current_source_child.set(Some(source));

        // Step 9.children.7. Set the currentSrc attribute to the result of applying the URL
        // serializer to urlRecord.
        *self.current_src.borrow_mut() = url_record.as_str().into();

        // Step 9.children.9. Run the resource fetch algorithm with urlRecord. If that
        // algorithm returns without aborting this one, then the load failed.
        // Note that the resource fetch algorithm itself takes care
        // of the cleanup in case of failure itself.
        self.resource_fetch_algorithm(Resource::Url(url_record));
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn load_from_source_child_failure_steps(&self, source: &HTMLSourceElement) {
        // Step 9.children.10. Failed with elements: Queue a media element task given the media
        // element to fire an event named error at candidate.
        let trusted_this = Trusted::new(self);
        let trusted_source = Trusted::new(source);
        let generation_id = self.generation_id.get();

        self.owner_global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(queue_error_event: move |cx| {
                let this = trusted_this.root();
                if generation_id != this.generation_id.get() {
                    return;
                }

                let source = trusted_source.root();
                source.upcast::<EventTarget>().fire_event(atom!("error"), CanGc::from_cx(cx));
            }));

        // Step 9.children.11. Await a stable state.
        let task = MediaElementMicrotask::SelectNextSourceChild {
            elem: DomRoot::from_ref(self),
            generation_id: self.generation_id.get(),
        };

        ScriptThread::await_stable_state(Microtask::MediaElement(task));
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn select_next_source_child(&self, can_gc: CanGc) {
        // Step 9.children.12. Forget the media element's media-resource-specific tracks.
        self.AudioTracks(can_gc).clear();
        self.VideoTracks(can_gc).clear();

        // Step 9.children.13. Find next candidate: Let candidate be null.
        let mut source_candidate = None;

        // Step 9.children.14. Search loop: If the node after pointer is the end of the list, then
        // jump to the waiting step below.
        // Step 9.children.15. If the node after pointer is a source element, let candidate be that
        // element.
        // Step 9.children.16. Advance pointer so that the node before pointer is now the node that
        // was after pointer, and the node after pointer is the node after the node that used to be
        // after pointer, if any.
        if let Some(ref source_children_pointer) = *self.source_children_pointer.borrow() {
            // Note that shared implementation between opaque types from
            // `inclusively_following_siblings` and `following_siblings` if not possible due to
            // precise capturing.
            if source_children_pointer.inclusive {
                for next_sibling in source_children_pointer
                    .source_before_pointer
                    .upcast::<Node>()
                    .inclusively_following_siblings()
                {
                    if let Some(next_source) = DomRoot::downcast::<HTMLSourceElement>(next_sibling)
                    {
                        source_candidate = Some(next_source);
                        break;
                    }
                }
            } else {
                for next_sibling in source_children_pointer
                    .source_before_pointer
                    .upcast::<Node>()
                    .following_siblings()
                {
                    if let Some(next_source) = DomRoot::downcast::<HTMLSourceElement>(next_sibling)
                    {
                        source_candidate = Some(next_source);
                        break;
                    }
                }
            };
        }

        // Step 9.children.17. If candidate is null, jump back to the search loop step. Otherwise,
        // jump back to the process candidate step.
        if let Some(source_candidate) = source_candidate {
            self.load_from_source_child(&source_candidate);
            return;
        }

        self.load_state.set(LoadState::WaitingForSource);

        *self.source_children_pointer.borrow_mut() = None;

        // Step 9.children.18. Waiting: Set the element's networkState attribute to the
        // NETWORK_NO_SOURCE value.
        self.network_state.set(NetworkState::NoSource);

        // Step 9.children.19. Set the element's show poster flag to true.
        self.show_poster.set(true);

        // Step 9.children.20. Queue a media element task given the media element to set the
        // element's delaying-the-load-event flag to false. This stops delaying the load event.
        let this = Trusted::new(self);
        let generation_id = self.generation_id.get();

        self.owner_global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(queue_delay_load_event: move |cx| {
                let this = this.root();
                if generation_id != this.generation_id.get() {
                    return;
                }

                this.delay_load_event(false, cx);
            }));

        // Step 9.children.22. Wait until the node after pointer is a node other than the end of the
        // list. (This step might wait forever.)
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn resource_selection_algorithm_failure_steps(&self) {
        match self.load_state.get() {
            LoadState::LoadingFromSrcObject => {
                // Step 9.object.4. Failed with media provider: Reaching this step indicates that
                // the media resource failed to load. Take pending play promises and queue a media
                // element task given the media element to run the dedicated media source failure
                // steps with the result.
                self.queue_dedicated_media_source_failure_steps();
            },
            LoadState::LoadingFromSrcAttribute => {
                // Step 9.attribute.6. Failed with attribute: Reaching this step indicates that the
                // media resource failed to load or that urlRecord is failure. Take pending play
                // promises and queue a media element task given the media element to run the
                // dedicated media source failure steps with the result.
                self.queue_dedicated_media_source_failure_steps();
            },
            LoadState::LoadingFromSourceChild => {
                // Step 9.children.10. Failed with elements: Queue a media element task given the
                // media element to fire an event named error at candidate.
                if let Some(source) = self.current_source_child.take() {
                    self.load_from_source_child_failure_steps(&source);
                }
            },
            _ => {},
        }
    }
    pub(super) fn fetch_request(&self, offset: Option<u64>) {
        if self.resource_url.borrow().is_none() && self.blob_url.borrow().is_none() {
            error!("Missing request url");
            self.resource_selection_algorithm_failure_steps();
            return;
        }

        let document = self.owner_document();
        let destination = match self.media_type_id() {
            HTMLMediaElementTypeId::HTMLAudioElement => Destination::Audio,
            HTMLMediaElementTypeId::HTMLVideoElement => Destination::Video,
        };
        let mut headers = HeaderMap::new();
        if let Some(offset) = offset {
            // FIXME(eijebong): Use typed headers once we have a constructor for the range header
            headers.insert(
                header::RANGE,
                HeaderValue::from_str(&format!("bytes={offset}-")).unwrap(),
            );
        }
        let url = match self.resource_url.borrow().as_ref() {
            Some(url) => url.clone(),
            None => self.blob_url.borrow().as_ref().unwrap().clone(),
        };

        let cors_setting = cors_setting_for_element(self.upcast());
        let global = self.global();
        let request = create_a_potential_cors_request(
            Some(document.webview_id()),
            url.clone(),
            destination,
            cors_setting,
            None,
            global.get_referrer(),
        )
        .with_global_scope(&global)
        .headers(headers)
        .referrer_policy(document.get_referrer_policy());

        let mut current_fetch_context = self.current_fetch_context.borrow_mut();
        if let Some(ref mut current_fetch_context) = *current_fetch_context {
            current_fetch_context.cancel(CancelReason::Abort);
        }

        *current_fetch_context = Some(HTMLMediaElementFetchContext::new(
            request.id,
            global.core_resource_thread(),
        ));
        let listener =
            HTMLMediaElementFetchListener::new(self, request.id, url.clone(), offset.unwrap_or(0));

        self.owner_document().fetch_background(request, listener);
    }

    fn send_resolve_request_blocking(
        embedder_chan: embedder_traits::ScriptToEmbedderChan,
        webview_id: base::id::WebViewId,
        origin_url: String,
        request: embedder_traits::HpprResolveRequest,
    ) -> Result<embedder_traits::HpprResolveResponse, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let callback = GenericCallback::new(move |message| {
            let result = match message {
                Ok(HpprControlResponse::Resolve(response)) => Ok(response),
                Ok(HpprControlResponse::Error(error)) => {
                    Ok(embedder_traits::HpprResolveResponse::Error(error))
                },
                Ok(_) => Err("unexpected control response for resolve request".to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = tx.send(result);
        })
        .map_err(|error| error.to_string())?;

        embedder_chan
            .send(EmbedderMsg::HpprControlOperation(
                webview_id,
                origin_url,
                HpprControlRequest::Resolve(request),
                callback,
            ))
            .map_err(|error| error.to_string())?;

        rx.recv().map_err(|error| error.to_string())?
    }

    fn handle_resolved_hppr_asset_response(&self, url: BrowserUrl, response: HpprControlResponse) {
        let resolved = match response {
            HpprControlResponse::Resolve(embedder_traits::HpprResolveResponse::Media(resolved)) => {
                resolved
            },
            HpprControlResponse::Resolve(embedder_traits::HpprResolveResponse::Error(error)) => {
                info!("media: HPPR resolve failed url={} error={}", url, error);
                self.media_data_processing_failure_steps();
                return;
            },
            HpprControlResponse::Error(error) => {
                info!("media: HPPR resolve failed url={} error={}", url, error);
                self.media_data_processing_failure_steps();
                return;
            },
            _ => {
                self.media_data_processing_failure_steps();
                return;
            },
        };

        let Ok(packet) = Packet::parse(resolved.packet.into_boxed_slice()) else {
            self.media_data_processing_failure_steps();
            return;
        };

        let embedder_chan = self.owner_global().script_to_embedder_chan().clone();
        let webview_id = self.owner_window().webview_id();
        let origin_url = self.owner_global().get_url().to_string();
        let source = resolved.source.clone();
        let read_range = Arc::new(move |offset: u64, length: usize| {
            match Self::send_resolve_request_blocking(
                embedder_chan.clone(),
                webview_id,
                origin_url.clone(),
                embedder_traits::HpprResolveRequest::ReadBytes {
                    source: source.clone(),
                    offset,
                    length,
                },
            ) {
                Ok(embedder_traits::HpprResolveResponse::Bytes(bytes)) => Ok(bytes),
                Ok(embedder_traits::HpprResolveResponse::Error(error)) => Err(error),
                Ok(_) => Err("unexpected resolve response for byte read".to_string()),
                Err(error) => Err(error),
            }
        });
        let Ok(asset) = ResolvedHpprMediaAsset::from_packet(
            resolved.endpoint,
            resolved.is_repo,
            &packet,
            read_range,
        ) else {
            self.media_data_processing_failure_steps();
            return;
        };
        let Some(mime) = self.infer_resolved_custom_mime(&url, asset.asset().content_type()) else {
            self.media_data_processing_failure_steps();
            return;
        };
        if self.create_resolved_media_player(asset.into_asset(), mime).is_err() {
            self.media_data_processing_failure_steps();
            return;
        }

        self.upcast::<EventTarget>()
            .fire_event(atom!("progress"), CanGc::note());
        self.network_state.set(NetworkState::Idle);
        self.upcast::<EventTarget>()
            .fire_event(atom!("suspend"), CanGc::note());
    }

    fn resolve_hppr_media_asset(&self, url: BrowserUrl) {
        let mut current_fetch_context = self.current_fetch_context.borrow_mut();
        if let Some(ref mut current_fetch_context) = *current_fetch_context {
            current_fetch_context.cancel(CancelReason::Abort);
        }
        *current_fetch_context = None;
        drop(current_fetch_context);

        let task_source = self
            .owner_global()
            .task_manager()
            .media_element_task_source()
            .to_sendable();
        let trusted = Trusted::new(self);
        let generation_id = self.generation_id.get();
        let expected_url = url.clone();
        let callback = GenericCallback::new(move |message| {
            let trusted = trusted.clone();
            let url = expected_url.clone();
            task_source.queue(task!(resolve_hppr_media_asset: move || {
                let element = trusted.root();
                if element.generation_id.get() != generation_id {
                    return;
                }
                if element.resource_url.borrow().as_ref() != Some(&url) {
                    return;
                }
                match message {
                    Ok(response) => element.handle_resolved_hppr_asset_response(url, response),
                    Err(_) => element.media_data_processing_failure_steps(),
                }
            }));
        })
        .expect("Could not create HPPR media resolve callback");

        let window = self.owner_window();
        let origin_url = self.owner_global().get_url().to_string();
        window.send_to_embedder(EmbedderMsg::HpprControlOperation(
            window.webview_id(),
            origin_url,
            HpprControlRequest::Resolve(embedder_traits::HpprResolveRequest::Media {
                url: url.to_string(),
            }),
            callback,
        ));
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-resource>
    pub(super) fn resource_fetch_algorithm(&self, resource: Resource) {
        if let Resource::Url(url) = &resource {
            if let Some(media_source) = MediaSource::from_object_url(url) {
                self.attached_media_source
                    .borrow_mut()
                    .replace(Dom::from_ref(&*media_source));
                if media_source.attach_to_element(self, CanGc::note()).is_err() {
                    self.resource_selection_algorithm_failure_steps();
                }
                return;
            }
        }

        let uses_custom_resolved_playback = matches!(
            &resource,
            Resource::Url(url) if Self::should_use_custom_resolved_hppr_playback(url)
        );

        if !uses_custom_resolved_playback {
            if let Err(e) = self.create_media_player(&resource) {
                error!("Create media player error {:?}", e);
                self.resource_selection_algorithm_failure_steps();
                return;
            }
        }

        // Steps 1-2.
        // Unapplicable, the `resource` variable already conveys which mode
        // is in use.

        // Step 3.
        // FIXME(nox): Remove all media-resource-specific text tracks.

        // Step 5. Run the appropriate steps from the following list:
        match resource {
            Resource::Url(url) => {
                // Step 5.remote.1. Optionally, run the following substeps. This is the expected
                // behavior if the user agent intends to not attempt to fetch the resource until the
                // user requests it explicitly (e.g. as a way to implement the preload attribute's
                // none keyword).
                if self.Preload() == "none" && !self.autoplaying.get() {
                    // Step 5.remote.1.1. Set the networkState to NETWORK_IDLE.
                    self.network_state.set(NetworkState::Idle);

                    // Step 5.remote.1.2. Queue a media element task given the media element to fire
                    // an event named suspend at the element.
                    self.queue_media_element_task_to_fire_event(atom!("suspend"));

                    // Step 5.remote.1.3. Queue a media element task given the media element to set
                    // the element's delaying-the-load-event flag to false. This stops delaying the
                    // load event.
                    let this = Trusted::new(self);
                    let generation_id = self.generation_id.get();

                    self.owner_global()
                        .task_manager()
                        .media_element_task_source()
                        .queue(task!(queue_delay_load_event: move |cx| {
                            let this = this.root();
                            if generation_id != this.generation_id.get() {
                                return;
                            }

                            this.delay_load_event(false, cx);
                        }));

                    // TODO Steps 5.remote.1.4. Wait for the task to be run.
                    // FIXME(nox): Somehow we should wait for the task from previous
                    // step to be ran before continuing.

                    // TODO Steps 5.remote.1.5-5.remote.1.7.
                    // FIXME(nox): Wait for an implementation-defined event and
                    // then continue with the normal set of steps instead of just
                    // returning.
                    return;
                }

                let uses_custom_resolved_playback =
                    Self::should_use_custom_resolved_hppr_playback(&url);
                *self.resource_url.borrow_mut() = Some(url.clone());

                // Steps 5.remote.2-5.remote.8
                if uses_custom_resolved_playback {
                    self.resolve_hppr_media_asset(url);
                } else {
                    self.fetch_request(None);
                }
            },
            Resource::Object => {
                if let Some(ref src_object) = *self.src_object.borrow() {
                    match src_object {
                        SrcObject::Blob(blob) => {
                            let blob_url = URL::CreateObjectURL(
                                &self.global(),
                                BlobOrMediaSource::Blob(DomRoot::from_ref(&**blob)),
                            );
                            *self.blob_url.borrow_mut() =
                                Some(BrowserUrl::parse(&blob_url.str()).expect("infallible"));
                            self.fetch_request(None);
                        },
                        SrcObject::MediaStream(stream) => {
                            self.setup_media_stream(stream);
                        },
                    }
                }
            },
        }
    }
    pub(crate) fn handle_source_child_insertion(
        &self,
        source: &HTMLSourceElement,
        cx: &mut js::context::JSContext,
    ) {
        // <https://html.spec.whatwg.org/multipage/#the-source-element:html-element-insertion-steps>
        // Step 2. If parent is a media element that has no src attribute and whose networkState has
        // the value NETWORK_EMPTY, then invoke that media element's resource selection algorithm.
        if self.upcast::<Element>().has_attribute(&local_name!("src")) {
            return;
        }

        if self.network_state.get() == NetworkState::Empty {
            self.invoke_resource_selection_algorithm(cx);
            return;
        }

        // <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
        // Step 9.children.22. Wait until the node after pointer is a node other than the end of the
        // list. (This step might wait forever.)
        if self.load_state.get() != LoadState::WaitingForSource {
            return;
        }

        self.load_state.set(LoadState::LoadingFromSourceChild);

        *self.source_children_pointer.borrow_mut() =
            Some(SourceChildrenPointer::new(DomRoot::from_ref(source), true));

        // Step 9.children.23. Await a stable state.
        let task = MediaElementMicrotask::SelectNextSourceChildAfterWait {
            elem: DomRoot::from_ref(self),
            generation_id: self.generation_id.get(),
        };

        ScriptThread::await_stable_state(Microtask::MediaElement(task));
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-algorithm>
    pub(super) fn select_next_source_child_after_wait(&self, cx: &mut js::context::JSContext) {
        // Step 9.children.24. Set the element's delaying-the-load-event flag back to true (this
        // delays the load event again, in case it hasn't been fired yet).
        self.delay_load_event(true, cx);

        // Step 9.children.25. Set the networkState back to NETWORK_LOADING.
        self.network_state.set(NetworkState::Loading);

        // Step 9.children.26. Jump back to the find next candidate step above.
        self.select_next_source_child(CanGc::from_cx(cx));
    }
    /// <https://html.spec.whatwg.org/multipage/#media-data-processing-steps-list>
    /// => "If the media data cannot be fetched at all, due to network errors..."
    /// => "If the media data can be fetched but is found by inspection to be in an unsupported
    /// format, or can otherwise not be rendered at all"
    pub(super) fn media_data_processing_failure_steps(&self) {
        // Step 1. The user agent should cancel the fetching process.
        if let Some(ref mut current_fetch_context) = *self.current_fetch_context.borrow_mut() {
            current_fetch_context.cancel(CancelReason::Error);
        }

        // Step 2. Abort this subalgorithm, returning to the resource selection algorithm.
        self.resource_selection_algorithm_failure_steps();
    }
    /// <https://html.spec.whatwg.org/multipage/#media-data-processing-steps-list>
    /// => "If the connection is interrupted after some media data has been received..."
    /// => "If the media data is corrupted"
    pub(super) fn media_data_processing_fatal_steps(&self, error: u16, cx: &mut js::context::JSContext) {
        *self.source_children_pointer.borrow_mut() = None;
        self.current_source_child.set(None);

        // Step 1. The user agent should cancel the fetching process.
        if let Some(ref mut current_fetch_context) = *self.current_fetch_context.borrow_mut() {
            current_fetch_context.cancel(CancelReason::Error);
        }

        // Step 2. Set the error attribute to the result of creating a MediaError with
        // MEDIA_ERR_NETWORK/MEDIA_ERR_DECODE.
        self.error.set(Some(&*MediaError::new(
            &self.owner_window(),
            error,
            CanGc::from_cx(cx),
        )));

        // Step 3. Set the element's networkState attribute to the NETWORK_IDLE value.
        self.network_state.set(NetworkState::Idle);

        // Step 4. Set the element's delaying-the-load-event flag to false. This stops delaying
        // the load event.
        self.delay_load_event(false, cx);

        // Step 5. Fire an event named error at the media element.
        self.upcast::<EventTarget>()
            .fire_event(atom!("error"), CanGc::from_cx(cx));

        // Step 6. Abort the overall resource selection algorithm.
    }
    /// Queues a task to run the [dedicated media source failure steps][steps].
    ///
    /// [steps]: https://html.spec.whatwg.org/multipage/#dedicated-media-source-failure-steps
    pub(super) fn queue_dedicated_media_source_failure_steps(&self) {
        let this = Trusted::new(self);
        let generation_id = self.generation_id.get();
        self.take_pending_play_promises(Err(Error::NotSupported(None)));
        self.owner_global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(dedicated_media_source_failure_steps: move |cx| {
                let this = this.root();
                if generation_id != this.generation_id.get() {
                    return;
                }

                this.fulfill_in_flight_play_promises(|| {
                    // Step 1. Set the error attribute to the result of creating a MediaError with
                    // MEDIA_ERR_SRC_NOT_SUPPORTED.
                    this.error.set(Some(&*MediaError::new(
                        &this.owner_window(),
                        MEDIA_ERR_SRC_NOT_SUPPORTED, CanGc::from_cx(cx))));

                    // Step 2. Forget the media element's media-resource-specific tracks.
                    this.AudioTracks(CanGc::from_cx(cx)).clear();
                    this.VideoTracks(CanGc::from_cx(cx)).clear();

                    // Step 3. Set the element's networkState attribute to the NETWORK_NO_SOURCE
                    // value.
                    this.network_state.set(NetworkState::NoSource);

                    // Step 4. Set the element's show poster flag to true.
                    this.show_poster.set(true);

                    // Step 5. Fire an event named error at the media element.
                    this.upcast::<EventTarget>().fire_event(atom!("error"), CanGc::from_cx(cx));

                    this.reset_media_player();

                    // Step 6. Reject pending play promises with promises and a "NotSupportedError"
                    // DOMException.
                    // Done after running this closure in `fulfill_in_flight_play_promises`.
                });

                // Step 7. Set the element's delaying-the-load-event flag to false. This stops
                // delaying the load event.
                this.delay_load_event(false, cx);
            }));
    }
    pub(super) fn should_use_custom_resolved_hppr_playback(url: &BrowserUrl) -> bool {
        matches!(url.scheme(), "hppr" | "hppr-browse")
    }
    pub(super) fn infer_resolved_custom_mime(&self, url: &BrowserUrl, content_type: Option<&str>) -> Option<String> {
        let base = content_type
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match base {
            Some("video/mp4") | Some("video/x-m4v") => return Some("video/mp4".to_string()),
            Some("audio/mp4") | Some("audio/x-m4a") => return Some("audio/mp4".to_string()),
            Some(_) => return None,
            None => {}
        }

        let path = url.path().to_ascii_lowercase();
        if path.ends_with(".m4a") {
            return Some("audio/mp4".to_string());
        }
        if path.ends_with(".mp4") || path.ends_with(".m4v") {
            return Some(if matches!(
                self.media_type_id(),
                HTMLMediaElementTypeId::HTMLAudioElement
            ) {
                "audio/mp4"
            } else {
                "video/mp4"
            }
            .to_string());
        }
        None
    }
    /// Resolve a media resource to a MediaOrigin.
    pub(super) fn resolve_media_source(&self, resource: &Resource) -> Result<MediaOrigin, ()> {
        match resource {
            Resource::Url(url) => {
                let url_str = url.as_str();
                if let Some(rest) = url_str.strip_prefix("data:") {
                    // data: URL — decode base64 body
                    let comma = rest.find(',').ok_or(())?;
                    let mime = rest[..comma].split(';').next().unwrap_or("");
                    let encoded = &rest[comma + 1..];
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(encoded.trim())
                        .map_err(|_| ())?;
                    info!(
                        "media: resolved data url mime={} bytes={}",
                        mime,
                        bytes.len()
                    );
                    Ok(MediaOrigin::InMemory(std::sync::Arc::new(bytes)))
                } else if url_str.starts_with("file://") {
                    info!("media: resolved file source {}", &url_str[7..]);
                    Ok(MediaOrigin::Filesystem(url_str[7..].to_string()))
                } else {
                    info!("media: resolved network source {}", url_str);
                    Ok(MediaOrigin::Network(url_str.to_string()))
                }
            },
            Resource::Object => {
                let src_object = self.src_object.borrow();
                match src_object.as_ref().ok_or(())? {
                    SrcObject::Blob(blob) => {
                        let bytes = blob.get_bytes().map_err(|_| ())?;
                        info!("media: resolved blob source bytes={}", bytes.len());
                        Ok(MediaOrigin::InMemory(std::sync::Arc::new(bytes)))
                    },
                    SrcObject::MediaStream(_) => Err(()),
                }
            },
        }
    }
    /// <https://html.spec.whatwg.org/multipage/#concept-media-load-resource>
    pub(crate) fn origin_is_clean(&self) -> bool {
        // Step 5.local (media provider object).
        if self.src_object.borrow().is_some() {
            // The resource described by the current media resource, if any,
            // contains the media data. It is CORS-same-origin.
            return true;
        }

        // Step 5.remote (URL record).
        if self.resource_url.borrow().is_some() {
            // Update the media data with the contents
            // of response's unsafe response obtained in this fashion.
            // Response can be CORS-same-origin or CORS-cross-origin;
            if let Some(ref current_fetch_context) = *self.current_fetch_context.borrow() {
                return current_fetch_context.origin_is_clean();
            }
        }

        true
    }
}
