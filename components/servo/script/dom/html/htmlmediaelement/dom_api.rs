use super::*;

impl HTMLMediaElementMethods<crate::DomTypeHolder> for HTMLMediaElement {
    /// <https://html.spec.whatwg.org/multipage/#dom-media-networkstate>
    fn NetworkState(&self) -> u16 {
        self.network_state.get() as u16
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-readystate>
    fn ReadyState(&self) -> u16 {
        self.ready_state.get() as u16
    }

    // https://html.spec.whatwg.org/multipage/#dom-media-autoplay
    make_bool_getter!(Autoplay, "autoplay");
    // https://html.spec.whatwg.org/multipage/#dom-media-autoplay
    make_bool_setter!(SetAutoplay, "autoplay");

    // https://html.spec.whatwg.org/multipage/#attr-media-loop
    make_bool_getter!(Loop, "loop");
    // https://html.spec.whatwg.org/multipage/#attr-media-loop
    make_bool_setter!(SetLoop, "loop");

    // https://html.spec.whatwg.org/multipage/#dom-media-defaultmuted
    make_bool_getter!(DefaultMuted, "muted");
    // https://html.spec.whatwg.org/multipage/#dom-media-defaultmuted
    make_bool_setter!(SetDefaultMuted, "muted");

    // https://html.spec.whatwg.org/multipage/#dom-media-controls
    make_bool_getter!(Controls, "controls");
    // https://html.spec.whatwg.org/multipage/#dom-media-controls
    make_bool_setter!(SetControls, "controls");

    // https://html.spec.whatwg.org/multipage/#dom-media-src
    make_url_getter!(Src, "src");

    // https://html.spec.whatwg.org/multipage/#dom-media-src
    make_url_setter!(SetSrc, "src");

    /// <https://html.spec.whatwg.org/multipage/#dom-media-crossOrigin>
    fn GetCrossOrigin(&self) -> Option<DOMString> {
        reflect_cross_origin_attribute(self.upcast::<Element>())
    }
    /// <https://html.spec.whatwg.org/multipage/#dom-media-crossOrigin>
    fn SetCrossOrigin(&self, value: Option<DOMString>, can_gc: CanGc) {
        set_cross_origin_attribute(self.upcast::<Element>(), value, can_gc);
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-muted>
    fn Muted(&self) -> bool {
        self.muted.get()
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-muted>
    fn SetMuted(&self, value: bool) {
        if self.muted.get() == value {
            return;
        }

        self.muted.set(value);

        if let Some(ref mc) = *self.media_controller.borrow() {
            if value {
                mc.mute();
            } else {
                mc.unmute();
            }
        }

        // The user agent must queue a media element task given the media element to fire an event
        // named volumechange at the media element.
        self.queue_media_element_task_to_fire_event(atom!("volumechange"));

        // Then, if the media element is not allowed to play, the user agent must run the internal
        // pause steps for the media element.
        if !self.is_allowed_to_play() {
            self.internal_pause_steps();
        }
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-srcobject>
    fn GetSrcObject(&self) -> Option<MediaStreamOrMediaSourceOrBlob> {
        (*self.src_object.borrow())
            .as_ref()
            .map(|src_object| match src_object {
                SrcObject::Blob(blob) => {
                    MediaStreamOrMediaSourceOrBlob::Blob(DomRoot::from_ref(&**blob))
                },
                SrcObject::MediaSource(media_source) => {
                    MediaStreamOrMediaSourceOrBlob::MediaSource(DomRoot::from_ref(&**media_source))
                },
                SrcObject::MediaStream(stream) => {
                    MediaStreamOrMediaSourceOrBlob::MediaStream(DomRoot::from_ref(&**stream))
                },
            })
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-srcobject>
    fn SetSrcObject(
        &self,
        cx: &mut js::context::JSContext,
        value: Option<MediaStreamOrMediaSourceOrBlob>,
    ) {
        *self.src_object.borrow_mut() = value.map(|value| value.into());
        self.media_element_load_algorithm(cx);
    }

    // https://html.spec.whatwg.org/multipage/#attr-media-preload
    // Missing/Invalid values are user-agent defined.
    make_enumerated_getter!(
        Preload,
        "preload",
        "none" | "metadata" | "auto",
        missing => "auto",
        invalid => "auto"
    );

    // https://html.spec.whatwg.org/multipage/#attr-media-preload
    make_setter!(SetPreload, "preload");

    /// <https://html.spec.whatwg.org/multipage/#dom-media-currentsrc>
    fn CurrentSrc(&self) -> USVString {
        USVString(self.current_src.borrow().clone())
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-load>
    fn Load(&self, cx: &mut js::context::JSContext) {
        self.media_element_load_algorithm(cx);
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-navigator-canplaytype>
    fn CanPlayType(&self, type_: DOMString) -> CanPlayTypeResult {
        match crate::media::controller::can_play_type(&type_.str()) {
            "" => CanPlayTypeResult::_empty,
            "probably" => CanPlayTypeResult::Probably,
            _ => CanPlayTypeResult::Maybe,
        }
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-error>
    fn GetError(&self) -> Option<DomRoot<MediaError>> {
        self.error.get()
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-play>
    fn Play(&self, cx: &mut CurrentRealm) -> Rc<Promise> {
        let promise = Promise::new_in_realm(cx);

        // TODO Step 1. If the media element is not allowed to play, then return a promise rejected
        // with a "NotAllowedError" DOMException.

        // Step 2. If the media element's error attribute is not null and its code is
        // MEDIA_ERR_SRC_NOT_SUPPORTED, then return a promise rejected with a "NotSupportedError"
        // DOMException.
        if self
            .error
            .get()
            .is_some_and(|e| e.Code() == MEDIA_ERR_SRC_NOT_SUPPORTED)
        {
            promise.reject_error(Error::NotSupported(None), CanGc::from_cx(cx));
            return promise;
        }

        // Step 3. Let promise be a new promise and append promise to the list of pending play
        // promises.
        self.push_pending_play_promise(&promise);

        // Step 4. Run the internal play steps for the media element.
        self.internal_play_steps(cx);

        // Step 5. Return promise.
        promise
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-pause>
    fn Pause(&self, cx: &mut js::context::JSContext) {
        // Step 1. If the media element's networkState attribute has the value NETWORK_EMPTY, invoke
        // the media element's resource selection algorithm.
        if self.network_state.get() == NetworkState::Empty {
            self.invoke_resource_selection_algorithm(cx);
        }

        // Step 2. Run the internal pause steps for the media element.
        self.internal_pause_steps();
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-paused>
    fn Paused(&self) -> bool {
        self.paused.get()
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-defaultplaybackrate>
    fn GetDefaultPlaybackRate(&self) -> Fallible<Finite<f64>> {
        Ok(Finite::wrap(self.default_playback_rate.get()))
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-defaultplaybackrate>
    fn SetDefaultPlaybackRate(&self, value: Finite<f64>) -> ErrorResult {
        // If the given value is not supported by the user agent, then throw a "NotSupportedError"
        // DOMException.
        let min_allowed = -64.0;
        let max_allowed = 64.0;
        if *value < min_allowed || *value > max_allowed {
            return Err(Error::NotSupported(None));
        }

        if self.default_playback_rate.get() == *value {
            return Ok(());
        }

        self.default_playback_rate.set(*value);

        // The user agent must queue a media element task given the media element to fire an event
        // named ratechange at the media element.
        self.queue_media_element_task_to_fire_event(atom!("ratechange"));

        Ok(())
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-playbackrate>
    fn GetPlaybackRate(&self) -> Fallible<Finite<f64>> {
        Ok(Finite::wrap(self.playback_rate.get()))
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-playbackrate>
    fn SetPlaybackRate(&self, value: Finite<f64>) -> ErrorResult {
        // The attribute is mutable: on setting, the user agent must follow these steps:

        // Step 1. If the given value is not supported by the user agent, then throw a
        // "NotSupportedError" DOMException.
        let min_allowed = -64.0;
        let max_allowed = 64.0;
        if *value < min_allowed || *value > max_allowed {
            return Err(Error::NotSupported(None));
        }

        if self.playback_rate.get() == *value {
            return Ok(());
        }

        // Step 2. Set playbackRate to the new value, and if the element is potentially playing,
        // change the playback speed.
        self.playback_rate.set(*value);

        if self.is_potentially_playing() {
            if let Some(ref mc) = *self.media_controller.borrow() {
                mc.set_playback_rate(*value);
            }
        }

        // The user agent must queue a media element task given the media element to fire an event
        // named ratechange at the media element.
        self.queue_media_element_task_to_fire_event(atom!("ratechange"));

        Ok(())
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-duration>
    fn Duration(&self) -> f64 {
        self.duration.get()
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-currenttime>
    fn CurrentTime(&self) -> Finite<f64> {
        Finite::wrap(if self.default_playback_start_position.get() != 0. {
            self.default_playback_start_position.get()
        } else if self.seeking.get() {
            // Note that the other browsers do the similar (by checking `seeking` value or clamp the
            // `official` position to the earliest possible position, the duration, and the seekable
            // ranges.
            // <https://github.com/whatwg/html/issues/11773>
            self.current_seek_position.get()
        } else {
            self.official_playback_position.get()
        })
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-currenttime>
    fn SetCurrentTime(&self, time: Finite<f64>) {
        if self.ready_state.get() == ReadyState::HaveNothing {
            self.default_playback_start_position.set(*time);
        } else {
            self.official_playback_position.set(*time);
            self.seek(*time, /* approximate_for_speed */ false);
        }
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-seeking>
    fn Seeking(&self) -> bool {
        self.seeking.get()
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-ended>
    fn Ended(&self) -> bool {
        self.ended_playback(LoopCondition::Included)
            && self.direction_of_playback() == PlaybackDirection::Forwards
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-fastseek>
    fn FastSeek(&self, time: Finite<f64>) {
        self.seek(*time, /* approximate_for_speed */ true);
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-played>
    fn Played(&self, can_gc: CanGc) -> DomRoot<TimeRanges> {
        TimeRanges::new(
            self.global().as_window(),
            self.played.borrow().clone(),
            can_gc,
        )
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-seekable>
    fn Seekable(&self, can_gc: CanGc) -> DomRoot<TimeRanges> {
        TimeRanges::new(self.global().as_window(), self.seekable(), can_gc)
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-buffered>
    fn Buffered(&self, can_gc: CanGc) -> DomRoot<TimeRanges> {
        let mut buffered = TimeRangesContainer::default();
        if let Some(ref mc) = *self.media_controller.borrow() {
            for &(start, end) in &mc.buffered_ranges {
                let _ = buffered.add(start, end);
            }
        }
        TimeRanges::new(self.global().as_window(), buffered, can_gc)
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-audiotracks>
    fn AudioTracks(&self, can_gc: CanGc) -> DomRoot<AudioTrackList> {
        let window = self.owner_window();
        self.audio_tracks_list
            .or_init(|| AudioTrackList::new(&window, &[], Some(self), can_gc))
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-videotracks>
    fn VideoTracks(&self, can_gc: CanGc) -> DomRoot<VideoTrackList> {
        let window = self.owner_window();
        self.video_tracks_list
            .or_init(|| VideoTrackList::new(&window, &[], Some(self), can_gc))
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-texttracks>
    fn TextTracks(&self, can_gc: CanGc) -> DomRoot<TextTrackList> {
        let window = self.owner_window();
        self.text_tracks_list
            .or_init(|| TextTrackList::new(&window, &[], can_gc))
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-addtexttrack>
    fn AddTextTrack(
        &self,
        kind: TextTrackKind,
        label: DOMString,
        language: DOMString,
        can_gc: CanGc,
    ) -> DomRoot<TextTrack> {
        let window = self.owner_window();
        // Step 1 & 2
        // FIXME(#22314, dlrobertson) set the ready state to Loaded
        let track = TextTrack::new(
            &window,
            "".into(),
            kind,
            label,
            language,
            TextTrackMode::Hidden,
            None,
            can_gc,
        );
        // Step 3 & 4
        self.TextTracks(can_gc).add(&track);
        // Step 5
        DomRoot::from_ref(&track)
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-volume>
    fn GetVolume(&self) -> Fallible<Finite<f64>> {
        Ok(Finite::wrap(self.volume.get()))
    }

    /// <https://html.spec.whatwg.org/multipage/#dom-media-volume>
    fn SetVolume(&self, value: Finite<f64>) -> ErrorResult {
        // If the new value is outside the range 0.0 to 1.0 inclusive, then, on setting, an
        // "IndexSizeError" DOMException must be thrown instead.
        let minimum_volume = 0.0;
        let maximum_volume = 1.0;
        if *value < minimum_volume || *value > maximum_volume {
            return Err(Error::IndexSize(None));
        }

        if self.volume.get() == *value {
            return Ok(());
        }

        self.volume.set(*value);

        if let Some(ref mc) = *self.media_controller.borrow() {
            mc.set_volume(*value);
        }

        // The user agent must queue a media element task given the media element to fire an event
        // named volumechange at the media element.
        self.queue_media_element_task_to_fire_event(atom!("volumechange"));

        // Then, if the media element is not allowed to play, the user agent must run the internal
        // pause steps for the media element.
        if !self.is_allowed_to_play() {
            self.internal_pause_steps();
        }

        Ok(())
    }
}
