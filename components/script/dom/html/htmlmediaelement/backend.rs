use std::sync::Arc;

use media::{MediaAssetMetadata, MediaByteSource, ResolvedMediaAsset, clamp_byte_range};

use super::*;

struct InMemoryBakedByteSource {
    bytes: Vec<u8>,
}

impl InMemoryBakedByteSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl MediaByteSource for InMemoryBakedByteSource {
    fn read_range(&self, start: u64, len: usize) -> Result<Vec<u8>, String> {
        let Some((start, end)) = clamp_byte_range(self.bytes.len() as u64, start, len) else {
            return Ok(Vec::new());
        };
        Ok(self.bytes[start as usize..end as usize].to_vec())
    }
}

impl HTMLMediaElement {
    /// Set up a MediaStream as the source for this media element.
    ///
    /// Finds the video track's image key (registered in VideoTextureMap by
    /// the embedder) and wires it into `video_frame_state` so the existing
    /// renderer draws the camera frames.
    pub(super) fn setup_media_stream(&self, stream: &MediaStream) {
        use servo_media::streams::MediaStreamType;
        use webrender_api::IdNamespace;

        self.load_state.set(LoadState::LoadingFromSrcObject);

        // Find the first video track with an image key.
        let tracks = stream.get_tracks();
        let video_track = tracks
            .iter()
            .find(|t| t.ty() == MediaStreamType::Video);

        let video_track = match video_track {
            Some(t) => t,
            None => {
                // Audio-only stream — no video to render.
                self.change_ready_state(ReadyState::HaveEnoughData);
                return;
            },
        };

        let raw_key = match video_track.source().image_key() {
            Some(k) => k,
            None => {
                self.resource_selection_algorithm_failure_steps();
                return;
            },
        };

        // Build the webrender ImageKey from raw (namespace, index).
        let image_key = webrender_api::ImageKey(IdNamespace(raw_key.0), raw_key.1);

        // Dimensions will be updated when VideoPlaybackPrepared fires.
        // Use a placeholder for now; the renderer handles zero-size gracefully.
        self.video_frame_state.lock().unwrap().current_frame = Some(MediaFrame {
            image_key,
            width: 0,
            height: 0,
        });

        // Transition to a ready state so playback can proceed.
        self.change_ready_state(ReadyState::HaveMetadata);
        self.change_ready_state(ReadyState::HaveEnoughData);
        self.show_poster.set(false);

        self.upcast::<Node>().dirty(NodeDamage::Other);
    }
    /// <https://html.spec.whatwg.org/multipage/#poster-frame>
    pub(crate) fn set_poster_frame(&self, image: Option<Arc<RasterImage>>) {
        if pref!(media_testing_enabled) && image.is_some() {
            self.queue_media_element_task_to_fire_event(atom!("postershown"));
        }

        self.video_frame_state
            .lock()
            .unwrap()
            .set_poster_frame(image);

        self.upcast::<Node>().dirty(NodeDamage::Other);
    }
    pub(super) fn install_media_controller(&self, controller: MediaController) {
        let video_id = controller.video_id;
        info!("media: controller ready video_id={}", video_id);
        *self.media_controller.borrow_mut() = Some(controller);

        let (event_tx, event_rx) = crossbeam_channel::unbounded::<MediaEvent>();
        register_event_sender(video_id, event_tx);

        let task_source = self
            .owner_global()
            .task_manager()
            .media_element_task_source()
            .to_sendable();
        let trusted_self = crate::dom::bindings::refcounted::Trusted::new(self);
        let generation_id = self.generation_id.get();

        std::thread::Builder::new()
            .name(format!("media-bridge-{video_id}"))
            .spawn(move || {
                for event in event_rx {
                    let trusted = trusted_self.clone();
                    let ev = event.clone();
                    let gen_id = generation_id;
                    task_source.queue(task!(handle_makepad_media_event: move |cx| {
                        let element = trusted.root();
                        if element.generation_id.get() == gen_id {
                            element.handle_makepad_event(ev, CanGc::from_cx(cx));
                        }
                    }));
                }
            })
            .ok();
    }
    pub(super) fn create_media_player(&self, resource: &Resource) -> Result<(), ()> {
        // MediaStream sources use the camera texture path, not a media player.
        if let Resource::Object = resource {
            if let Some(SrcObject::MediaStream(_)) = self.src_object.borrow().as_ref() {
                return Err(());
            }
        }

        let source = self.resolve_media_source(resource)?;
        let window = self.owner_window();
        let webview_id = self.owner_document().webview_id();
        let autoplay = self.Autoplay();
        let should_loop = self.Loop();
        let muted = self.muted.get();

        let is_video = matches!(
            self.media_type_id(),
            HTMLMediaElementTypeId::HTMLVideoElement
        );
        let source_kind = match &source {
            MediaOrigin::InMemory(_) => "memory",
            MediaOrigin::Network(_) => "network",
            MediaOrigin::Filesystem(_) => "file",
        };

        info!(
            "media: create player kind={} source={} autoplay={} loop={} muted={}",
            if is_video { "video" } else { "audio" },
            source_kind,
            autoplay,
            should_loop,
            muted,
        );

        let controller = if is_video {
            let image_key = window
                .paint_api()
                .generate_image_key_blocking(webview_id)
                .map(|k| (k.0.0, k.1))
                .unwrap_or((0, 0));

            info!("media: video image_key={:?}", image_key);
            MediaController::new_video(source, image_key, autoplay, should_loop)
        } else {
            MediaController::new_audio(source, autoplay, should_loop)
        };

        if muted {
            controller.mute();
        }
        self.install_media_controller(controller);
        Ok(())
    }
    pub(super) fn create_in_memory_baked_asset(
        &self,
        bytes: Vec<u8>,
        content_type: Option<String>,
    ) -> ResolvedMediaAsset {
        let content_length = bytes.len() as u64;
        ResolvedMediaAsset::new(
            MediaAssetMetadata::new(content_length, content_type),
            Arc::new(InMemoryBakedByteSource::new(bytes)),
        )
    }

    pub(crate) fn create_mse_media_player(
        &self,
        media_source: &MediaSource,
        mime: String,
    ) -> Result<u64, Error> {
        let autoplay = self.Autoplay();
        let should_loop = self.Loop();
        let muted = self.muted.get();
        let image_key = if matches!(
            self.media_type_id(),
            HTMLMediaElementTypeId::HTMLVideoElement
        ) {
            let window = self.owner_window();
            let webview_id = self.owner_document().webview_id();
            Some(
                window
                    .paint_api()
                    .generate_image_key_blocking(webview_id)
                    .map(|k| (k.0.0, k.1))
                    .unwrap_or((0, 0)),
            )
        } else {
            None
        };

        info!(
            "media: create MSE player kind={} mime={} image_key={:?} autoplay={} loop={} muted={}",
            if image_key.is_some() { "video" } else { "audio" },
            mime,
            image_key,
            autoplay,
            should_loop,
            muted,
        );

        let controller = MediaController::new_mse_playback(
            mime,
            image_key,
            autoplay,
            should_loop,
        );
        let video_id = controller.video_id;
        if muted {
            controller.mute();
        }
        self.attached_media_source
            .borrow_mut()
            .replace(Dom::from_ref(media_source));
        self.install_media_controller(controller);
        Ok(video_id)
    }
    pub(super) fn create_baked_media_player(
        &self,
        asset: ResolvedMediaAsset,
        mime: String,
    ) -> Result<(), ()> {
        let autoplay = self.Autoplay();
        let should_loop = self.Loop();
        let muted = self.muted.get();
        let image_key = if matches!(
            self.media_type_id(),
            HTMLMediaElementTypeId::HTMLVideoElement
        ) {
            let window = self.owner_window();
            let webview_id = self.owner_document().webview_id();
            let image_key = window
                .paint_api()
                .generate_image_key_blocking(webview_id)
                .map(|k| (k.0.0, k.1))
                .unwrap_or((0, 0));
            info!("media: baked video image_key={:?}", image_key);
            Some(image_key)
        } else {
            None
        };

        info!(
            "media: create baked player kind={} mime={} bytes={} autoplay={} loop={} muted={}",
            if image_key.is_some() { "video" } else { "audio" },
            mime,
            asset.content_length(),
            autoplay,
            should_loop,
            muted,
        );

        let controller = MediaController::new_baked_playback(
            asset,
            mime,
            image_key,
            autoplay,
            should_loop,
        );
        if muted {
            controller.mute();
        }
        self.install_media_controller(controller);
        Ok(())
    }
    pub(crate) fn apply_media_source_duration(&self, duration: f64) {
        let old_duration = self.duration.get();
        let changed = !(old_duration.is_nan() && duration.is_nan()) && old_duration != duration;
        if !changed {
            return;
        }
        self.duration.set(duration);
        self.queue_media_element_task_to_fire_event(atom!("durationchange"));
    }
    pub(super) fn reset_media_player(&self) {
        if self.media_controller.borrow().is_none() {
            return;
        }

        // cleanup() sends CxOsOp::Cleanup and deregisters event sender.
        if let Some(mc) = self.media_controller.borrow().as_ref() {
            mc.cleanup();
        }
        *self.media_controller.borrow_mut() = None;
        self.video_frame_state.lock().unwrap().current_frame = None;

        if let Some(video_element) = self.downcast::<HTMLVideoElement>() {
            video_element.set_natural_dimensions(None, None);
        }
    }
    /// Handle a MediaEvent arriving from the Makepad platform backend.
    /// Called on the script thread via the bridge task.
    #[expect(unsafe_code)]
    pub(crate) fn handle_makepad_event(&self, event: MediaEvent, can_gc: CanGc) {
        // Update controller state first.
        if let Some(mc) = self.media_controller.borrow_mut().as_mut() {
            mc.apply_event(&event);
        }

        match event {
            MediaEvent::Prepared {
                width,
                height,
                duration_ms,
                is_seekable,
                ref video_tracks,
                ref audio_tracks,
            } => {
                // Build a Metadata-like structure and reuse playback_metadata_updated logic.
                self.handle_prepared(
                    width,
                    height,
                    duration_ms,
                    is_seekable,
                    video_tracks,
                    audio_tracks,
                    can_gc,
                );
            },
            MediaEvent::PositionChanged(pos_ms) => {
                self.playback_position_changed(pos_ms as f64 / 1000.0);
            },
            MediaEvent::PlaybackCompleted => {
                self.playback_end();
            },
            MediaEvent::Error(ref msg) => {
                let mut cx = unsafe { script_bindings::script_runtime::temp_cx() };
                self.playback_error(msg, &mut cx);
            },
            MediaEvent::SeekableRanges(_) | MediaEvent::BufferedRanges(_) => {
                // Ranges stored in controller.apply_event; layout queries use them.
            },
            MediaEvent::MseAppendDone { .. }
            | MediaEvent::MseInitSegmentParsed { .. }
            | MediaEvent::MseError(_) => {
                let attached_media_source = self
                    .attached_media_source
                    .borrow()
                    .as_ref()
                    .map(|media_source| media_source.as_rooted());
                if let Some(media_source) = attached_media_source {
                    media_source.handle_media_event(&event, can_gc);
                }
            },
        }
    }
    /// Process a Prepared event: set up tracks, dimensions, duration, readyState.
    pub(super) fn handle_prepared(
        &self,
        width: u32,
        height: u32,
        duration_ms: u128,
        _is_seekable: bool,
        video_track_names: &[String],
        audio_track_names: &[String],
        can_gc: CanGc,
    ) {
        if self.ready_state.get() != ReadyState::HaveNothing {
            return;
        }

        for (i, _) in audio_track_names.iter().enumerate() {
            let audio_track_list = self.AudioTracks(can_gc);
            let kind = if i == 0 {
                DOMString::from("main")
            } else {
                DOMString::new()
            };
            let audio_track = crate::dom::audio::audiotrack::AudioTrack::new(
                self.global().as_window(),
                DOMString::new(),
                kind,
                DOMString::new(),
                DOMString::new(),
                Some(&*audio_track_list),
                can_gc,
            );
            audio_track_list.add(&audio_track);
            if let Some(servo_url) = self.resource_url.borrow().as_ref() {
                let fragment = MediaFragmentParser::from(servo_url);
                if let Some(id) = fragment.id() {
                    if audio_track.id() == id {
                        audio_track_list.set_enabled(audio_track_list.len() - 1, true);
                    }
                }
                if fragment.tracks().contains(&audio_track.kind().into()) {
                    audio_track_list.set_enabled(audio_track_list.len() - 1, true);
                }
            }
            if audio_track_list.enabled_index().is_none() {
                audio_track_list.set_enabled(audio_track_list.len() - 1, true);
            }
            let event = crate::dom::trackevent::TrackEvent::new(
                self.global().as_window(),
                atom!("addtrack"),
                false,
                false,
                &Some(VideoTrackOrAudioTrackOrTextTrack::AudioTrack(audio_track)),
                can_gc,
            );
            event.upcast::<crate::dom::event::Event>().fire(
                audio_track_list.upcast::<crate::dom::eventtarget::EventTarget>(),
                can_gc,
            );
        }

        for (i, _) in video_track_names.iter().enumerate() {
            let video_track_list = self.VideoTracks(can_gc);
            let kind = if i == 0 {
                DOMString::from("main")
            } else {
                DOMString::new()
            };
            let video_track = crate::dom::videotrack::VideoTrack::new(
                self.global().as_window(),
                DOMString::new(),
                kind,
                DOMString::new(),
                DOMString::new(),
                Some(&*video_track_list),
                can_gc,
            );
            video_track_list.add(&video_track);
            if let Some(track) = video_track_list.item(0) {
                if let Some(servo_url) = self.resource_url.borrow().as_ref() {
                    let fragment = MediaFragmentParser::from(servo_url);
                    if let Some(id) = fragment.id() {
                        if track.id() == id {
                            video_track_list.set_selected(0, true);
                        }
                    } else if fragment.tracks().contains(&track.kind().into()) {
                        video_track_list.set_selected(0, true);
                    }
                }
            }
            if video_track_list.selected_index().is_none() {
                video_track_list.set_selected(video_track_list.len() - 1, true);
            }
            let event = crate::dom::trackevent::TrackEvent::new(
                self.global().as_window(),
                atom!("addtrack"),
                false,
                false,
                &Some(VideoTrackOrAudioTrackOrTextTrack::VideoTrack(video_track)),
                can_gc,
            );
            event.upcast::<crate::dom::event::Event>().fire(
                video_track_list.upcast::<crate::dom::eventtarget::EventTarget>(),
                can_gc,
            );
        }

        // Set current playback positions to earliest possible.
        self.current_playback_position.set(0.0);
        self.official_playback_position.set(0.0);

        // Update duration.
        let dur_secs = if let Some(media_source) = self
            .attached_media_source
            .borrow()
            .as_ref()
            .map(|media_source| media_source.as_rooted())
        {
            media_source.effective_media_element_duration(duration_ms)
        } else if duration_ms == 0 {
            f64::INFINITY
        } else {
            duration_ms as f64 / 1000.0
        };
        self.apply_media_source_duration(dur_secs);

        // Update video element dimensions.
        if let Some(video_element) = self.downcast::<HTMLVideoElement>() {
            if width > 0 && height > 0 {
                video_element.set_natural_dimensions(Some(width), Some(height));

                // Update the current frame for layout.
                if let Some(mc) = self.media_controller.borrow().as_ref() {
                    if mc.image_key != (0, 0) {
                        // Construct the ImageKey for the MediaFrame.
                        use webrender_api::IdNamespace;
                        let image_key =
                            webrender_api::ImageKey(IdNamespace(mc.image_key.0), mc.image_key.1);
                        self.video_frame_state.lock().unwrap().current_frame = Some(MediaFrame {
                            image_key,
                            width: width as i32,
                            height: height as i32,
                        });
                    }
                }
            }
            self.queue_media_element_task_to_fire_event(atom!("resize"));
        }

        self.change_ready_state(ReadyState::HaveMetadata);

        if let Some(servo_url) = self.resource_url.borrow().as_ref() {
            let fragment = MediaFragmentParser::from(servo_url);
            if let Some(initial_playback_position) = fragment.start() {
                if initial_playback_position > 0.0 && initial_playback_position < self.duration.get() {
                    self.seek(initial_playback_position, /* approximate_for_speed */ false);
                }
            }
        }

        self.change_ready_state(ReadyState::HaveEnoughData);

        // Keep platform playback state in sync with HTMLMediaElement paused state.
        // This ensures autoplay and early play() calls start decoding after metadata
        // is ready, even if backend-level autoplay was not triggered.
        if !self.Paused() {
            if let Some(mc) = self.media_controller.borrow_mut().as_mut() {
                info!("media: begin playback video_id={}", mc.video_id);
                mc.play();
            }
        }
    }
    pub(crate) fn set_audio_track(&self, _idx: usize, _enabled: bool) {
        // Track selection not yet supported via Makepad.
    }
    pub(crate) fn set_video_track(&self, _idx: usize, _enabled: bool) {
        // Track selection not yet supported via Makepad.
    }
    pub(super) fn render_controls(&self, can_gc: CanGc) {
        if self.upcast::<Element>().is_shadow_host() {
            // Bail out if we are already showing the controls.
            return;
        }

        // FIXME(stevennovaryo): Recheck styling of media element to avoid
        //                       reparsing styles.
        let shadow_root = self
            .upcast::<Element>()
            .attach_ua_shadow_root(false, can_gc);
        let document = self.owner_document();
        let script = Element::create(
            QualName::new(None, ns!(html), local_name!("script")),
            None,
            &document,
            ElementCreator::ScriptCreated,
            CustomElementCreationMode::Asynchronous,
            None,
            can_gc,
        );
        // This is our hacky way to temporarily workaround the lack of a privileged
        // JS context.
        // The media controls UI accesses the document.servoGetMediaControls(id) API
        // to get an instance to the media controls ShadowRoot.
        // `id` needs to match the internally generated UUID assigned to a media element.
        let id = Uuid::new_v4().to_string();
        document.register_media_controls(&id, &shadow_root);
        let media_controls_script = MEDIA_CONTROL_JS.replace("@@@id@@@", &id);
        *self.media_controls_id.borrow_mut() = Some(id);
        script
            .upcast::<Node>()
            .set_text_content_for_element(Some(DOMString::from(media_controls_script)), can_gc);
        if let Err(e) = shadow_root
            .upcast::<Node>()
            .AppendChild(script.upcast::<Node>(), can_gc)
        {
            warn!("Could not render media controls {:?}", e);
            return;
        }

        let style = Element::create(
            QualName::new(None, ns!(html), local_name!("style")),
            None,
            &document,
            ElementCreator::ScriptCreated,
            CustomElementCreationMode::Asynchronous,
            None,
            can_gc,
        );

        style
            .upcast::<Node>()
            .set_text_content_for_element(Some(DOMString::from(MEDIA_CONTROL_CSS)), can_gc);

        if let Err(e) = shadow_root
            .upcast::<Node>()
            .AppendChild(style.upcast::<Node>(), can_gc)
        {
            warn!("Could not render media controls {:?}", e);
        }

        self.upcast::<Node>().dirty(NodeDamage::Other);
    }
    pub(super) fn remove_controls(&self) {
        if let Some(id) = self.media_controls_id.borrow_mut().take() {
            self.owner_document().unregister_media_controls(&id);
        }
    }
    /// Gets the current frame of the video element to present, if any.
    /// <https://html.spec.whatwg.org/multipage/#the-video-element:the-video-element-7>
    pub(crate) fn get_current_frame_to_present(&self) -> Option<MediaFrame> {
        let state = self.video_frame_state.lock().unwrap();
        let current_frame = state.current_frame;
        let poster_frame = state.poster_frame;

        if (self.show_poster.get() || current_frame.is_none()) && poster_frame.is_some() {
            return poster_frame;
        }

        current_frame
    }
    /// By default the audio is rendered through the audio sink automatically
    /// selected by the servo-media Player instance. However, in some cases, like
    /// the WebAudio MediaElementAudioSourceNode, we need to set a custom audio
    /// renderer.
    pub(crate) fn set_audio_renderer(
        &self,
        audio_renderer: Option<Arc<std::sync::Mutex<dyn AudioRenderer>>>,
        cx: &mut js::context::JSContext,
    ) {
        *self.audio_renderer.borrow_mut() = audio_renderer;

        let had_controller = self.media_controller.borrow().is_some();

        if had_controller {
            self.reset_media_player();
            self.media_element_load_algorithm(cx);
        }
    }
    pub(super) fn send_media_session_event(&self, event: MediaSessionEvent) {
        let global = self.global();
        let media_session = global.as_window().Navigator().MediaSession();

        media_session.register_media_instance(self);

        media_session.send_event(event);
    }
}
