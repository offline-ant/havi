/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Screenshot support wired through a shared screenshot bridge.

use std::cell::RefCell;
use base::id::WebViewId;
use embedder_traits::ScreenshotCaptureError;
use image::RgbaImage;
use webrender_api::units::DeviceRect;

use crate::src_bridge::{PendingScreenshot, PendingScreenshotMap, ScreenshotBridge};

#[derive(Default)]
pub(crate) struct ScreenshotTaker {
    requests: RefCell<PendingScreenshotMap>,
}

impl ScreenshotTaker {
    pub(crate) fn request_screenshot(
        &self,
        bridge: &ScreenshotBridge,
        webview_id: WebViewId,
        rect: Option<DeviceRect>,
        callback: Box<dyn FnOnce(Result<RgbaImage, ScreenshotCaptureError>) + 'static>,
    ) -> u64 {
        let request_id = bridge.allocate_request_id();
        self.requests.borrow_mut().insert(
            request_id,
            PendingScreenshot {
                request_id,
                webview_id,
                rect,
                callback,
            },
        );
        request_id
    }

    pub(crate) fn fulfill_completed(&self, bridge: &ScreenshotBridge) {
        for result in bridge.drain_results() {
            let Some(request) = self.requests.borrow_mut().remove(&result.request_id) else {
                continue;
            };
            let image = if let Some(rect) = request.rect {
                let min_x = rect.min.x.max(0.0).floor() as u32;
                let min_y = rect.min.y.max(0.0).floor() as u32;
                let max_x = rect.max.x.ceil().max(rect.min.x).min(result.image.width() as f32) as u32;
                let max_y = rect.max.y.ceil().max(rect.min.y).min(result.image.height() as f32) as u32;
                let width = max_x.saturating_sub(min_x);
                let height = max_y.saturating_sub(min_y);
                if width == 0 || height == 0 {
                    (request.callback)(Err(ScreenshotCaptureError::CouldNotReadImage));
                    continue;
                }
                image::imageops::crop_imm(&result.image, min_x, min_y, width, height).to_image()
            } else {
                result.image
            };
            (request.callback)(Ok(image));
        }
    }

    pub(crate) fn fail_webview(&self, webview_id: WebViewId) {
        let mut requests = self.requests.borrow_mut();
        let ids: Vec<u64> = requests
            .iter()
            .filter_map(|(id, request)| (request.webview_id == webview_id).then_some(*id))
            .collect();
        for id in ids {
            if let Some(request) = requests.remove(&id) {
                (request.callback)(Err(ScreenshotCaptureError::WebViewDoesNotExist));
            }
        }
    }
}
