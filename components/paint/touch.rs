/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Touch-to-scroll conversion. Converts touch start/move/end sequences into
//! scroll events that flow through Servo's normal scroll path (same as mouse
//! wheel). No Makepad-side scroll state is maintained — all scroll state lives
//! in Servo's layout.

use std::collections::HashMap;

use base::id::WebViewId;
use embedder_traits::{Scroll, TouchEvent, TouchEventType, WebViewPoint, WebViewVector};
use webrender_api::units::DeviceVector2D;

use crate::paint::Paint;

/// Per-touch-point tracking state.
struct ActiveTouch {
    /// Last known position of this touch point.
    point: WebViewPoint,
}

/// Converts touch gestures into scroll events.
///
/// Single-finger drag produces scroll deltas. Multi-touch is ignored for
/// scrolling (pinch zoom is handled separately).
pub(crate) struct TouchHandler {
    /// Currently active touch points, keyed by `TouchId.0`.
    active: HashMap<i32, ActiveTouch>,
}

impl TouchHandler {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    /// Process a touch event. Returns a scroll event (delta + point) if this
    /// touch should produce scrolling.
    pub fn on_touch_event(&mut self, event: TouchEvent) -> Option<(Scroll, WebViewPoint)> {
        let id = event.touch_id.0;
        match event.event_type {
            TouchEventType::Down => {
                self.active.insert(id, ActiveTouch { point: event.point });
                None
            },
            TouchEventType::Move => {
                // Only scroll for single-finger drag.
                if self.active.len() != 1 {
                    if let Some(touch) = self.active.get_mut(&id) {
                        touch.point = event.point;
                    }
                    return None;
                }

                let touch = self.active.get_mut(&id)?;
                let delta = point_delta(touch.point, event.point);
                touch.point = event.point;

                // Scroll delta is inverted: dragging down means content moves
                // up, so the scroll offset decreases (negative delta).
                let scroll = Scroll::Delta(WebViewVector::Device(DeviceVector2D::new(
                    -delta.x, -delta.y,
                )));
                Some((scroll, event.point))
            },
            TouchEventType::Up | TouchEventType::Cancel => {
                self.active.remove(&id);
                None
            },
        }
    }
}

/// Compute the pixel delta between two `WebViewPoint`s. Both must use the same
/// coordinate space; in practice touch events from Makepad are always in device
/// pixels.
fn point_delta(from: WebViewPoint, to: WebViewPoint) -> DeviceVector2D {
    match (from, to) {
        (WebViewPoint::Device(a), WebViewPoint::Device(b)) => {
            DeviceVector2D::new(b.x - a.x, b.y - a.y)
        },
        (WebViewPoint::Page(a), WebViewPoint::Page(b)) => {
            // Treat page-pixel values as device pixels for delta purposes.
            DeviceVector2D::new(b.x - a.x, b.y - a.y)
        },
        // Mixed coordinate spaces — should not happen in practice.
        _ => DeviceVector2D::zero(),
    }
}

impl Paint {
    /// Handle a touch event, converting it to scroll if appropriate.
    pub fn on_touch_event(&self, webview_id: WebViewId, event: TouchEvent) {
        if let Some((scroll, point)) = self.touch_handler.borrow_mut().on_touch_event(event) {
            self.notify_scroll_event(webview_id, scroll, point);
        }
    }
}
