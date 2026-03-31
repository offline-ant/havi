/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Touch-to-scroll conversion. Converts touch start/move/end sequences into
//! browser scroll deltas routed back to the embedder's BrowserScrollController.
//! TouchHandler recognizes gestures only. It does not own browser scroll state.

use std::collections::HashMap;

use base::id::WebViewId;
use embedder_traits::{TouchEvent, TouchEventType, WebViewPoint};
use euclid::Point2D;
use webrender_api::units::{DeviceVector2D, LayoutVector2D};

use super::paint::Paint;

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

    /// Process a touch event. Returns a point + scroll delta when this touch
    /// should produce browser scrolling.
    pub fn on_touch_event(&mut self, event: TouchEvent) -> Option<(WebViewPoint, DeviceVector2D)> {
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
                Some((
                    event.point,
                    DeviceVector2D::new(-delta.x, -delta.y),
                ))
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
    /// Handle a touch event, converting it to browser scroll if appropriate.
    pub fn on_touch_event(&self, webview_id: WebViewId, event: TouchEvent) {
        if let Some((point, delta)) = self.touch_handler.borrow_mut().on_touch_event(event) {
            let dpp = self.device_pixels_per_page_pixel(webview_id);
            let point = match point {
                WebViewPoint::Device(point) => point / dpp,
                WebViewPoint::Page(point) => point,
            };
            let delta = LayoutVector2D::new(delta.x / dpp.get(), delta.y / dpp.get());
            self.notify_scroll_default_action(
                webview_id,
                Some(Point2D::new(point.x, point.y)),
                delta,
            );
        }
    }
}
