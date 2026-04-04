/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::thread::{self, JoinHandle};
use std::time::Duration;

use base::id::{PipelineId, WebViewId};
use constellation_traits::EmbedderToConstellationMessage;
use crossbeam_channel::{Sender, select, unbounded};
use embedder_traits::{AnimationState, EventLoopWaker};
use log::warn;
use rustc_hash::FxHashMap;

const ANIMATION_FRAME_DURATION: Duration = Duration::from_millis(16);

enum AnimationTickDriverCommand {
    SetAnimatingWebviews(Vec<WebViewId>),
    Quit,
}

pub(crate) struct AnimationTickDriver {
    sender: Sender<AnimationTickDriverCommand>,
    join_handle: Option<JoinHandle<()>>,
}

impl AnimationTickDriver {
    pub(crate) fn new(
        constellation_sender: Sender<EmbedderToConstellationMessage>,
        event_loop_waker: Box<dyn EventLoopWaker>,
    ) -> Self {
        let (sender, receiver) = unbounded::<AnimationTickDriverCommand>();
        let join_handle = thread::Builder::new()
            .name("PaintAnimationTick".to_string())
            .spawn(move || {
                let mut animating_webviews = Vec::new();

                loop {
                    if animating_webviews.is_empty() {
                        match receiver.recv() {
                            Ok(AnimationTickDriverCommand::SetAnimatingWebviews(webviews)) => {
                                animating_webviews = webviews;
                            }
                            Ok(AnimationTickDriverCommand::Quit) | Err(_) => return,
                        }
                        continue;
                    }

                    select! {
                        recv(receiver) -> message => match message {
                            Ok(AnimationTickDriverCommand::SetAnimatingWebviews(webviews)) => {
                                animating_webviews = webviews;
                            }
                            Ok(AnimationTickDriverCommand::Quit) | Err(_) => return,
                        },
                        default(ANIMATION_FRAME_DURATION) => {
                            if animating_webviews.is_empty() {
                                continue;
                            }
                            if let Err(error) = constellation_sender.send(
                                EmbedderToConstellationMessage::TickAnimation(animating_webviews.clone()),
                            ) {
                                warn!("Sending tick to constellation failed ({error:?}).");
                                return;
                            }
                            event_loop_waker.wake();
                        }
                    }
                }
            })
            .expect("Could not create paint animation tick thread.");

        Self {
            sender,
            join_handle: Some(join_handle),
        }
    }

    pub(crate) fn set_animating_webviews(&self, webviews: Vec<WebViewId>) {
        let _ = self
            .sender
            .send(AnimationTickDriverCommand::SetAnimatingWebviews(webviews));
    }

    pub(crate) fn shutdown(&mut self) {
        let _ = self.sender.send(AnimationTickDriverCommand::Quit);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AnimationUpdate {
    pub(crate) started_animating: bool,
    pub(crate) is_animating: bool,
}

#[derive(Default)]
pub(crate) struct AnimationStateTracker {
    webviews: FxHashMap<WebViewId, WebViewAnimationState>,
}

impl AnimationStateTracker {
    pub(crate) fn change_running_animations_state(
        &mut self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
        animation_state: AnimationState,
    ) -> AnimationUpdate {
        let update = self
            .webviews
            .entry(webview_id)
            .or_default()
            .change_pipeline_running_animations_state(pipeline_id, animation_state);
        self.remove_webview_if_idle(webview_id);
        update
    }

    pub(crate) fn set_throttled(
        &mut self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
        throttled: bool,
    ) -> AnimationUpdate {
        let update = self
            .webviews
            .entry(webview_id)
            .or_default()
            .set_throttled(pipeline_id, throttled);
        self.remove_webview_if_idle(webview_id);
        update
    }

    pub(crate) fn remove_pipeline(
        &mut self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
    ) -> AnimationUpdate {
        let Some(webview) = self.webviews.get_mut(&webview_id) else {
            return AnimationUpdate::default();
        };
        let update = webview.remove_pipeline(pipeline_id);
        self.remove_webview_if_idle(webview_id);
        update
    }

    pub(crate) fn remove_webview(&mut self, webview_id: WebViewId) {
        self.webviews.remove(&webview_id);
    }

    pub(crate) fn animating_webviews(&self) -> Vec<WebViewId> {
        self.webviews
            .iter()
            .filter_map(|(webview_id, webview)| {
                if webview.animating {
                    Some(*webview_id)
                } else {
                    None
                }
            })
            .collect()
    }

    fn remove_webview_if_idle(&mut self, webview_id: WebViewId) {
        if self
            .webviews
            .get(&webview_id)
            .is_some_and(WebViewAnimationState::is_empty)
        {
            self.webviews.remove(&webview_id);
        }
    }
}

#[derive(Default)]
struct WebViewAnimationState {
    pipelines: FxHashMap<PipelineId, PipelineAnimationState>,
    animating: bool,
}

impl WebViewAnimationState {
    fn change_pipeline_running_animations_state(
        &mut self,
        pipeline_id: PipelineId,
        animation_state: AnimationState,
    ) -> AnimationUpdate {
        let pipeline = self.pipelines.entry(pipeline_id).or_default();
        let was_animating = pipeline.animating();
        match animation_state {
            AnimationState::AnimationsPresent => {
                pipeline.animations_running = true;
            }
            AnimationState::AnimationCallbacksPresent => {
                pipeline.animation_callbacks_running = true;
            }
            AnimationState::NoAnimationsPresent => {
                pipeline.animations_running = false;
            }
            AnimationState::NoAnimationCallbacksPresent => {
                pipeline.animation_callbacks_running = false;
            }
        }
        let started_animating = !was_animating && pipeline.animating();
        self.update_animation_state();
        AnimationUpdate {
            started_animating,
            is_animating: self.animating,
        }
    }

    fn set_throttled(&mut self, pipeline_id: PipelineId, throttled: bool) -> AnimationUpdate {
        let pipeline = self.pipelines.entry(pipeline_id).or_default();
        let was_animating = pipeline.animating();
        pipeline.throttled = throttled;
        let started_animating = !was_animating && pipeline.animating();
        self.update_animation_state();
        AnimationUpdate {
            started_animating,
            is_animating: self.animating,
        }
    }

    fn remove_pipeline(&mut self, pipeline_id: PipelineId) -> AnimationUpdate {
        self.pipelines.remove(&pipeline_id);
        self.update_animation_state();
        AnimationUpdate {
            started_animating: false,
            is_animating: self.animating,
        }
    }

    fn update_animation_state(&mut self) {
        self.animating = self.pipelines.values().any(PipelineAnimationState::animating);
    }

    fn is_empty(&self) -> bool {
        self.pipelines.is_empty() && !self.animating
    }
}

#[derive(Default)]
struct PipelineAnimationState {
    animations_running: bool,
    animation_callbacks_running: bool,
    throttled: bool,
}

impl PipelineAnimationState {
    fn animating(&self) -> bool {
        !self.throttled && (self.animations_running || self.animation_callbacks_running)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use base::id::{TEST_PIPELINE_ID, TEST_WEBVIEW_ID};
    use crossbeam_channel::unbounded;

    #[derive(Clone)]
    struct DummyEventLoopWaker(Arc<AtomicUsize>);

    impl EventLoopWaker for DummyEventLoopWaker {
        fn clone_box(&self) -> Box<dyn EventLoopWaker> {
            Box::new(self.clone())
        }

        fn wake(&self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn tracker_marks_webview_animating_for_css_animation_state() {
        let mut tracker = AnimationStateTracker::default();

        let update = tracker.change_running_animations_state(
            TEST_WEBVIEW_ID,
            TEST_PIPELINE_ID,
            AnimationState::AnimationsPresent,
        );

        assert_eq!(
            update,
            AnimationUpdate {
                started_animating: true,
                is_animating: true,
            }
        );
        assert_eq!(tracker.animating_webviews(), vec![TEST_WEBVIEW_ID]);
    }

    #[test]
    fn tracker_clears_webview_when_animation_state_ends() {
        let mut tracker = AnimationStateTracker::default();
        tracker.change_running_animations_state(
            TEST_WEBVIEW_ID,
            TEST_PIPELINE_ID,
            AnimationState::AnimationsPresent,
        );

        let update = tracker.change_running_animations_state(
            TEST_WEBVIEW_ID,
            TEST_PIPELINE_ID,
            AnimationState::NoAnimationsPresent,
        );

        assert_eq!(
            update,
            AnimationUpdate {
                started_animating: false,
                is_animating: false,
            }
        );
        assert!(tracker.animating_webviews().is_empty());
    }

    #[test]
    fn throttling_stops_animation_without_losing_state() {
        let mut tracker = AnimationStateTracker::default();
        tracker.change_running_animations_state(
            TEST_WEBVIEW_ID,
            TEST_PIPELINE_ID,
            AnimationState::AnimationsPresent,
        );

        let throttled = tracker.set_throttled(TEST_WEBVIEW_ID, TEST_PIPELINE_ID, true);
        assert_eq!(
            throttled,
            AnimationUpdate {
                started_animating: false,
                is_animating: false,
            }
        );
        assert!(tracker.animating_webviews().is_empty());

        let unthrottled = tracker.set_throttled(TEST_WEBVIEW_ID, TEST_PIPELINE_ID, false);
        assert_eq!(
            unthrottled,
            AnimationUpdate {
                started_animating: true,
                is_animating: true,
            }
        );
        assert_eq!(tracker.animating_webviews(), vec![TEST_WEBVIEW_ID]);
    }

    #[test]
    fn removing_pipeline_clears_animation_state() {
        let mut tracker = AnimationStateTracker::default();
        tracker.change_running_animations_state(
            TEST_WEBVIEW_ID,
            TEST_PIPELINE_ID,
            AnimationState::AnimationCallbacksPresent,
        );

        let update = tracker.remove_pipeline(TEST_WEBVIEW_ID, TEST_PIPELINE_ID);

        assert_eq!(
            update,
            AnimationUpdate {
                started_animating: false,
                is_animating: false,
            }
        );
        assert!(tracker.animating_webviews().is_empty());
    }

    #[test]
    fn animation_tick_driver_sends_tick_messages() {
        let (sender, receiver) = unbounded();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let mut driver = AnimationTickDriver::new(
            sender,
            Box::new(DummyEventLoopWaker(wake_count.clone())),
        );

        driver.set_animating_webviews(vec![TEST_WEBVIEW_ID]);

        let message = receiver
            .recv_timeout(Duration::from_millis(250))
            .expect("expected animation tick");
        match message {
            EmbedderToConstellationMessage::TickAnimation(webviews) => {
                assert_eq!(webviews, vec![TEST_WEBVIEW_ID]);
            }
            _ => panic!("unexpected animation tick message"),
        }
        assert!(wake_count.load(Ordering::Relaxed) > 0);

        driver.shutdown();
    }
}
