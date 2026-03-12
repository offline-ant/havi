use super::*;

impl HTMLMediaElement {
    pub(super) fn update_media_state(&self) {
        let is_playing = self
            .media_controller
            .borrow()
            .as_ref()
            .is_some_and(|mc| !mc.paused);

        if self.is_potentially_playing() && !is_playing {
            if let Some(ref mut mc) = *self.media_controller.borrow_mut() {
                mc.set_playback_rate(self.playback_rate.get());
                mc.set_volume(self.volume.get());
                mc.play();
            }
        } else if is_playing {
            if let Some(ref mut mc) = *self.media_controller.borrow_mut() {
                mc.pause();
            }
        }
    }
    /// <https://html.spec.whatwg.org/multipage/#time-marches-on>
    pub(super) fn time_marches_on(&self) {
        // Step 6. If the time was reached through the usual monotonic increase of the current
        // playback position during normal playback, and if the user agent has not fired a
        // timeupdate event at the element in the past 15 to 250ms and is not still running event
        // handlers for such an event, then the user agent must queue a media element task given the
        // media element to fire an event named timeupdate at the element.
        if Instant::now() > self.next_timeupdate_event.get() {
            self.queue_media_element_task_to_fire_event(atom!("timeupdate"));
            self.next_timeupdate_event
                .set(Instant::now() + Duration::from_millis(250));
        }
    }
    /// <https://html.spec.whatwg.org/multipage/#internal-play-steps>
    pub(super) fn internal_play_steps(&self, cx: &mut js::context::JSContext) {
        // Step 1. If the media element's networkState attribute has the value NETWORK_EMPTY, invoke
        // the media element's resource selection algorithm.
        if self.network_state.get() == NetworkState::Empty {
            self.invoke_resource_selection_algorithm(cx);
        }

        // Step 2. If the playback has ended and the direction of playback is forwards, seek to the
        // earliest possible position of the media resource.
        // Generally "ended" and "looping" are exclusive. Here, the loop attribute is ignored to
        // seek back to start in case loop was set after playback ended.
        // <https://github.com/whatwg/html/issues/4487>
        if self.ended_playback(LoopCondition::Ignored)
            && self.direction_of_playback() == PlaybackDirection::Forwards
        {
            self.seek(
                self.earliest_possible_position(),
                /* approximate_for_speed */ false,
            );
        }

        let state = self.ready_state.get();

        // Step 3. If the media element's paused attribute is true, then:
        if self.Paused() {
            // Step 3.1. Change the value of paused to false.
            self.paused.set(false);

            // Step 3.2. If the show poster flag is true, set the element's show poster flag to
            // false and run the time marches on steps.
            if self.show_poster.get() {
                self.show_poster.set(false);
                self.time_marches_on();
            }

            // Step 3.3. Queue a media element task given the media element to fire an event named
            // play at the element.
            self.queue_media_element_task_to_fire_event(atom!("play"));

            // Step 3.4. If the media element's readyState attribute has the value HAVE_NOTHING,
            // HAVE_METADATA, or HAVE_CURRENT_DATA, queue a media element task given the media
            // element to fire an event named waiting at the element. Otherwise, the media element's
            // readyState attribute has the value HAVE_FUTURE_DATA or HAVE_ENOUGH_DATA: notify about
            // playing for the element.
            match state {
                ReadyState::HaveNothing
                | ReadyState::HaveMetadata
                | ReadyState::HaveCurrentData => {
                    self.queue_media_element_task_to_fire_event(atom!("waiting"));
                },
                ReadyState::HaveFutureData | ReadyState::HaveEnoughData => {
                    self.notify_about_playing();
                },
            }
        }
        // Step 4. Otherwise, if the media element's readyState attribute has the value
        // HAVE_FUTURE_DATA or HAVE_ENOUGH_DATA, take pending play promises and queue a media
        // element task given the media element to resolve pending play promises with the
        // result.
        else if state == ReadyState::HaveFutureData || state == ReadyState::HaveEnoughData {
            self.take_pending_play_promises(Ok(()));

            let this = Trusted::new(self);
            let generation_id = self.generation_id.get();

            self.owner_global()
                .task_manager()
                .media_element_task_source()
                .queue(task!(resolve_pending_play_promises: move || {
                    let this = this.root();
                    if generation_id != this.generation_id.get() {
                        return;
                    }

                    this.fulfill_in_flight_play_promises(|| {});
                }));
        }

        // Step 5. Set the media element's can autoplay flag to false.
        self.autoplaying.set(false);

        self.update_media_state();
    }
    /// <https://html.spec.whatwg.org/multipage/#internal-pause-steps>
    pub(super) fn internal_pause_steps(&self) {
        // Step 1. Set the media element's can autoplay flag to false.
        self.autoplaying.set(false);

        // Step 2. If the media element's paused attribute is false, run the following steps:
        if !self.Paused() {
            // Step 2.1. Change the value of paused to true.
            self.paused.set(true);

            // Step 2.2. Take pending play promises and let promises be the result.
            self.take_pending_play_promises(Err(Error::Abort(None)));

            // Step 2.3. Queue a media element task given the media element and the following steps:
            let this = Trusted::new(self);
            let generation_id = self.generation_id.get();

            self.owner_global()
                .task_manager()
                .media_element_task_source()
                .queue(task!(internal_pause_steps: move || {
                    let this = this.root();
                    if generation_id != this.generation_id.get() {
                        return;
                    }

                    this.fulfill_in_flight_play_promises(|| {
                        // Step 2.3.1. Fire an event named timeupdate at the element.
                        this.upcast::<EventTarget>().fire_event(atom!("timeupdate"), CanGc::note());

                        // Step 2.3.2. Fire an event named pause at the element.
                        this.upcast::<EventTarget>().fire_event(atom!("pause"), CanGc::note());

                        // Step 2.3.3. Reject pending play promises with promises and an
                        // "AbortError" DOMException.
                        // Done after running this closure in `fulfill_in_flight_play_promises`.
                    });
                }));

            // Step 2.4. Set the official playback position to the current playback position.
            self.official_playback_position
                .set(self.current_playback_position.get());
        }

        self.update_media_state();
    }
    /// <https://html.spec.whatwg.org/multipage/#allowed-to-play>
    pub(super) fn is_allowed_to_play(&self) -> bool {
        true
    }
    /// <https://html.spec.whatwg.org/multipage/#notify-about-playing>
    pub(super) fn notify_about_playing(&self) {
        // Step 1. Take pending play promises and let promises be the result.
        self.take_pending_play_promises(Ok(()));

        // Step 2. Queue a media element task given the element and the following steps:
        let this = Trusted::new(self);
        let generation_id = self.generation_id.get();

        self.owner_global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(notify_about_playing: move || {
                let this = this.root();
                if generation_id != this.generation_id.get() {
                    return;
                }

                this.fulfill_in_flight_play_promises(|| {
                    // Step 2.1. Fire an event named playing at the element.
                    this.upcast::<EventTarget>().fire_event(atom!("playing"), CanGc::note());

                    // Step 2.2. Resolve pending play promises with promises.
                    // Done after running this closure in `fulfill_in_flight_play_promises`.
                });
            }));
    }
    /// <https://html.spec.whatwg.org/multipage/#ready-states>
    pub(super) fn change_ready_state(&self, ready_state: ReadyState) {
        let old_ready_state = self.ready_state.get();
        self.ready_state.set(ready_state);

        if self.network_state.get() == NetworkState::Empty {
            return;
        }

        if old_ready_state == ready_state {
            return;
        }

        // Step 1. Apply the first applicable set of substeps from the following list:
        match (old_ready_state, ready_state) {
            // => "If the previous ready state was HAVE_NOTHING, and the new ready state is
            // HAVE_METADATA"
            (ReadyState::HaveNothing, ReadyState::HaveMetadata) => {
                // Queue a media element task given the media element to fire an event named
                // loadedmetadata at the element.
                self.queue_media_element_task_to_fire_event(atom!("loadedmetadata"));
                // No other steps are applicable in this case.
                return;
            },
            // => "If the previous ready state was HAVE_METADATA and the new ready state is
            // HAVE_CURRENT_DATA or greater"
            (ReadyState::HaveMetadata, new) if new >= ReadyState::HaveCurrentData => {
                // If this is the first time this occurs for this media element since the load()
                // algorithm was last invoked, the user agent must queue a media element task given
                // the media element to fire an event named loadeddata at the element.
                if !self.fired_loadeddata_event.get() {
                    self.fired_loadeddata_event.set(true);

                    let this = Trusted::new(self);
                    let generation_id = self.generation_id.get();

                    self.owner_global()
                        .task_manager()
                        .media_element_task_source()
                        .queue(task!(media_reached_current_data: move |cx| {
                            let this = this.root();
                            if generation_id != this.generation_id.get() {
                                return;
                            }

                            this.upcast::<EventTarget>().fire_event(atom!("loadeddata"), CanGc::from_cx(cx));
                            // Once the readyState attribute reaches HAVE_CURRENT_DATA, after the
                            // loadeddata event has been fired, set the element's
                            // delaying-the-load-event flag to false.
                            this.delay_load_event(false, cx);
                        }));
                }

                // Steps for the transition from HaveMetadata to HaveCurrentData
                // or HaveFutureData also apply here, as per the next match
                // expression.
            },
            (ReadyState::HaveFutureData, new) if new <= ReadyState::HaveCurrentData => {
                // FIXME(nox): Queue a task to fire timeupdate and waiting
                // events if the conditions call from the spec are met.

                // No other steps are applicable in this case.
                return;
            },

            _ => (),
        }

        // => "If the previous ready state was HAVE_CURRENT_DATA or less, and the new ready state is
        // HAVE_FUTURE_DATA or more"
        if old_ready_state <= ReadyState::HaveCurrentData
            && ready_state >= ReadyState::HaveFutureData
        {
            // The user agent must queue a media element task given the media element to fire an
            // event named canplay at the element.
            self.queue_media_element_task_to_fire_event(atom!("canplay"));

            // If the element's paused attribute is false, the user agent must notify about playing
            // for the element.
            if !self.Paused() {
                self.notify_about_playing();
            }
        }

        // => "If the new ready state is HAVE_ENOUGH_DATA"
        if ready_state == ReadyState::HaveEnoughData {
            // The user agent must queue a media element task given the media element to fire an
            // event named canplaythrough at the element.
            self.queue_media_element_task_to_fire_event(atom!("canplaythrough"));

            // If the element is eligible for autoplay, then the user agent may run the following
            // substeps:
            if self.eligible_for_autoplay() {
                // Step 1. Set the paused attribute to false.
                self.paused.set(false);

                // Step 2. If the element's show poster flag is true, set it to false and run the
                // time marches on steps.
                if self.show_poster.get() {
                    self.show_poster.set(false);
                    self.time_marches_on();
                }

                // Step 3. Queue a media element task given the element to fire an event named play
                // at the element.
                self.queue_media_element_task_to_fire_event(atom!("play"));

                // Step 4. Notify about playing for the element.
                self.notify_about_playing();
            }
        }

        self.update_media_state();
    }
    /// <https://html.spec.whatwg.org/multipage/#eligible-for-autoplay>
    pub(super) fn eligible_for_autoplay(&self) -> bool {
        // its can autoplay flag is true;
        self.autoplaying.get() &&

        // its paused attribute is true;
        self.Paused() &&

        // it has an autoplay attribute specified;
        self.Autoplay() &&

        // its node document's active sandboxing flag set does not have the sandboxed automatic
        // features browsing context flag set; and
        {
            let document = self.owner_document();

            !document.has_active_sandboxing_flag(
                SandboxingFlagSet::SANDBOXED_AUTOMATIC_FEATURES_BROWSING_CONTEXT_FLAG,
            )
        }

        // its node document is allowed to use the "autoplay" feature.
        // TODO: Feature policy: https://html.spec.whatwg.org/iframe-embed-object.html#allowed-to-use
    }
    pub(super) fn in_error_state(&self) -> bool {
        self.error.get().is_some()
    }
    /// <https://html.spec.whatwg.org/multipage/#potentially-playing>
    pub(super) fn is_potentially_playing(&self) -> bool {
        !self.paused.get()
            && !self.ended_playback(LoopCondition::Included)
            && self.error.get().is_none()
            && !self.is_blocked_media_element()
    }
    /// <https://html.spec.whatwg.org/multipage/#blocked-media-element>
    pub(super) fn is_blocked_media_element(&self) -> bool {
        self.ready_state.get() <= ReadyState::HaveCurrentData
            || self.is_paused_for_user_interaction()
            || self.is_paused_for_in_band_content()
    }
    /// <https://html.spec.whatwg.org/multipage/#paused-for-user-interaction>
    pub(super) fn is_paused_for_user_interaction(&self) -> bool {
        // FIXME: we will likely be able to fill this placeholder once (if) we
        //        implement the MediaSession API.
        false
    }
    /// <https://html.spec.whatwg.org/multipage/#paused-for-in-band-content>
    pub(super) fn is_paused_for_in_band_content(&self) -> bool {
        // FIXME: we will likely be able to fill this placeholder once (if) we
        //        implement https://github.com/servo/servo/issues/22314
        false
    }
    /// <https://html.spec.whatwg.org/multipage/#media-element-load-algorithm>
    pub(super) fn media_element_load_algorithm(&self, cx: &mut js::context::JSContext) {
        // Reset the flag that signals whether loadeddata was ever fired for
        // this invokation of the load algorithm.
        self.fired_loadeddata_event.set(false);

        // TODO Step 1. Set this element's is currently stalled to false.

        // Step 2. Abort any already-running instance of the resource selection algorithm for this
        // element.
        self.generation_id.set(self.generation_id.get() + 1);

        self.load_state.set(LoadState::NotLoaded);
        *self.source_children_pointer.borrow_mut() = None;
        self.current_source_child.set(None);

        // Step 3. Let pending tasks be a list of all tasks from the media element's media element
        // event task source in one of the task queues.

        // Step 4. For each task in pending tasks that would resolve pending play promises or reject
        // pending play promises, immediately resolve or reject those promises in the order the
        // corresponding tasks were queued.
        while !self.in_flight_play_promises_queue.borrow().is_empty() {
            self.fulfill_in_flight_play_promises(|| ());
        }

        // Step 5. Remove each task in pending tasks from its task queue.
        // Note that each media element's pending event and callback is scheduled with associated
        // generation id and will be aborted eventually (from Step 2).

        let network_state = self.network_state.get();

        // Step 6. If the media element's networkState is set to NETWORK_LOADING or NETWORK_IDLE,
        // queue a media element task given the media element to fire an event named abort at the
        // media element.
        if network_state == NetworkState::Loading || network_state == NetworkState::Idle {
            self.queue_media_element_task_to_fire_event(atom!("abort"));
        }

        // Reset the media player for any previously playing media resource (see Step 11).
        self.reset_media_player();

        // Step 7. If the media element's networkState is not set to NETWORK_EMPTY, then:
        if network_state != NetworkState::Empty {
            // Step 7.1. Queue a media element task given the media element to fire an event named
            // emptied at the media element.
            self.queue_media_element_task_to_fire_event(atom!("emptied"));

            // Step 7.2. If a fetching process is in progress for the media element, the user agent
            // should stop it.
            if let Some(ref mut current_fetch_context) = *self.current_fetch_context.borrow_mut() {
                current_fetch_context.cancel(CancelReason::Abort);
            }

            let detached_media_source = self
                .attached_media_source
                .borrow_mut()
                .take()
                .map(|media_source| media_source.as_rooted());
            if let Some(media_source) = detached_media_source {
                media_source.detach_from_element(CanGc::from_cx(cx));
            }

            // Step 7.4. Forget the media element's media-resource-specific tracks.
            self.AudioTracks(CanGc::from_cx(cx)).clear();
            self.VideoTracks(CanGc::from_cx(cx)).clear();

            // Step 7.5. If readyState is not set to HAVE_NOTHING, then set it to that state.
            if self.ready_state.get() != ReadyState::HaveNothing {
                self.change_ready_state(ReadyState::HaveNothing);
            }

            // Step 7.6. If the paused attribute is false, then:
            if !self.Paused() {
                // Step 7.6.1. Set the paused attribute to true.
                self.paused.set(true);

                // Step 7.6.2. Take pending play promises and reject pending play promises with the
                // result and an "AbortError" DOMException.
                self.take_pending_play_promises(Err(Error::Abort(None)));
                self.fulfill_in_flight_play_promises(|| ());
            }

            // Step 7.7. If seeking is true, set it to false.
            self.seeking.set(false);

            self.current_seek_position.set(f64::NAN);

            // Step 7.8. Set the current playback position to 0.
            // Set the official playback position to 0.
            // If this changed the official playback position, then queue a media element task given
            // the media element to fire an event named timeupdate at the media element.
            self.current_playback_position.set(0.);
            if self.official_playback_position.get() != 0. {
                self.queue_media_element_task_to_fire_event(atom!("timeupdate"));
            }
            self.official_playback_position.set(0.);

            // TODO Step 7.9. Set the timeline offset to Not-a-Number (NaN).

            // Step 7.10. Update the duration attribute to Not-a-Number (NaN).
            self.duration.set(f64::NAN);
        }

        // Step 8. Set the playbackRate attribute to the value of the defaultPlaybackRate attribute.
        self.playback_rate.set(self.default_playback_rate.get());

        // Step 9. Set the error attribute to null and the can autoplay flag to true.
        self.error.set(None);
        self.autoplaying.set(true);

        // Step 10. Invoke the media element's resource selection algorithm.
        self.invoke_resource_selection_algorithm(cx);

        // Step 11. Note: Playback of any previously playing media resource for this element stops.
    }
    /// Queue a media element task given the media element to fire an event at the media element.
    /// <https://html.spec.whatwg.org/multipage/#queue-a-media-element-task>
    pub(super) fn queue_media_element_task_to_fire_event(&self, name: Atom) {
        let this = Trusted::new(self);
        let generation_id = self.generation_id.get();

        self.owner_global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(queue_event: move |cx| {
                let this = this.root();
                if generation_id != this.generation_id.get() {
                    return;
                }

                this.upcast::<EventTarget>().fire_event(name, CanGc::from_cx(cx));
            }));
    }
    /// Appends a promise to the list of pending play promises.
    pub(super) fn push_pending_play_promise(&self, promise: &Rc<Promise>) {
        self.pending_play_promises
            .borrow_mut()
            .push(promise.clone());
    }
    /// Takes the pending play promises.
    ///
    /// The result with which these promises will be fulfilled is passed here
    /// and this method returns nothing because we actually just move the
    /// current list of pending play promises to the
    /// `in_flight_play_promises_queue` field.
    ///
    /// Each call to this method must be followed by a call to
    /// `fulfill_in_flight_play_promises`, to actually fulfill the promises
    /// which were taken and moved to the in-flight queue.
    pub(super) fn take_pending_play_promises(&self, result: ErrorResult) {
        let pending_play_promises = std::mem::take(&mut *self.pending_play_promises.borrow_mut());
        self.in_flight_play_promises_queue
            .borrow_mut()
            .push_back((pending_play_promises.into(), result));
    }
    /// Fulfills the next in-flight play promises queue after running a closure.
    ///
    /// See the comment on `take_pending_play_promises` for why this method
    /// does not take a list of promises to fulfill. Callers cannot just pop
    /// the front list off of `in_flight_play_promises_queue` and later fulfill
    /// the promises because that would mean putting
    /// `#[cfg_attr(crown, expect(crown::unrooted_must_root))]` on even more functions, potentially
    /// hiding actual safety bugs.
    pub(super) fn fulfill_in_flight_play_promises<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        let (promises, result) = self
            .in_flight_play_promises_queue
            .borrow_mut()
            .pop_front()
            .expect("there should be at least one list of in flight play promises");
        f();
        for promise in &*promises {
            match result {
                Ok(ref value) => promise.resolve_native(value, CanGc::note()),
                Err(ref error) => promise.reject_error(error.clone(), CanGc::note()),
            }
        }
    }
    /// <https://html.spec.whatwg.org/multipage/#dom-media-seek>
    pub(super) fn seek(&self, time: f64, _approximate_for_speed: bool) {
        // Step 1. Set the media element's show poster flag to false.
        self.show_poster.set(false);

        // Step 2. If the media element's readyState is HAVE_NOTHING, return.
        if self.ready_state.get() == ReadyState::HaveNothing {
            return;
        }

        // Step 3. If the element's seeking IDL attribute is true, then another instance of this
        // algorithm is already running. Abort that other instance of the algorithm without waiting
        // for the step that it is running to complete.
        self.current_seek_position.set(f64::NAN);

        // Step 4. Set the seeking IDL attribute to true.
        self.seeking.set(true);

        // Step 5. If the seek was in response to a DOM method call or setting of an IDL attribute,
        // then continue the script. The remainder of these steps must be run in parallel.

        // Step 6. If the new playback position is later than the end of the media resource, then
        // let it be the end of the media resource instead.
        let time = f64::min(time, self.Duration());

        // Step 7. If the new playback position is less than the earliest possible position, let it
        // be that position instead.
        let time = f64::max(time, self.earliest_possible_position());

        // Step 8. If the (possibly now changed) new playback position is not in one of the ranges
        // given in the seekable attribute, then let it be the position in one of the ranges given
        // in the seekable attribute that is the nearest to the new playback position. If there are
        // no ranges given in the seekable attribute, then set the seeking IDL attribute to false
        // and return.
        let seekable = self.seekable();

        if seekable.is_empty() {
            self.seeking.set(false);
            return;
        }

        let mut nearest_seekable_position = 0.0;
        let mut in_seekable_range = false;
        let mut nearest_seekable_distance = f64::MAX;
        for i in 0..seekable.len() {
            let start = seekable.start(i).unwrap().abs();
            let end = seekable.end(i).unwrap().abs();
            if time >= start && time <= end {
                nearest_seekable_position = time;
                in_seekable_range = true;
                break;
            } else if time < start {
                let distance = start - time;
                if distance < nearest_seekable_distance {
                    nearest_seekable_distance = distance;
                    nearest_seekable_position = start;
                }
            } else {
                let distance = time - end;
                if distance < nearest_seekable_distance {
                    nearest_seekable_distance = distance;
                    nearest_seekable_position = end;
                }
            }
        }
        let time = if in_seekable_range {
            time
        } else {
            nearest_seekable_position
        };

        // Step 9. If the approximate-for-speed flag is set, adjust the new playback position to a
        // value that will allow for playback to resume promptly. If new playback position before
        // this step is before current playback position, then the adjusted new playback position
        // must also be before the current playback position. Similarly, if the new playback
        // position before this step is after current playback position, then the adjusted new
        // playback position must also be after the current playback position.
        // TODO: Note that servo-media with gstreamer does not support inaccurate seeking for now.

        // Step 10. Queue a media element task given the media element to fire an event named
        // seeking at the element.
        self.queue_media_element_task_to_fire_event(atom!("seeking"));

        // Step 11. Set the current playback position to the new playback position.
        self.current_playback_position.set(time);

        if let Some(ref mc) = *self.media_controller.borrow() {
            mc.seek((time * 1000.0) as u64);
        }

        self.current_seek_position.set(time);

        // Step 12. Wait until the user agent has established whether or not the media data for the
        // new playback position is available, and, if it is, until it has decoded enough data to
        // play back that position.
        // The rest of the steps are handled when the media engine signals a ready state change or
        // otherwise satisfies seek completion and signals a position change.
    }
    /// <https://html.spec.whatwg.org/multipage/#direction-of-playback>
    pub(super) fn direction_of_playback(&self) -> PlaybackDirection {
        // If the element's playbackRate is positive or zero, then the direction of playback is
        // forwards. Otherwise, it is backwards.
        if self.playback_rate.get() >= 0. {
            PlaybackDirection::Forwards
        } else {
            PlaybackDirection::Backwards
        }
    }
    /// <https://html.spec.whatwg.org/multipage/#ended-playback>
    pub(super) fn ended_playback(&self, loop_condition: LoopCondition) -> bool {
        // A media element is said to have ended playback when:

        // The element's readyState attribute is HAVE_METADATA or greater, and
        if self.ready_state.get() < ReadyState::HaveMetadata {
            return false;
        }

        let playback_position = self.current_playback_position.get();

        match self.direction_of_playback() {
            // Either: The current playback position is the end of the media resource, and the
            // direction of playback is forwards, and the media element does not have a loop
            // attribute specified.
            PlaybackDirection::Forwards => {
                playback_position >= self.Duration()
                    && (loop_condition == LoopCondition::Ignored || !self.Loop())
            },
            // Or: The current playback position is the earliest possible position, and the
            // direction of playback is backwards.
            PlaybackDirection::Backwards => playback_position <= self.earliest_possible_position(),
        }
    }
    /// <https://html.spec.whatwg.org/multipage/#reaches-the-end>
    pub(super) fn end_of_playback_in_forwards_direction(&self) {
        // When the current playback position reaches the end of the media resource when the
        // direction of playback is forwards, then the user agent must follow these steps:

        // Step 1. If the media element has a loop attribute specified, then seek to the earliest
        // posible position of the media resource and return.
        if self.Loop() {
            self.seek(
                self.earliest_possible_position(),
                /* approximate_for_speed */ false,
            );
            return;
        }

        // Step 2. As defined above, the ended IDL attribute starts returning true once the event
        // loop returns to step 1.

        // Step 3. Queue a media element task given the media element and the following steps:
        let this = Trusted::new(self);
        let generation_id = self.generation_id.get();

        self.owner_global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(reaches_the_end_steps: move || {
                let this = this.root();
                if generation_id != this.generation_id.get() {
                    return;
                }

                // Step 3.1. Fire an event named timeupdate at the media element.
                this.upcast::<EventTarget>().fire_event(atom!("timeupdate"), CanGc::note());

                // Step 3.2. If the media element has ended playback, the direction of playback is
                // forwards, and paused is false, then:
                if this.ended_playback(LoopCondition::Included) &&
                    this.direction_of_playback() == PlaybackDirection::Forwards &&
                    !this.Paused() {
                    // Step 3.2.1. Set the paused attribute to true.
                    this.paused.set(true);

                    // Step 3.2.2. Fire an event named pause at the media element.
                    this.upcast::<EventTarget>().fire_event(atom!("pause"), CanGc::note());

                    // Step 3.2.3. Take pending play promises and reject pending play promises with
                    // the result and an "AbortError" DOMException.
                    this.take_pending_play_promises(Err(Error::Abort(None)));
                    this.fulfill_in_flight_play_promises(|| ());
                }

                // Step 3.3. Fire an event named ended at the media element.
                this.upcast::<EventTarget>().fire_event(atom!("ended"), CanGc::note());
            }));

        // <https://html.spec.whatwg.org/multipage/#dom-media-have_current_data>
        self.change_ready_state(ReadyState::HaveCurrentData);
    }
    /// <https://html.spec.whatwg.org/multipage/#reaches-the-end>
    pub(super) fn end_of_playback_in_backwards_direction(&self) {
        // When the current playback position reaches the earliest possible position of the media
        // resource when the direction of playback is backwards, then the user agent must only queue
        // a media element task given the media element to fire an event named timeupdate at the
        // element.
        if self.current_playback_position.get() <= self.earliest_possible_position() {
            self.queue_media_element_task_to_fire_event(atom!("timeupdate"));
        }
    }
    pub(super) fn playback_end(&self) {
        // Abort the following steps of the end of playback if seeking is in progress.
        if self.seeking.get() {
            return;
        }

        match self.direction_of_playback() {
            PlaybackDirection::Forwards => self.end_of_playback_in_forwards_direction(),
            PlaybackDirection::Backwards => self.end_of_playback_in_backwards_direction(),
        }
    }
    pub(super) fn playback_error(&self, error: &str, cx: &mut js::context::JSContext) {
        error!("Player error: {:?}", error);

        // If we have already flagged an error condition while processing
        // the network response, we should silently skip any observable
        // errors originating while decoding the erroneous response.
        if self.in_error_state() {
            return;
        }

        // <https://html.spec.whatwg.org/multipage/#media-data-processing-steps-list>
        if self.ready_state.get() == ReadyState::HaveNothing {
            // => "If the media data can be fetched but is found by inspection to be in an
            // unsupported format, or can otherwise not be rendered at all"
            self.media_data_processing_failure_steps();
        } else {
            // => "If the media data is corrupted"
            self.media_data_processing_fatal_steps(MEDIA_ERR_DECODE, cx);
        }
    }
    pub(super) fn playback_position_changed(&self, position: f64) {
        // Abort the following steps of the current time update if seeking is in progress.
        if self.seeking.get() {
            return;
        }

        let _ = self
            .played
            .borrow_mut()
            .add(self.current_playback_position.get(), position);
        self.current_playback_position.set(position);
        self.official_playback_position.set(position);
        self.time_marches_on();

        let media_position_state =
            MediaPositionState::new(self.duration.get(), self.playback_rate.get(), position);
        debug!(
            "Sending media session event set position state {:?}",
            media_position_state
        );
        self.send_media_session_event(MediaSessionEvent::SetPositionState(media_position_state));
    }
    pub(super) fn seekable(&self) -> TimeRangesContainer {
        let mut seekable = TimeRangesContainer::default();
        if let Some(ref mc) = *self.media_controller.borrow() {
            for &(start, end) in &mc.seekable_ranges {
                let _ = seekable.add(start, end);
            }
        }
        seekable
    }
    /// <https://html.spec.whatwg.org/multipage/#earliest-possible-position>
    pub(super) fn earliest_possible_position(&self) -> f64 {
        self.seekable()
            .start(0)
            .unwrap_or_else(|_| self.current_playback_position.get())
    }
}
